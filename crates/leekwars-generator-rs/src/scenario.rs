use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct FarmerInfo {
    pub id: i32,
    pub name: String,
    pub country: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamInfo {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntityInfo {
    pub id: i32,
    pub ai: Option<String>,
    pub name: String,
    pub r#type: i32,
    pub farmer: i32,
    pub team: i32,

    #[serde(default)]
    pub level: Option<i32>,

    #[serde(default)]
    pub cell: Option<i32>,

    #[serde(default)]
    pub weapons: Vec<i32>,

    #[serde(default)]
    pub chips: Vec<i32>,

    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    pub farmers: Vec<FarmerInfo>,
    pub teams: Vec<TeamInfo>,
    pub entities: Vec<Vec<EntityInfo>>,

    #[serde(default)]
    pub map: Option<Value>,

    #[serde(default, rename = "random_seed")]
    pub random_seed: Option<i32>,

    #[serde(default, rename = "max_turns")]
    pub max_turns: Option<i32>,

    #[serde(default, rename = "max_operations_per_entity")]
    pub max_operations_per_entity: Option<i64>,

    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, Value>,
}
