//! Tournament range helpers (composition / leek level).

use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    pub async fn tournament_range_compo(&self, power: i64) -> Result<Value> {
        self.get_json(&format!("tournament/range-compo/{power}"))
            .await
    }

    pub async fn tournament_range_leek(&self, level: i64) -> Result<Value> {
        self.get_json(&format!("tournament/range-leek/{level}"))
            .await
    }
}
