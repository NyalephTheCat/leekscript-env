use super::planner::RunTask;
use crate::error::GenError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn stable_json_bytes(v: &Value) -> Result<Vec<u8>, GenError> {
    serde_json::to_vec(v).map_err(GenError::ScenarioJson)
}

/// Content-addressed cache key for a run task.
pub fn cache_key_for_task(task: &RunTask) -> Result<String, GenError> {
    let mut h = Sha256::new();
    h.update(stable_json_bytes(&task.scenario_value)?);
    h.update(task.arm_name.as_bytes());
    let tun = serde_json::to_vec(&serde_json::json!(task.tunables)).map_err(GenError::ScenarioJson)?;
    h.update(&tun);
    h.update(env!("CARGO_PKG_VERSION").as_bytes());
    Ok(hex::encode(h.finalize()))
}

pub fn cache_outcome_path(cache_dir: &Path, key: &str) -> PathBuf {
    cache_dir.join(format!("{key}.outcome.json"))
}
