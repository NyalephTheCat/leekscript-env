//! Public game data (`data/*`).

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::client::LeekWarsClient;
use crate::error::Result;

#[derive(Debug, Deserialize)]
pub struct DataVersion {
    pub master_version: String,
}

#[derive(Debug, Deserialize)]
pub struct GameDataAll {
    pub data: Value,
    pub hashes: HashMap<String, String>,
    pub master_version: String,
}

impl LeekWarsClient {
    pub async fn data_version(&self) -> Result<DataVersion> {
        self.get_json("data/version").await
    }

    pub async fn data_get_all(&self) -> Result<GameDataAll> {
        self.get_json("data/get-all").await
    }
}
