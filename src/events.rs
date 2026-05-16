use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[derive(Clone, Debug)]
pub struct UiEvent {
    pub at: SystemTime,
    pub level: UiLevel,
    pub kind: UiKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug)]
pub enum UiLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub enum UiKind {
    Runtime,
    Overlay,
    Gsi,
    Demo,
    Effect,
    Config,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiEventDto {
    pub timestamp_ms: u64,
    pub level: &'static str,
    pub kind: &'static str,
    pub message: String,
}

impl UiEvent {
    pub fn info(kind: UiKind, message: impl Into<String>) -> Self {
        Self::new(UiLevel::Info, kind, message)
    }

    pub fn warn(kind: UiKind, message: impl Into<String>) -> Self {
        Self::new(UiLevel::Warn, kind, message)
    }

    pub fn error(kind: UiKind, message: impl Into<String>) -> Self {
        Self::new(UiLevel::Error, kind, message)
    }

    fn new(level: UiLevel, kind: UiKind, message: impl Into<String>) -> Self {
        Self {
            at: SystemTime::now(),
            level,
            kind,
            message: message.into(),
        }
    }
}

impl From<UiEvent> for UiEventDto {
    fn from(event: UiEvent) -> Self {
        let timestamp_ms = event
            .at
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default();
        Self {
            timestamp_ms,
            level: match event.level {
                UiLevel::Info => "info",
                UiLevel::Warn => "warn",
                UiLevel::Error => "error",
            },
            kind: match event.kind {
                UiKind::Runtime => "runtime",
                UiKind::Overlay => "overlay",
                UiKind::Gsi => "gsi",
                UiKind::Demo => "demo",
                UiKind::Effect => "effect",
                UiKind::Config => "config",
            },
            message: event.message,
        }
    }
}
