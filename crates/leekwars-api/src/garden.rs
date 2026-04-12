//! Garden / potager (matchmaking) endpoints.

use serde_json::{Value, json};

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    /// Full garden state (queue, compositions, etc.).
    pub async fn garden_get(&self) -> Result<Value> {
        self.get_json("garden/get").await
    }

    pub async fn garden_get_leek_opponents(&self, leek_id: i64) -> Result<Value> {
        self.get_json(&format!("garden/get-leek-opponents/{leek_id}"))
            .await
    }

    pub async fn garden_get_farmer_opponents(&self) -> Result<Value> {
        self.get_json("garden/get-farmer-opponents").await
    }

    pub async fn garden_start_solo_fight(&self, leek_id: i64, target_id: i64) -> Result<Value> {
        self.post_json(
            "garden/start-solo-fight",
            &json!({ "leek_id": leek_id, "target_id": target_id }),
        )
        .await
    }

    pub async fn garden_start_farmer_fight(&self, target_id: i64) -> Result<Value> {
        self.post_json(
            "garden/start-farmer-fight",
            &json!({ "target_id": target_id }),
        )
        .await
    }

    /// Team garden: opponents for one of your compositions (`data.opponents` in the web client).
    pub async fn garden_get_composition_opponents(&self, composition_id: i64) -> Result<Value> {
        self.get_json(&format!(
            "garden/get-composition-opponents/{composition_id}"
        ))
        .await
    }

    /// `target_id` is the **enemy composition** id (same as the website).
    pub async fn garden_start_team_fight(
        &self,
        composition_id: i64,
        target_composition_id: i64,
    ) -> Result<Value> {
        self.post_json(
            "garden/start-team-fight",
            &json!({
                "composition_id": composition_id,
                "target_id": target_composition_id,
            }),
        )
        .await
    }
}
