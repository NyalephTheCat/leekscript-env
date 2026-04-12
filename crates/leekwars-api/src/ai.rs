//! AI source sync, save, and download.

use serde::Deserialize;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::client::LeekWarsClient;
use crate::error::Result;

/// One entry returned by `ai/sync` (see `leek-wars` editor filesystem).
#[derive(Debug, Deserialize)]
pub struct AiSyncEntry {
    pub id: i64,
    pub modified: i64,
    pub code: String,
}

/// Request body for `ai/sync`: `ais` must be a **string** containing JSON (double-encoded), matching the web client.
#[derive(Debug, Serialize)]
pub struct AiSyncRequest {
    pub ais: String,
}

#[derive(Debug, Serialize)]
pub struct AiSaveRequest<'a> {
    pub ai_id: i64,
    pub code: &'a str,
}

impl LeekWarsClient {
    /// Sync AI sources: `timestamps` maps AI id → last known `modified` (as in localStorage `ai/time/`).
    pub async fn ai_sync(&self, timestamps: &[(i64, i64)]) -> Result<Vec<AiSyncEntry>> {
        let mut obj = Map::new();
        for &(id, t) in timestamps {
            obj.insert(id.to_string(), json!(t));
        }
        let map = Value::Object(obj);
        let body = AiSyncRequest {
            ais: serde_json::to_string(&map)?,
        };
        self.post_json("ai/sync", &body).await
    }

    /// Save AI source code.
    pub async fn ai_save(&self, ai_id: i64, code: &str) -> Result<Value> {
        let body = AiSaveRequest { ai_id, code };
        self.post_json("ai/save", &body).await
    }

    /// Absolute URL to download AI source in the browser (`GET`, cookies / auth as for other requests).
    pub fn ai_download_url(&self, ai_id: i64) -> Result<String> {
        let u = self.endpoint_url(&format!("ai/download/{ai_id}"))?;
        Ok(u.to_string())
    }

    /// Download AI source bytes (authenticated).
    pub async fn ai_download(&self, ai_id: i64) -> Result<Vec<u8>> {
        let path = format!("ai/download/{ai_id}");
        self.get_bytes(&path).await
    }

    /// Move AI to trash.
    pub async fn ai_delete(&self, ai_id: i64) -> Result<Value> {
        self.delete_json("ai/delete", &json!({ "ai_id": ai_id }))
            .await
    }

    /// Permanently delete AI.
    pub async fn ai_destroy(&self, ai_id: i64) -> Result<Value> {
        self.delete_json("ai/destroy", &json!({ "ai_id": ai_id }))
            .await
    }

    /// Restore AI from trash.
    pub async fn ai_restore(&self, ai_id: i64) -> Result<Value> {
        self.post_json("ai/restore", &json!({ "ai_id": ai_id }))
            .await
    }

    pub async fn ai_rename(&self, ai_id: i64, new_name: &str) -> Result<Value> {
        self.post_json(
            "ai/rename",
            &json!({ "ai_id": ai_id, "new_name": new_name }),
        )
        .await
    }

    /// Create a new AI in a folder (`version` is LeekScript level, e.g. `4`).
    pub async fn ai_new_name(&self, folder_id: i64, version: i32, name: &str) -> Result<Value> {
        self.post_json(
            "ai/new-name",
            &json!({ "folder_id": folder_id, "version": version, "name": name }),
        )
        .await
    }

    pub async fn ai_change_folder(&self, ai_id: i64, folder_id: i64) -> Result<Value> {
        self.post_json(
            "ai/change-folder",
            &json!({ "ai_id": ai_id, "folder_id": folder_id }),
        )
        .await
    }

    pub async fn ai_folder_change_folder(
        &self,
        folder_id: i64,
        dest_folder_id: i64,
    ) -> Result<Value> {
        self.post_json(
            "ai-folder/change-folder",
            &json!({ "folder_id": folder_id, "dest_folder_id": dest_folder_id }),
        )
        .await
    }

    pub async fn ai_folder_delete(&self, folder_id: i64) -> Result<Value> {
        self.delete_json("ai-folder/delete", &json!({ "folder_id": folder_id }))
            .await
    }

    pub async fn ai_folder_restore(&self, folder_id: i64) -> Result<Value> {
        self.post_json("ai-folder/restore", &json!({ "folder_id": folder_id }))
            .await
    }

    pub async fn ai_test_scenario(&self, scenario_id: i64, ai_id: i64) -> Result<Value> {
        self.post_json(
            "ai/test-scenario",
            &json!({ "scenario_id": scenario_id, "ai_id": ai_id }),
        )
        .await
    }

    pub async fn ai_set_version(&self, ai_id: i64, version: i32) -> Result<Value> {
        self.put_json("ai/version", &json!({ "ai_id": ai_id, "version": version }))
            .await
    }

    pub async fn ai_set_strict(&self, ai_id: i64, strict: bool) -> Result<Value> {
        self.put_json("ai/strict", &json!({ "ai_id": ai_id, "strict": strict }))
            .await
    }

    /// Empty the AI recycle bin.
    pub async fn ai_bin_empty(&self) -> Result<Value> {
        self.delete_json("ai/bin", &json!({})).await
    }
}
