use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use cs2_gsi::GameStateListener;
use cs2_gsi::cfg::GsiCfg;
use cs2_gsi::events::{
    FreezetimeStarted, KillFeed, NewGameState, PlayerGotKill, PlayerUpdated, RoundStarted,
};
use cs2_gsi::model::{GameState, Player, PlayerTeam};
use parking_lot::{Mutex, RwLock};
use tokio::sync::{broadcast, mpsc};

use crate::config::AppConfig;
use crate::effects::{KillTrigger, Side};
use crate::events::{UiEvent, UiKind};

const PORT_FALLBACK_COUNT: u16 = 5;

#[derive(Default)]
struct PlayerKillState {
    last_kill_at: Option<Instant>,
    streak: u32,
    last_observed_round_kills: i32,
}

impl PlayerKillState {
    fn next_streak(&mut self, reset_seconds: f32) -> u32 {
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

    fn reset_round(&mut self) {
        self.last_kill_at = None;
        self.streak = 0;
        self.last_observed_round_kills = 0;
    }
}

#[derive(Default)]
struct LocalPlayerState {
    steamid: Option<String>,
    side: Option<Side>,
    players: HashMap<String, PlayerKillState>,
    suppressed_switch_kills: HashSet<(String, i32)>,
}

struct KillCandidate {
    steamid: String,
    side: Option<Side>,
    observed_round_kills: i32,
    weapon: Option<String>,
    is_headshot: bool,
}

impl LocalPlayerState {
    fn update_from_game_state(&mut self, state: &GameState) -> Option<Side> {
        let provider_steamid = state.provider.steamid.trim();
        if provider_steamid.is_empty() {
            return None;
        }

        if self.steamid.as_deref() != Some(provider_steamid) {
            self.steamid = Some(provider_steamid.to_owned());
            self.side = None;
        }

        let player = local_player(state, provider_steamid)?;

        self.observe_local_round_kills(provider_steamid, player.state.round_kills);
        let detected_side = side_from_team(player.team);
        let changed_side = detected_side.filter(|side| self.side != Some(*side));
        self.side = detected_side;
        changed_side
    }

    fn observe_local_round_kills(&mut self, steamid: &str, round_kills: i32) {
        let round_kills = round_kills.max(0);
        match self.players.get_mut(steamid) {
            Some(player) if round_kills < player.last_observed_round_kills => {
                player.reset_round();
                player.last_observed_round_kills = round_kills;
            }
            Some(_) => {}
            None => {
                self.players.insert(
                    steamid.to_owned(),
                    PlayerKillState {
                        last_observed_round_kills: round_kills,
                        ..Default::default()
                    },
                );
            }
        }
    }

    fn suppress_switch_kill(&mut self, previous: &Player, player: &Player) {
        if previous.steamid.is_empty()
            || player.steamid.is_empty()
            || previous.steamid == player.steamid
            || player.state.round_kills <= 0
        {
            return;
        }

        self.suppressed_switch_kills
            .insert((player.steamid.clone(), player.state.round_kills));
    }

    fn try_accept_kill(
        &mut self,
        candidate: KillCandidate,
        reset_seconds: f32,
    ) -> Option<KillTrigger> {
        if candidate.steamid.is_empty()
            || self.steamid.as_deref() != Some(candidate.steamid.as_str())
            || candidate.observed_round_kills <= 0
            || self
                .suppressed_switch_kills
                .contains(&(candidate.steamid.clone(), candidate.observed_round_kills))
        {
            return None;
        }

        let side = candidate.side.or(self.side);
        if let Some(side) = candidate.side {
            self.side = Some(side);
        }

        let player = self.players.entry(candidate.steamid).or_default();
        if candidate.observed_round_kills <= player.last_observed_round_kills {
            return None;
        }

        player.last_observed_round_kills = candidate.observed_round_kills;
        let round_kills = player.next_streak(reset_seconds);
        Some(KillTrigger {
            round_kills,
            weapon: candidate.weapon,
            is_headshot: candidate.is_headshot,
            side,
        })
    }

    fn reset_round(&mut self) {
        self.suppressed_switch_kills.clear();
        for player in self.players.values_mut() {
            player.reset_round();
        }
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
            let changed_side = local.lock().update_from_game_state(&event.state);

            if let Some(side) = changed_side {
                emit(
                    &ui_tx,
                    UiEvent::info(UiKind::Gsi, format!("已识别本地阵营: {}", side.suffix())),
                );
            }
        });
    }

    {
        let local = Arc::clone(&local);
        listener.on(move |event: &PlayerUpdated| {
            if let Some(previous) = event.previous.as_ref() {
                local.lock().suppress_switch_kill(previous, &event.player);
            }
        });
    }

    {
        let local = Arc::clone(&local);
        listener.on(move |_event: &RoundStarted| {
            local.lock().reset_round();
        });
    }

    {
        let local = Arc::clone(&local);
        listener.on(move |_event: &FreezetimeStarted| {
            local.lock().reset_round();
        });
    }

    {
        let local = Arc::clone(&local);
        let tx = tx.clone();
        let ui_tx = ui_tx.clone();
        let config = Arc::clone(&config);
        listener.on(move |event: &PlayerGotKill| {
            let reset_seconds = config.read().kill_streak_reset_seconds;
            let candidate = KillCandidate {
                steamid: event.player.steamid.clone(),
                side: side_from_team(event.player.team),
                observed_round_kills: event.new_round_kills,
                weapon: event.weapon.clone(),
                is_headshot: event.is_headshot,
            };
            handle_kill_candidate(
                &local,
                &tx,
                &ui_tx,
                candidate,
                reset_seconds,
                "PlayerGotKill",
            );
        });
    }

    {
        let local = Arc::clone(&local);
        let tx = tx.clone();
        let ui_tx = ui_tx.clone();
        let config = Arc::clone(&config);
        listener.on(move |event: &KillFeed| {
            let reset_seconds = config.read().kill_streak_reset_seconds;
            let candidate = KillCandidate {
                steamid: event.killer.steamid.clone(),
                side: side_from_team(event.killer.team),
                observed_round_kills: event.killer.state.round_kills,
                weapon: event.weapon.clone(),
                is_headshot: event.is_headshot,
            };
            handle_kill_candidate(&local, &tx, &ui_tx, candidate, reset_seconds, "KillFeed");
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

fn handle_kill_candidate(
    local: &Mutex<LocalPlayerState>,
    tx: &mpsc::Sender<KillTrigger>,
    ui_tx: &broadcast::Sender<UiEvent>,
    candidate: KillCandidate,
    reset_seconds: f32,
    source: &str,
) {
    let Some(trigger) = local.lock().try_accept_kill(candidate, reset_seconds) else {
        return;
    };

    let round_kills = trigger.round_kills;
    let side = trigger.side;
    match tx.try_send(trigger) {
        Ok(()) => emit(
            ui_tx,
            UiEvent::info(
                UiKind::Gsi,
                format!(
                    "收到本地击杀数据({source}): 第 {round_kills} 杀，阵营 {}",
                    side.map(Side::suffix).unwrap_or("未知")
                ),
            ),
        ),
        Err(err) => {
            tracing::warn!(%err, "GSI 击杀事件队列已满");
            emit(
                ui_tx,
                UiEvent::warn(UiKind::Gsi, "GSI 击杀事件过多，已丢弃一条"),
            );
        }
    }
}

fn local_player<'a>(state: &'a GameState, steamid: &str) -> Option<&'a Player> {
    state.allplayers.get(steamid).or_else(|| {
        state
            .player
            .as_ref()
            .filter(|player| player.steamid == steamid)
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use cs2_gsi::model::{PlayerState, Provider};

    fn player(steamid: &str, team: PlayerTeam, round_kills: i32) -> Player {
        Player {
            steamid: steamid.to_owned(),
            team,
            state: PlayerState {
                round_kills,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn candidate(steamid: &str, observed_round_kills: i32) -> KillCandidate {
        KillCandidate {
            steamid: steamid.to_owned(),
            side: Some(Side::Ct),
            observed_round_kills,
            weapon: None,
            is_headshot: false,
        }
    }

    #[test]
    fn provider_steamid_is_used_instead_of_spectated_player() {
        let mut local = LocalPlayerState::default();
        let mut state = GameState {
            provider: Provider {
                steamid: "local".to_owned(),
                ..Default::default()
            },
            player: Some(player("enemy", PlayerTeam::T, 3)),
            ..Default::default()
        };
        state
            .allplayers
            .insert("local".to_owned(), player("local", PlayerTeam::CT, 0));

        let changed_side = local.update_from_game_state(&state);

        assert_eq!(local.steamid.as_deref(), Some("local"));
        assert_eq!(changed_side, Some(Side::Ct));
        assert!(local.try_accept_kill(candidate("enemy", 4), 5.0).is_none());
    }

    #[test]
    fn duplicate_sources_only_accept_one_kill() {
        let mut local = LocalPlayerState {
            steamid: Some("local".to_owned()),
            ..Default::default()
        };

        let first = local.try_accept_kill(candidate("local", 1), 5.0);
        let duplicate = local.try_accept_kill(candidate("local", 1), 5.0);

        assert_eq!(first.unwrap().round_kills, 1);
        assert!(duplicate.is_none());
    }

    #[test]
    fn spectated_player_switch_suppresses_historical_round_kill() {
        let mut local = LocalPlayerState {
            steamid: Some("local".to_owned()),
            ..Default::default()
        };
        let previous = player("enemy", PlayerTeam::T, 0);
        let current = player("local", PlayerTeam::CT, 1);

        local.suppress_switch_kill(&previous, &current);

        assert!(local.try_accept_kill(candidate("local", 1), 5.0).is_none());
        assert_eq!(
            local
                .try_accept_kill(candidate("local", 2), 5.0)
                .unwrap()
                .round_kills,
            1
        );
    }

    #[test]
    fn streak_is_isolated_by_steamid() {
        let mut local = LocalPlayerState {
            steamid: Some("a".to_owned()),
            ..Default::default()
        };

        assert_eq!(
            local
                .try_accept_kill(candidate("a", 1), 5.0)
                .unwrap()
                .round_kills,
            1
        );
        assert_eq!(
            local
                .try_accept_kill(candidate("a", 2), 5.0)
                .unwrap()
                .round_kills,
            2
        );

        local.steamid = Some("b".to_owned());

        assert_eq!(
            local
                .try_accept_kill(candidate("b", 1), 5.0)
                .unwrap()
                .round_kills,
            1
        );
    }

    #[test]
    fn round_reset_allows_next_round_first_kill() {
        let mut local = LocalPlayerState {
            steamid: Some("local".to_owned()),
            ..Default::default()
        };

        assert!(local.try_accept_kill(candidate("local", 1), 5.0).is_some());
        assert!(local.try_accept_kill(candidate("local", 2), 5.0).is_some());

        local.reset_round();

        let next_round = local.try_accept_kill(candidate("local", 1), 5.0).unwrap();
        assert_eq!(next_round.round_kills, 1);
    }

    #[test]
    fn initial_local_snapshot_becomes_dedupe_baseline() {
        let mut local = LocalPlayerState::default();
        let state = GameState {
            provider: Provider {
                steamid: "local".to_owned(),
                ..Default::default()
            },
            player: Some(player("local", PlayerTeam::CT, 1)),
            ..Default::default()
        };

        local.update_from_game_state(&state);

        assert!(local.try_accept_kill(candidate("local", 1), 5.0).is_none());
        assert_eq!(
            local
                .try_accept_kill(candidate("local", 2), 5.0)
                .unwrap()
                .round_kills,
            1
        );
    }
}
