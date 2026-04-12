//! Notifications.

use serde::Serialize;
use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

#[derive(Debug, Serialize)]
pub struct NotificationReadBody {
    pub notification_id: i64,
}

impl LeekWarsClient {
    pub async fn notification_get_latest(&self, limit: u32) -> Result<Value> {
        self.get_json(&format!("notification/get-latest/{limit}"))
            .await
    }

    pub async fn notification_read(&self, notification_id: i64) -> Result<Value> {
        self.post_json(
            "notification/read",
            &NotificationReadBody { notification_id },
        )
        .await
    }
}
