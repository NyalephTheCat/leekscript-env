//! Stable JSON shapes for ranking exports (`lw-meta ranking …`).

use serde::{Deserialize, Serialize};

/// One page of ranking data from `ranking/get` or `ranking/get-active`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingResponse {
    pub pages: u32,
    pub total: u32,
    pub ranking: Vec<serde_json::Value>,
}

/// Normalized leek row (worldwide active ranking).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeekRankingRow {
    pub rank: u32,
    pub id: u64,
    pub name: String,
    pub talent: u32,
    pub level: u32,
    pub xp: u64,
    pub farmer_id: u64,
    pub farmer: String,
    #[serde(default)]
    pub country: Option<String>,
    /// Absent or JSON `null` when the leek has no team.
    #[serde(default)]
    pub team_id: Option<u64>,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Normalized team row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRankingRow {
    pub rank: u32,
    pub id: u64,
    pub name: String,
    pub level: u32,
    pub talent: u32,
    pub total_level: u32,
    pub xp: u64,
    pub leek_count: u32,
    pub farmer_count: u32,
    #[serde(default)]
    pub active: Option<bool>,
}

/// Normalized team composition row (squad within a team).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionRankingRow {
    pub rank: u32,
    pub id: u64,
    pub name: String,
    pub talent: u32,
    pub total_level: u32,
    pub leek_count: u32,
    pub team_id: u64,
    pub team_name: String,
}

/// Bundle written by `lw-meta ranking --top N`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaExport {
    pub source: String,
    pub category: String,
    pub order: String,
    pub active_only: bool,
    pub country: Option<String>,
    pub fetched_rows: usize,
    pub pages: u32,
    pub total_entities: u32,
    pub leeks: Option<Vec<LeekRankingRow>>,
    pub teams: Option<Vec<TeamRankingRow>>,
    pub compositions: Option<Vec<CompositionRankingRow>>,
}

/// Entry from `service/get-all` (API catalog).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    pub module: String,
    pub function: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub auth: Option<bool>,
    #[serde(default)]
    pub parameters: Vec<String>,
    #[serde(default)]
    pub parameters_types: Vec<String>,
    #[serde(default)]
    pub returns: Vec<String>,
    #[serde(default)]
    pub returns_types: Vec<String>,
    #[serde(default)]
    pub example_url: Option<String>,
    #[serde(default)]
    pub deprecated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCatalogExport {
    pub source: String,
    pub services: Vec<ServiceDescriptor>,
    #[serde(default)]
    pub filtered_modules: Vec<String>,
}
