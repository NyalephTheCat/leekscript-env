//! Private messages and conversations.

use serde_json::{Value, json};

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    pub async fn message_get_messages(
        &self,
        conversation_id: i64,
        count: u32,
        offset: u32,
    ) -> Result<Value> {
        let path = format!("message/get-messages/{conversation_id}/{count}/{offset}");
        self.get_json(&path).await
    }

    pub async fn message_get_conversations(&self, offset: u32, limit: u32) -> Result<Value> {
        let path = format!("message/get-conversations/{offset}/{limit}");
        self.get_json(&path).await
    }

    pub async fn message_find_conversation(&self, farmer_id: i64) -> Result<Value> {
        self.get_json(&format!("message/find-conversation/{farmer_id}"))
            .await
    }

    pub async fn message_toggle_notifications(&self, conversation_id: i64) -> Result<Value> {
        self.post_json(
            "message/toggle-notifications",
            &json!({ "conversation_id": conversation_id }),
        )
        .await
    }

    pub async fn message_quit_conversation(&self, conversation_id: i64) -> Result<Value> {
        self.post_json(
            "message/quit-conversation",
            &json!({ "conversation_id": conversation_id }),
        )
        .await
    }

    pub async fn message_complete_pseudo(
        &self,
        conversation_id: i64,
        pseudo: &str,
    ) -> Result<Value> {
        self.post_json(
            "message/complete-pseudo",
            &json!({ "conversation_id": conversation_id, "pseudo": pseudo }),
        )
        .await
    }

    pub async fn message_create_conversation(
        &self,
        farmer_id: i64,
        message: &str,
    ) -> Result<Value> {
        self.post_json(
            "message/create-conversation",
            &json!({ "farmer_id": farmer_id, "message": message }),
        )
        .await
    }

    pub async fn message_send_message(&self, conversation_id: i64, message: &str) -> Result<Value> {
        self.post_json(
            "message/send-message",
            &json!({ "conversation_id": conversation_id, "message": message }),
        )
        .await
    }

    pub async fn message_read(&self, conversation_id: i64) -> Result<Value> {
        self.post_json(
            "message/read",
            &json!({ "conversation_id": conversation_id }),
        )
        .await
    }

    pub async fn message_censor(&self, message_ids: &[i64], mute: bool) -> Result<Value> {
        self.post_json(
            "message/censor",
            &json!({ "messages": message_ids, "mute": mute }),
        )
        .await
    }

    pub async fn message_delete(&self, message_ids: &[i64], mute: bool) -> Result<Value> {
        self.delete_json(
            "message/delete",
            &json!({ "messages": message_ids, "mute": mute }),
        )
        .await
    }

    pub async fn message_mute(
        &self,
        target_id: i64,
        chat_id: i64,
        duration_seconds: i64,
    ) -> Result<Value> {
        self.post_json(
            "message/mute",
            &json!({
                "target_id": target_id,
                "chat": chat_id,
                "duration": duration_seconds,
            }),
        )
        .await
    }
}
