//! Optional fight telemetry (Rust-only sidecar; not part of official outcome JSON).

use serde::{Deserialize, Serialize};

/// User-facing trace options (CLI / experiment spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceConfig {
    /// When false, no trace events are recorded.
    pub enabled: bool,
    /// Hard cap on recorded events (oldest dropped or stop recording; we stop recording when full).
    #[serde(default = "default_max_events")]
    pub max_events: usize,
}

fn default_max_events() -> usize {
    10_000
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_events: default_max_events(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceEvent {
    pub turn: i32,
    pub fid: i32,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

impl TraceConfig {
    #[must_use]
    pub fn disabled() -> Self {
        Self::default()
    }
}
