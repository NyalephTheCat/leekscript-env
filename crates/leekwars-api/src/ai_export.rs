//! Export farmer AIs to disk with the same relative paths as the Leek Wars editor
//! ([`FileSystem.getAIFullPath`](https://github.com/leek-wars/leek-wars/blob/master/src/model/filesystem.ts)).

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

/// Options for [`LeekWarsClient::export_farmer_ais_to_directory`].
#[derive(Debug, Clone)]
pub struct AiExportOptions {
    /// `ai/sync` batch size (default 40).
    pub sync_chunk_size: usize,
    /// Pause between sync chunks and download fallbacks (default 350ms).
    pub throttle: Duration,
    /// If sync omits an id, try `ai/download` (default true).
    pub download_fallback: bool,
}

impl Default for AiExportOptions {
    fn default() -> Self {
        Self {
            sync_chunk_size: 40,
            throttle: Duration::from_millis(350),
            download_fallback: true,
        }
    }
}

/// Result of [`LeekWarsClient::export_farmer_ais_to_directory`].
#[derive(Debug, Default)]
pub struct AiExportReport {
    pub written: usize,
    pub paths: Vec<PathBuf>,
    pub failures: Vec<(i64, String)>,
}

#[derive(Debug)]
struct FolderMeta {
    #[allow(dead_code)]
    id: i64,
    name: String,
    parent: i64,
}

impl LeekWarsClient {
    /// Log in is assumed done; pass `farmer` from `session["farmer"]` after
    /// [`farmer_get_from_token`](crate::LeekWarsClient::farmer_get_from_token) (or login payload).
    ///
    /// Uses `ai/sync` for source (same as the editor); optionally falls back to `ai/download`.
    pub async fn export_farmer_ais_to_directory(
        &self,
        farmer: &Value,
        root: impl AsRef<Path>,
        options: AiExportOptions,
    ) -> Result<AiExportReport> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)?;

        let ais = farmer["ais"].as_array().ok_or_else(|| {
            crate::error::Error::Api("farmer.ais is missing or not an array".into())
        })?;

        let folder_map = parse_folders(farmer);

        let mut ids: Vec<i64> = Vec::new();
        let mut rel_by_id: HashMap<i64, PathBuf> = HashMap::new();

        for ai in ais {
            let id = match ai["id"].as_i64() {
                Some(i) if i > 0 => i,
                _ => continue,
            };
            let rel = ai_leek_path_with_folder_map(ai, &folder_map);
            rel_by_id.insert(id, rel);
            ids.push(id);
        }

        let mut report = AiExportReport::default();
        let mut got: HashSet<i64> = HashSet::new();

        for chunk in ids.chunks(options.sync_chunk_size.max(1)) {
            let stamps: Vec<(i64, i64)> = chunk.iter().map(|&id| (id, 0)).collect();
            match self.ai_sync(&stamps).await {
                Ok(entries) => {
                    for entry in entries {
                        got.insert(entry.id);
                        let rel = rel_by_id
                            .get(&entry.id)
                            .cloned()
                            .unwrap_or_else(|| PathBuf::from(format!("{}.leek", entry.id)));
                        let file_path = root.join(&rel);
                        if let Some(parent) = file_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&file_path, entry.code.as_bytes())?;
                        report.written += 1;
                        report.paths.push(file_path);
                    }
                }
                Err(e) => {
                    for &id in chunk {
                        report.failures.push((id, format!("ai/sync chunk: {e}")));
                    }
                }
            }
            tokio::time::sleep(options.throttle).await;
        }

        if options.download_fallback {
            for id in &ids {
                if got.contains(id) {
                    continue;
                }
                let rel = rel_by_id
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from(format!("{id}.leek")));
                let file_path = root.join(&rel);
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                match self.ai_download(*id).await {
                    Ok(bytes) => {
                        std::fs::write(&file_path, bytes)?;
                        report.written += 1;
                        report.paths.push(file_path);
                        got.insert(*id);
                    }
                    Err(e) => report.failures.push((*id, e.to_string())),
                }
                tokio::time::sleep(options.throttle).await;
            }
        } else {
            for id in &ids {
                if !got.contains(id) {
                    report
                        .failures
                        .push((*id, "missing from ai/sync and fallback disabled".into()));
                }
            }
        }

        Ok(report)
    }
}

/// Relative path (under an export root) for one AI, matching the editor (`getAIFullPath`).
pub fn ai_leek_relative_path(farmer: &Value, ai: &Value) -> PathBuf {
    let map = parse_folders(farmer);
    ai_leek_path_with_folder_map(ai, &map)
}

fn ai_leek_path_with_folder_map(ai: &Value, folder_map: &HashMap<i64, FolderMeta>) -> PathBuf {
    let id = ai["id"].as_i64().unwrap_or(0);
    let name = ai["name"].as_str().unwrap_or("unnamed");
    let folder_id = ai["folder"].as_i64().unwrap_or(0);

    if let Some(s) = ai["path"].as_str() {
        let t = s.trim().trim_start_matches('/');
        if !t.is_empty() && !t.starts_with("##") {
            let p = safe_path_from_slash(t);
            if !p.as_os_str().is_empty() {
                return ensure_leek_extension(p);
            }
        }
    }

    if is_in_bin(folder_id, folder_map) {
        return PathBuf::from("trash")
            .join(id.to_string())
            .join(leek_file_name(name));
    }

    if folder_id != 0 && !folder_map.contains_key(&folder_id) {
        return PathBuf::from(format!("{}_{}", id, leek_file_name(name)));
    }

    let mut path = PathBuf::new();
    for seg in folder_prefix_components(folder_id, folder_map) {
        path.push(seg);
    }
    path.push(leek_file_name(name));
    path
}

fn ensure_leek_extension(mut p: PathBuf) -> PathBuf {
    if p.extension().and_then(|e| e.to_str()) != Some("leek") {
        p.set_extension("leek");
    }
    p
}

fn parse_folders(farmer: &Value) -> HashMap<i64, FolderMeta> {
    let mut m = HashMap::new();
    if let Some(arr) = farmer["folders"].as_array() {
        for f in arr {
            let Some(id) = f["id"].as_i64() else {
                continue;
            };
            let name = f["name"].as_str().unwrap_or("folder").to_string();
            let parent = f["folder"].as_i64().unwrap_or(0);
            m.insert(id, FolderMeta { id, name, parent });
        }
    }
    m
}

fn is_in_bin(mut folder_id: i64, folders: &HashMap<i64, FolderMeta>) -> bool {
    let mut guard = 0;
    while guard < 256 {
        guard += 1;
        if folder_id == -1 {
            return true;
        }
        if folder_id == 0 {
            return false;
        }
        let Some(f) = folders.get(&folder_id) else {
            return false;
        };
        folder_id = f.parent;
    }
    false
}

/// Path segments from root down to `folder_id` (excluding the folder id itself as filename).
fn folder_prefix_components(folder_id: i64, folders: &HashMap<i64, FolderMeta>) -> Vec<String> {
    if folder_id == 0 {
        return Vec::new();
    }
    let mut chain = Vec::new();
    let mut cur = folder_id;
    let mut guard = 0;
    while cur != 0 && guard < 256 {
        guard += 1;
        let Some(f) = folders.get(&cur) else {
            break;
        };
        chain.push(sanitize_segment(&f.name));
        cur = f.parent;
    }
    chain.reverse();
    chain
}

fn safe_path_from_slash(s: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in s.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        let p = sanitize_segment(part);
        if !p.is_empty() {
            out.push(p);
        }
    }
    out
}

fn leek_file_name(name: &str) -> String {
    let base = sanitize_segment(name);
    if base.ends_with(".leek") {
        base
    } else {
        format!("{base}.leek")
    }
}

fn sanitize_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn path_at_root() {
        let farmer = json!({ "folders": [], "ais": [] });
        let ai = json!({ "id": 42, "name": "Main", "folder": 0 });
        assert_eq!(
            ai_leek_relative_path(&farmer, &ai),
            PathBuf::from("Main.leek")
        );
    }

    #[test]
    fn path_in_subfolder() {
        let farmer = json!({
            "folders": [{ "id": 10, "name": "Lib", "folder": 0 }],
            "ais": []
        });
        let ai = json!({ "id": 1, "name": "X", "folder": 10 });
        assert_eq!(
            ai_leek_relative_path(&farmer, &ai),
            PathBuf::from("Lib/X.leek")
        );
    }
}
