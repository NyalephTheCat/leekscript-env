//! Stable outcome metrics for experiments and external optimizers.

use crate::error::GenError;
use serde::{Deserialize, Serialize};

/// Summary metrics extracted from official-shaped outcome JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunMetrics {
    pub winner: Option<i64>,
    pub duration: Option<i64>,
    pub error: Option<String>,
}

impl RunMetrics {
    pub fn from_outcome_json(s: &str) -> Result<Self, GenError> {
        let v: serde_json::Value =
            serde_json::from_str(s).map_err(|e| GenError::Message(e.to_string()))?;
        Ok(Self::from_outcome_value(&v))
    }

    pub fn from_outcome_value(v: &serde_json::Value) -> Self {
        let winner = v.get("winner").and_then(|x| x.as_i64());
        let duration = v.get("duration").and_then(|x| x.as_i64());
        Self {
            winner,
            duration,
            error: None,
        }
    }

    pub fn with_error(e: String) -> Self {
        Self {
            winner: None,
            duration: None,
            error: Some(e),
        }
    }
}

