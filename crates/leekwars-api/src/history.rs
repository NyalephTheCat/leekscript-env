//! Fight history (`history/get-{kind}-history/{id}`).

use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    /// `kind`: `farmer`, `leek`, or `team` (matches the web client).
    pub async fn history_get(&self, kind: &str, id: i64) -> Result<Value> {
        let path = format!("history/get-{kind}-history/{id}");
        self.get_json(&path).await
    }
}
