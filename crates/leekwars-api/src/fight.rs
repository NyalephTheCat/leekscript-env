//! Fight replay, logs, and comments.

use serde_json::{Value, json};

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    pub async fn fight_get(&self, fight_id: i64) -> Result<Value> {
        self.get_json(&format!("fight/get/{fight_id}")).await
    }

    pub async fn fight_get_logs(&self, fight_id: i64) -> Result<Value> {
        self.get_json(&format!("fight/get-logs/{fight_id}")).await
    }

    pub async fn fight_comment(&self, fight_id: i64, comment: &str) -> Result<Value> {
        self.post_json(
            "fight/comment",
            &json!({ "fight_id": fight_id, "comment": comment }),
        )
        .await
    }
}
