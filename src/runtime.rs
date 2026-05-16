use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};

use crate::assets::AssetCatalog;
use crate::audio::AudioPlayer;
use crate::config::{AppConfig, RunMode, SidePreference};
use crate::effects::{KillTrigger, Side};
use crate::events::{UiEvent, UiKind};
use crate::gsi;
use crate::overlay_client::{OverlayClient, OverlayOptions, OverlayRunExit};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
type StartupResult = std::result::Result<(), String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DemoEvent {
    Single,
    Double,
    Triple,
    Quad,
    Penta,
    Hexa,
    Septa,
    Octo,
    Headshot,
    Knife,
    Grenade,
}

impl FromStr for DemoEvent {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "single" => Ok(Self::Single),
            "double" => Ok(Self::Double),
            "triple" => Ok(Self::Triple),
            "quad" => Ok(Self::Quad),
            "penta" => Ok(Self::Penta),
            "hexa" => Ok(Self::Hexa),
            "septa" => Ok(Self::Septa),
            "octo" => Ok(Self::Octo),
            "headshot" => Ok(Self::Headshot),
            "knife" => Ok(Self::Knife),
            "grenade" => Ok(Self::Grenade),
            _ => Err(anyhow!("未知测试事件: {value}")),
        }
    }
}

pub struct RuntimeHandle {
    stop_tx: Option<oneshot::Sender<()>>,
    trigger_tx: mpsc::Sender<KillTrigger>,
    join: Option<thread::JoinHandle<()>>,
    running: Arc<AtomicBool>,
    mode: RunMode,
}

impl RuntimeHandle {
    pub async fn spawn(
        config: AppConfig,
        config_state: Arc<RwLock<AppConfig>>,
        assets_root: PathBuf,
        ui_tx: broadcast::Sender<UiEvent>,
    ) -> Result<Self> {
        let (trigger_tx, trigger_rx) = mpsc::channel(64);
        let (stop_tx, stop_rx) = oneshot::channel();
        let (startup_tx, startup_rx) = oneshot::channel::<StartupResult>();
        let running = Arc::new(AtomicBool::new(true));
        let running_for_thread = Arc::clone(&running);
        let mode = config.mode;
        let ui_for_thread = ui_tx.clone();
        let trigger_for_thread = trigger_tx.clone();

        let join = thread::spawn(move || {
            let mut startup_tx = Some(startup_tx);
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("创建后台 tokio runtime 失败")
                .and_then(|runtime| {
                    runtime.block_on(run_worker(
                        WorkerArgs {
                            config,
                            config_state,
                            assets_root,
                            trigger_rx,
                            trigger_tx: trigger_for_thread,
                            stop_rx,
                            ui_tx: ui_for_thread.clone(),
                        },
                        &mut startup_tx,
                    ))
                });

            if let Err(err) = result {
                signal_startup(&mut startup_tx, Err(format!("{err:#}")));
                emit(
                    &ui_for_thread,
                    UiEvent::error(UiKind::Runtime, format!("后台运行失败: {err:#}")),
                );
                tracing::error!(%err, "CounterFire 2 后台运行失败");
            }
            running_for_thread.store(false, Ordering::SeqCst);
        });

        let handle = Self {
            stop_tx: Some(stop_tx),
            trigger_tx,
            join: Some(join),
            running,
            mode,
        };

        match tokio::time::timeout(STARTUP_TIMEOUT, startup_rx).await {
            Ok(Ok(Ok(()))) => Ok(handle),
            Ok(Ok(Err(message))) => {
                handle.stop().await;
                Err(anyhow!(message))
            }
            Ok(Err(_)) => {
                handle.stop().await;
                Err(anyhow!("后台启动状态通道已关闭"))
            }
            Err(_) => {
                handle.stop().await;
                Err(anyhow!(
                    "后台启动超时，请确认 overlay-engine 和 Game Bar 小组件已启动"
                ))
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn mode(&self) -> RunMode {
        self.mode
    }

    pub fn trigger(&self, event: DemoEvent, side: Option<Side>) -> Result<()> {
        self.trigger_tx
            .try_send(sample_trigger(event, side))
            .map_err(|err| anyhow!("发送测试击杀失败: {err}"))
    }

    pub async fn stop(mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = tokio::task::spawn_blocking(move || join.join()).await;
        }
    }
}

struct WorkerArgs {
    config: AppConfig,
    config_state: Arc<RwLock<AppConfig>>,
    assets_root: PathBuf,
    trigger_rx: mpsc::Receiver<KillTrigger>,
    trigger_tx: mpsc::Sender<KillTrigger>,
    stop_rx: oneshot::Receiver<()>,
    ui_tx: broadcast::Sender<UiEvent>,
}

async fn run_worker(
    args: WorkerArgs,
    startup_tx: &mut Option<oneshot::Sender<StartupResult>>,
) -> Result<()> {
    let WorkerArgs {
        config,
        config_state,
        assets_root,
        mut trigger_rx,
        trigger_tx,
        mut stop_rx,
        ui_tx,
    } = args;

    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let assets = AssetCatalog::load(&assets_root)
        .with_context(|| format!("加载素材失败: {}", assets_root.display()))?;
    emit(
        &ui_tx,
        UiEvent::info(
            UiKind::Runtime,
            format!("素材已加载: {} 张 bitmap", assets.bitmaps().len()),
        ),
    );

    let audio = AudioPlayer::new();
    let overlay_options = OverlayOptions {
        width: config.width,
        height: config.height,
        vsync: config.vsync,
    };
    emit(
        &ui_tx,
        UiEvent::info(UiKind::Overlay, "正在连接 overlay-engine"),
    );
    let mut overlay = OverlayClient::connect(&assets, overlay_options).await?;
    emit(&ui_tx, UiEvent::info(UiKind::Overlay, "overlay 已就绪"));

    let demo_task = if config.mode == RunMode::Demo {
        emit(&ui_tx, UiEvent::info(UiKind::Demo, "Demo 模式已启动"));
        Some(spawn_demo(trigger_tx.clone(), Arc::clone(&config_state)))
    } else {
        None
    };

    let gsi_handle = if config.mode == RunMode::Gsi {
        Some(
            gsi::start(
                config.port,
                trigger_tx.clone(),
                ui_tx.clone(),
                Arc::clone(&config_state),
            )
            .await?,
        )
    } else {
        None
    };

    emit(&ui_tx, UiEvent::info(UiKind::Runtime, "后台已启动"));
    signal_startup(startup_tx, Ok(()));

    let run_result = loop {
        match overlay
            .run(
                &assets,
                &mut trigger_rx,
                audio.clone(),
                Arc::clone(&config_state),
                &mut stop_rx,
                ui_tx.clone(),
            )
            .await
        {
            Ok(OverlayRunExit::CanvasSizeChanged) => {
                emit(
                    &ui_tx,
                    UiEvent::info(
                        UiKind::Overlay,
                        "检测到屏幕尺寸变化，正在重建 Game Bar 画布",
                    ),
                );
                drop(overlay);
                overlay = match OverlayClient::connect(&assets, overlay_options).await {
                    Ok(overlay) => overlay,
                    Err(err) => break Err(err),
                };
                emit(
                    &ui_tx,
                    UiEvent::info(UiKind::Overlay, "Game Bar 画布已重建"),
                );
            }
            result => break result,
        }
    };

    if let Some(task) = demo_task {
        task.abort();
    }
    if let Some(handle) = gsi_handle
        && let Err(err) = handle.stop().await
    {
        emit(
            &ui_tx,
            UiEvent::warn(UiKind::Gsi, format!("停止 GSI 失败: {err}")),
        );
    }

    run_result?;
    emit(&ui_tx, UiEvent::info(UiKind::Runtime, "后台已停止"));
    Ok(())
}

fn spawn_demo(
    tx: mpsc::Sender<KillTrigger>,
    config_state: Arc<RwLock<AppConfig>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let samples = [
            DemoEvent::Single,
            DemoEvent::Double,
            DemoEvent::Triple,
            DemoEvent::Quad,
            DemoEvent::Penta,
            DemoEvent::Hexa,
            DemoEvent::Septa,
            DemoEvent::Octo,
            DemoEvent::Headshot,
            DemoEvent::Knife,
            DemoEvent::Grenade,
        ];
        let mut idx = 0usize;
        loop {
            let side = demo_side(config_state.read().side);
            if tx.send(sample_trigger(samples[idx], side)).await.is_err() {
                break;
            }
            idx = (idx + 1) % samples.len();
            tokio::time::sleep(Duration::from_millis(1800)).await;
        }
    })
}

fn demo_side(preference: SidePreference) -> Option<Side> {
    match preference {
        SidePreference::Auto => Some(if rand::random() { Side::Ct } else { Side::T }),
        SidePreference::Ct => Some(Side::Ct),
        SidePreference::T => Some(Side::T),
    }
}

fn sample_trigger(event: DemoEvent, side: Option<Side>) -> KillTrigger {
    match event {
        DemoEvent::Single => KillTrigger {
            round_kills: 1,
            weapon: Some("weapon_ak47".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Double => KillTrigger {
            round_kills: 2,
            weapon: Some("weapon_m4a1".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Triple => KillTrigger {
            round_kills: 3,
            weapon: Some("weapon_awp".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Quad => KillTrigger {
            round_kills: 4,
            weapon: Some("weapon_ak47".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Penta => KillTrigger {
            round_kills: 5,
            weapon: Some("weapon_ak47".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Hexa => KillTrigger {
            round_kills: 6,
            weapon: Some("weapon_ak47".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Septa => KillTrigger {
            round_kills: 7,
            weapon: Some("weapon_ak47".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Octo => KillTrigger {
            round_kills: 8,
            weapon: Some("weapon_ak47".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Headshot => KillTrigger {
            round_kills: 1,
            weapon: Some("weapon_ak47".to_owned()),
            is_headshot: true,
            side,
        },
        DemoEvent::Knife => KillTrigger {
            round_kills: 1,
            weapon: Some("weapon_knife".to_owned()),
            is_headshot: false,
            side,
        },
        DemoEvent::Grenade => KillTrigger {
            round_kills: 1,
            weapon: Some("weapon_hegrenade".to_owned()),
            is_headshot: false,
            side,
        },
    }
}

fn signal_startup(startup_tx: &mut Option<oneshot::Sender<StartupResult>>, result: StartupResult) {
    if let Some(tx) = startup_tx.take() {
        let _ = tx.send(result);
    }
}

fn emit(ui_tx: &broadcast::Sender<UiEvent>, event: UiEvent) {
    let _ = ui_tx.send(event);
}
