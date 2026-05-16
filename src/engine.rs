use anyhow::{Result, anyhow};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

use crate::config::{AppConfig, RunMode, SidePreference};
use crate::effects::Side;
use crate::events::{UiEvent, UiKind};
use crate::runtime::{DemoEvent, RuntimeHandle};

#[derive(Clone)]
pub struct Engine {
    config: Arc<RwLock<AppConfig>>,
    config_dir: Arc<RwLock<Option<PathBuf>>>,
    assets_root: Arc<RwLock<PathBuf>>,
    runtime: Arc<Mutex<Option<RuntimeHandle>>>,
    starting: Arc<AtomicBool>,
    ui_tx: broadcast::Sender<UiEvent>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub running: bool,
    pub mode: Option<RunMode>,
}

impl Engine {
    pub fn new(config: AppConfig) -> Self {
        let (ui_tx, _) = broadcast::channel(128);
        Self {
            config: Arc::new(RwLock::new(config)),
            config_dir: Arc::new(RwLock::new(None)),
            assets_root: Arc::new(RwLock::new(default_assets_root())),
            runtime: Arc::new(Mutex::new(None)),
            starting: Arc::new(AtomicBool::new(false)),
            ui_tx,
        }
    }

    pub fn subscribe_ui(&self) -> broadcast::Receiver<UiEvent> {
        self.ui_tx.subscribe()
    }

    pub fn set_assets_root(&self, assets_root: PathBuf) {
        *self.assets_root.write() = assets_root;
    }

    pub fn load_config_from(&self, config_dir: PathBuf) -> Result<()> {
        let config = AppConfig::load_from(&config_dir)?;
        *self.config.write() = config;
        *self.config_dir.write() = Some(config_dir.clone());
        self.emit(UiEvent::info(
            UiKind::Config,
            format!("配置已加载: {}", config_dir.join("config.json").display()),
        ));
        Ok(())
    }

    pub fn emit(&self, event: UiEvent) {
        let _ = self.ui_tx.send(event);
    }

    pub fn config_snapshot(&self) -> AppConfig {
        self.config.read().clone()
    }

    fn persist_config(&self, config: &AppConfig) -> Result<()> {
        if let Some(config_dir) = self.config_dir.read().clone() {
            let path = config.save_to(&config_dir)?;
            tracing::info!(path = %path.display(), "配置已保存");
        }
        Ok(())
    }

    pub fn update_config(&self, mut config: AppConfig) -> Result<AppConfig> {
        config.normalize();
        let running = self.is_running();
        let snapshot = if running {
            let current = self.config.read();
            if current.mode != config.mode
                || current.port != config.port
                || current.width != config.width
                || current.height != config.height
                || current.vsync != config.vsync
            {
                return Err(anyhow!(
                    "运行中只能调整语言、阵营选择、图标位置、图标大小、音效音量和连杀重置时间"
                ));
            }
            let mut snapshot = current.clone();
            snapshot.lang = config.lang;
            snapshot.side = config.side;
            snapshot.icon_scale = config.icon_scale;
            snapshot.icon_x = config.icon_x;
            snapshot.icon_y = config.icon_y;
            snapshot.volume = config.volume;
            snapshot.icon_scales = config.icon_scales;
            snapshot.kill_streak_reset_seconds = config.kill_streak_reset_seconds;
            snapshot
        } else {
            config
        };

        self.persist_config(&snapshot)?;
        *self.config.write() = snapshot.clone();
        self.emit(UiEvent::info(
            UiKind::Config,
            if running {
                "实时配置已更新"
            } else {
                "配置已更新"
            },
        ));
        Ok(snapshot)
    }

    pub fn status(&self) -> RuntimeStatus {
        self.clear_finished();
        let guard = self.runtime.lock();
        let handle = guard.as_ref();
        RuntimeStatus {
            running: handle.is_some_and(RuntimeHandle::is_running),
            mode: handle.map(RuntimeHandle::mode),
        }
    }

    pub fn is_running(&self) -> bool {
        self.status().running
    }

    pub async fn start(&self) -> Result<()> {
        if self
            .starting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(anyhow!("CounterFire 2 正在启动"));
        }
        let result = self.start_inner().await;
        self.starting.store(false, Ordering::SeqCst);
        result
    }

    async fn start_inner(&self) -> Result<()> {
        self.clear_finished();
        {
            let guard = self.runtime.lock();
            if guard.as_ref().is_some_and(RuntimeHandle::is_running) {
                return Err(anyhow!("CounterFire 2 已在运行"));
            }
        }

        let config = self.config_snapshot();
        let mode = config.mode;
        let assets_root = self.assets_root.read().clone();
        self.emit(UiEvent::info(
            UiKind::Runtime,
            match mode {
                RunMode::Gsi => "正在启动 CS2 GSI 模式",
                RunMode::Demo => "正在启动 Demo 模式",
            },
        ));
        let handle = RuntimeHandle::spawn(
            config,
            Arc::clone(&self.config),
            assets_root,
            self.ui_tx.clone(),
        )
        .await?;
        *self.runtime.lock() = Some(handle);
        Ok(())
    }

    pub async fn stop(&self) {
        let handle = self.runtime.lock().take();
        if let Some(handle) = handle {
            self.emit(UiEvent::info(UiKind::Runtime, "正在停止后台"));
            handle.stop().await;
        } else {
            self.emit(UiEvent::warn(UiKind::Runtime, "后台未运行"));
        }
    }

    pub fn trigger_demo_once(&self, event: &str) -> Result<()> {
        self.clear_finished();
        let event = DemoEvent::from_str(event)?;
        let side = demo_side(self.config.read().side);
        let guard = self.runtime.lock();
        let Some(handle) = guard.as_ref().filter(|handle| handle.is_running()) else {
            return Err(anyhow!("请先启动后台，再触发测试击杀"));
        };
        handle.trigger(event, side)?;
        self.emit(UiEvent::info(
            UiKind::Demo,
            format!("已触发测试事件: {event:?}"),
        ));
        Ok(())
    }

    fn clear_finished(&self) {
        let mut guard = self.runtime.lock();
        if guard.as_ref().is_some_and(|handle| !handle.is_running()) {
            *guard = None;
        }
    }
}

fn default_assets_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let assets = dir.join("assets");
        if assets.is_dir() {
            return assets;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn demo_side(preference: SidePreference) -> Option<Side> {
    match preference {
        SidePreference::Auto => Some(if rand::random() { Side::Ct } else { Side::T }),
        SidePreference::Ct => Some(Side::Ct),
        SidePreference::T => Some(Side::T),
    }
}
