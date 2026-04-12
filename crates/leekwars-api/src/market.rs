//! Market (buy / sell / templates).

use serde::Serialize;
use serde_json::{Value, json};

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    pub async fn market_get_item_templates(&self) -> Result<Value> {
        self.get_json("market/get-item-templates").await
    }

    pub async fn market_buy_habs_quantity<I: Serialize>(
        &self,
        item_id: &I,
        quantity: i64,
    ) -> Result<Value> {
        self.post_json(
            "market/buy-habs-quantity",
            &json!({ "item_id": item_id, "quantity": quantity }),
        )
        .await
    }

    pub async fn market_buy_crystals_quantity<I: Serialize>(
        &self,
        item_id: &I,
        quantity: i64,
    ) -> Result<Value> {
        self.post_json(
            "market/buy-crystals-quantity",
            &json!({ "item_id": item_id, "quantity": quantity }),
        )
        .await
    }

    pub async fn market_sell_habs(&self, item_id: i64) -> Result<Value> {
        self.post_json("market/sell-habs", &json!({ "item_id": item_id }))
            .await
    }

    pub async fn market_item_seen(&self, item_id: i64) -> Result<Value> {
        self.post_json("market/item-seen", &json!({ "item": item_id }))
            .await
    }

    pub async fn market_sound_played(&self) -> Result<Value> {
        self.post_json("market/sound-played", &json!({})).await
    }
}
