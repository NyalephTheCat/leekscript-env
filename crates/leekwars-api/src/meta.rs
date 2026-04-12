//! Miscellaneous public or common JSON endpoints (functions list, countries, changelog, stats, talents, API catalog).

use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    /// LeekScript / API function list (encyclopedia-style metadata).
    pub async fn function_get_all(&self) -> Result<Value> {
        self.get_json("function/get-all").await
    }

    pub async fn country_get_all(&self) -> Result<Value> {
        self.get_json("country/get-all").await
    }

    pub async fn changelog_get(&self, locale: &str) -> Result<Value> {
        self.get_json(&format!("changelog/get/{locale}")).await
    }

    pub async fn statistic_get_all(&self) -> Result<Value> {
        self.get_json("statistic/get-all").await
    }

    pub async fn talent_farmer(&self) -> Result<Value> {
        self.get_json("talent/farmer").await
    }

    pub async fn talent_leek(&self) -> Result<Value> {
        self.get_json("talent/leek").await
    }

    /// Service catalog for the in-game API browser (requires authentication).
    pub async fn service_get_all(&self) -> Result<Value> {
        self.get_json("service/get-all").await
    }
}
