use tauri::{App, Emitter, Manager};
use tokio::sync::broadcast::error::RecvError;

use crate::events::UiEventDto;
use crate::state::AppState;

pub fn install(app: &mut App) -> tauri::Result<()> {
    let engine = app.state::<AppState>().engine.clone();
    let handle = app.handle().clone();
    let mut rx = engine.subscribe_ui();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(err) = handle.emit("ui-event", UiEventDto::from(event)) {
                        tracing::warn!(%err, "发送 ui-event 失败");
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "ui-event 接收端滞后");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
    Ok(())
}
