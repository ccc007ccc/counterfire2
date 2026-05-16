use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use cs2_gsi::GameStateListener;
use cs2_gsi::cfg::GsiCfg;
use cs2_gsi::events::{KillFeed, NewGameState, PlayerGotKill};
use cs2_gsi::model::PlayerTeam;
use parking_lot::{Mutex, RwLock};
use tokio::sync::{broadcast, mpsc};

use crate::config::AppConfig;
use crate::effects::{KillTrigger, Side};
use crate::events::{UiEvent, UiKind};

const PORT_FALLBACK_COUNT: u16 = 5;

#[derive(Default)]
struct LocalPlayerState {
    steamid: Option<String>,
    side: Option<Side>,
    last_kill_at: Option<Instant>,
    streak: u32,
}

impl LocalPlayerState {
    fn next_round_kills(&mut self, reset_seconds: f32) -> u32 {
        let now = Instant::now();
        let reset_after = Duration::from_secs_f32(reset_seconds);
        if self
            .last_kill_at
            .is_none_or(|last| now.duration_since(last) > reset_after)
        {
            self.streak = 1;
        } else {
            self.streak = self.streak.saturating_add(1).max(1);
        }
        self.last_kill_at = Some(now);
        self.streak
    }
}

pub struct GsiHandle {
    listener: GameStateListener,
}

impl GsiHandle {
    pub async fn stop(self) -> Result<()> {
        self.listener.stop().await?;
        Ok(())
    }
}

pub async fn start(
    port: u16,
    tx: mpsc::Sender<KillTrigger>,
    ui_tx: broadcast::Sender<UiEvent>,
    config: Arc<RwLock<AppConfig>>,
) -> Result<GsiHandle> {
    let listener = GameStateListener::new(port);
    let local = Arc::new(Mutex::new(LocalPlayerState::default()));

    {
        let local = Arc::clone(&local);
        let ui_tx = ui_tx.clone();
        listener.on(move |event: &NewGameState| {
            if let Some(player) = event.state.player.as_ref() {
                let detected_side = side_from_team(player.team);
                let changed_side = {
                    let mut local = local.lock();
                    if !player.steamid.is_empty() {
                        local.steamid = Some(player.steamid.clone());
                    }
                    let changed_side = detected_side.filter(|side| local.side != Some(*side));
                    local.side = detected_side;
                    changed_side
                };

                if let Some(side) = changed_side {
                    emit(
                        &ui_tx,
                        UiEvent::info(UiKind::Gsi, format!("已识别本地阵营: {}", side.suffix())),
                    );
                }
            }
        });
    }

    {
        let local = Arc::clone(&local);
        let tx = tx.clone();
        let ui_tx = ui_tx.clone();
        let config = Arc::clone(&config);
        listener.on(move |event: &PlayerGotKill| {
            let event_side = side_from_team(event.player.team);
            let reset_seconds = config.read().kill_streak_reset_seconds;
            let (side, round_kills) = {
                let mut local = local.lock();
                if let Some(local_steamid) = local.steamid.as_deref()
                    && local_steamid != event.player.steamid.as_str()
                {
                    return;
                }
                if let Some(side) = event_side {
                    local.side = Some(side);
                }
                (
                    event_side.or(local.side),
                    local.next_round_kills(reset_seconds),
                )
            };
            let trigger = KillTrigger {
                round_kills,
                weapon: event.weapon.clone(),
                is_headshot: event.is_headshot,
                side,
            };
            match tx.try_send(trigger) {
                Ok(()) => emit(
                    &ui_tx,
                    UiEvent::info(
                        UiKind::Gsi,
                        format!(
                            "收到本地击杀数据: 第 {round_kills} 杀，阵营 {}",
                            side.map(Side::suffix).unwrap_or("未知")
                        ),
                    ),
                ),
                Err(err) => {
                    tracing::warn!(%err, "GSI 击杀事件队列已满");
                    emit(
                        &ui_tx,
                        UiEvent::warn(UiKind::Gsi, "GSI 击杀事件过多，已丢弃一条"),
                    );
                }
            }
        });
    }

    {
        let local = Arc::clone(&local);
        let ui_tx = ui_tx.clone();
        let config = Arc::clone(&config);
        listener.on(move |event: &KillFeed| {
            let event_side = side_from_team(event.killer.team);
            let reset_seconds = config.read().kill_streak_reset_seconds;
            let (side, round_kills) = {
                let mut local = local.lock();
                if local.steamid.as_deref() != Some(event.killer.steamid.as_str()) {
                    return;
                }
                if let Some(side) = event_side {
                    local.side = Some(side);
                }
                (
                    event_side.or(local.side),
                    local.next_round_kills(reset_seconds),
                )
            };

            let trigger = KillTrigger {
                round_kills,
                weapon: event.weapon.clone(),
                is_headshot: event.is_headshot,
                side,
            };
            if let Err(err) = tx.try_send(trigger) {
                tracing::warn!(%err, "GSI kill feed 队列已满");
                emit(
                    &ui_tx,
                    UiEvent::warn(UiKind::Gsi, "GSI kill feed 事件过多，已丢弃一条"),
                );
            }
        });
    }

    listener.start_with_fallbacks(gsi_fallbacks(port)).await?;
    let bound_port = listener
        .actual_addr()
        .map(|addr| addr.port())
        .unwrap_or(port);

    if bound_port != port {
        emit(
            &ui_tx,
            UiEvent::warn(
                UiKind::Gsi,
                format!("首选 GSI 端口 {port} 被占用，已切换到 {bound_port}"),
            ),
        );
    }

    let cfg = GsiCfg::for_localhost("CounterFire 2", bound_port);
    match cfg.write_to_cs2() {
        Ok(path) => {
            tracing::info!(path = %path.display(), port = bound_port, "已写入 CS2 GSI cfg");
            emit(
                &ui_tx,
                UiEvent::info(
                    UiKind::Gsi,
                    format!("已写入 CS2 GSI cfg: {} (端口 {bound_port})", path.display()),
                ),
            );
        }
        Err(err) => {
            emit(
                &ui_tx,
                UiEvent::warn(UiKind::Gsi, format!("无法自动写入 CS2 GSI cfg: {err}")),
            );
            tracing::warn!(%err, "无法自动写入 CS2 GSI cfg");
        }
    }

    tracing::info!(port = bound_port, "CS2 GSI listener 已启动");
    emit(
        &ui_tx,
        UiEvent::info(
            UiKind::Gsi,
            format!("CS2 GSI listener 已启动: http://127.0.0.1:{bound_port}"),
        ),
    );
    Ok(GsiHandle { listener })
}

fn emit(ui_tx: &broadcast::Sender<UiEvent>, event: UiEvent) {
    let _ = ui_tx.send(event);
}

fn gsi_fallbacks(port: u16) -> Vec<SocketAddr> {
    (1..=PORT_FALLBACK_COUNT)
        .filter_map(|offset| port.checked_add(offset))
        .map(localhost)
        .chain(std::iter::once(localhost(0)))
        .collect()
}

fn localhost(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

fn side_from_team(team: PlayerTeam) -> Option<Side> {
    match team {
        PlayerTeam::CT => Some(Side::Ct),
        PlayerTeam::T => Some(Side::T),
        PlayerTeam::Unassigned => None,
    }
}
