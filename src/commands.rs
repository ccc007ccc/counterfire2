use tauri::State;

use crate::config::AppConfig;
use crate::engine::RuntimeStatus;
use crate::state::AppState;

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.engine.config_snapshot()
}

#[tauri::command]
pub fn update_config(state: State<'_, AppState>, config: AppConfig) -> Result<AppConfig, String> {
    state
        .engine
        .update_config(config)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn start_service(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.start().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn stop_service(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.stop().await;
    Ok(())
}

#[tauri::command]
pub fn runtime_status(state: State<'_, AppState>) -> RuntimeStatus {
    state.engine.status()
}

#[tauri::command]
pub fn demo_once(state: State<'_, AppState>, event: String) -> Result<(), String> {
    state
        .engine
        .trigger_demo_once(&event)
        .map_err(|err| err.to_string())
}
