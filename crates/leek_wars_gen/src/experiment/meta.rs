//! Fetch / snapshot meta lists (e.g. item IDs) for reproducible loadout experiments.

use crate::error::GenError;
use serde_json::Value;
use std::path::Path;

/// GET `url` and parse JSON (used for API snapshots).
pub fn fetch_json(url: &str) -> Result<Value, GenError> {
    let v: Value = ureq::get(url)
        .call()
        .map_err(|e| GenError::Message(format!("HTTP GET {url}: {e}")))?
        .into_json()
        .map_err(|e| GenError::Message(format!("JSON body: {e}")))?;
    Ok(v)
}

/// Write `{ "fetched_at": unix_secs, "url": url, "body": ... }` for offline experiments.
/// Load a file written by [`write_meta_snapshot`].
pub fn load_meta_snapshot(path: &Path) -> Result<serde_json::Value, GenError> {
    let raw = std::fs::read_to_string(path).map_err(GenError::from)?;
    serde_json::from_str(&raw).map_err(|e| GenError::Message(e.to_string()))
}

/// Same envelope as [`write_meta_snapshot`], but `url` is a logical label (e.g. `lw-meta:rankings/leeks`)
/// and `body` is the JSON export from `lw_meta` / `leekgen meta rankings`.
pub fn write_lw_meta_snapshot(path: &Path, url_label: &str, body: &Value) -> Result<(), GenError> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let doc = serde_json::json!({
        "fetched_at": ts,
        "url": url_label,
        "body": body,
    });
    let s = serde_json::to_string_pretty(&doc).map_err(GenError::ScenarioJson)?;
    std::fs::write(path, s).map_err(GenError::from)?;
    Ok(())
}

pub fn write_meta_snapshot(path: &Path, url: &str) -> Result<(), GenError> {
    let body = fetch_json(url)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let doc = serde_json::json!({
        "fetched_at": ts,
        "url": url,
        "body": body,
    });
    let s = serde_json::to_string_pretty(&doc).map_err(GenError::ScenarioJson)?;
    std::fs::write(path, s).map_err(GenError::from)?;
    Ok(())
}
