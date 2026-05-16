#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod audio;
mod commands;
mod config;
mod effects;
mod engine;
mod events;
mod gsi;
mod overlay_client;
mod runtime;
mod state;
mod ui_events;

use anyhow::Result;
use config::{AppConfig, CliAction, help_text, parse_cli_args};
use engine::Engine;
use state::AppState;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();

    if wants_cli() {
        if let Err(err) = run_cli() {
            eprintln!("[counterfire2] {err:#}");
            std::process::exit(1);
        }
        return;
    }

    run_gui();
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("counterfire2=info")),
        )
        .init();
}

fn wants_cli() -> bool {
    std::env::args()
        .skip(1)
        .any(|arg| matches!(arg.as_str(), "--cli" | "--help" | "-h"))
}

fn run_gui() {
    let engine = Engine::new(AppConfig::default());
    let app_state = AppState::new(engine);

    tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            ui_events::install(app)?;
            let state = app.state::<AppState>();
            if let Ok(resource_dir) = app.path().resource_dir() {
                let assets_root = resource_dir.join("assets");
                if assets_root.is_dir() {
                    state.engine.set_assets_root(assets_root.clone());
                    state.engine.emit(events::UiEvent::info(
                        events::UiKind::Runtime,
                        format!("素材目录已设置: {}", assets_root.display()),
                    ));
                }
            }
            match app.path().app_config_dir() {
                Ok(config_dir) => {
                    if let Err(err) = state.engine.load_config_from(config_dir) {
                        state.engine.emit(events::UiEvent::warn(
                            events::UiKind::Config,
                            format!("加载配置失败，已使用默认配置: {err}"),
                        ));
                    }
                }
                Err(err) => state.engine.emit(events::UiEvent::warn(
                    events::UiKind::Config,
                    format!("无法定位配置目录，配置不会持久化: {err}"),
                )),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::update_config,
            commands::start_service,
            commands::stop_service,
            commands::runtime_status,
            commands::demo_once,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("[counterfire2] GUI 运行失败: {err}");
            std::process::exit(1);
        });
}

fn run_cli() -> Result<()> {
    let action = parse_cli_args(std::env::args())?;
    let CliAction::Run(config) = action else {
        println!("{}", help_text());
        return Ok(());
    };

    let engine = Engine::new(config);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        engine.start().await?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = wait_until_stopped(engine.clone()) => {},
        }
        engine.stop().await;
        Ok(())
    })
}

async fn wait_until_stopped(engine: Engine) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if !engine.is_running() {
            break;
        }
    }
}
