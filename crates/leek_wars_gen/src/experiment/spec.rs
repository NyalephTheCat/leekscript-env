//! TOML experiment specification.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct ExperimentSpec {
    /// Scenario path (relative to cwd or absolute).
    pub scenario: PathBuf,
    /// Generator root (AI + data). Default: manifest / `LEEK_GENERATOR_CWD` resolved at run time.
    #[serde(default)]
    pub generator_root: Option<PathBuf>,
    /// Directory for manifest, runs.ndjson, cache, per-run artifacts.
    pub output_dir: PathBuf,
    #[serde(default)]
    pub seeds: SeedsSpec,
    /// Use the same seed ordering for every arm (variance reduction).
    #[serde(default)]
    pub paired_seeds: bool,
    #[serde(default)]
    pub arms: Vec<ArmSpec>,
    /// Optional JSON file: `{ "weapons": [...], "chips": [...] }` for sampling loadouts.
    #[serde(default)]
    pub loadout_preset: Option<PathBuf>,
    /// Probability in `[0, 1]` to dual-run Java vs Rust on a task (slow).
    #[serde(default)]
    pub java_verify_rate: f64,
    /// Enable Rust-only trace (sidecar `trace.jsonl` per run when trace enabled).
    #[serde(default)]
    pub trace: Option<ExperimentTraceSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SeedsSpec {
    List { list: Vec<i32> },
    Range {
        range: RangeSpec,
    },
}

impl Default for SeedsSpec {
    fn default() -> Self {
        SeedsSpec::List { list: vec![0] }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RangeSpec {
    pub start: i32,
    pub end: i32,
    #[serde(default = "default_step")]
    pub step: i32,
}

fn default_step() -> i32 {
    1
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ArmSpec {
    pub name: String,
    /// `scenario-relative AI path` → `const name` → list of JSON values (Cartesian product across names).
    #[serde(default)]
    pub tunables: HashMap<String, HashMap<String, Vec<serde_json::Value>>>,
    /// If non-empty, each entry is one variant: file path → const → value (sparse grid).
    #[serde(default)]
    pub variants: Vec<HashMap<String, HashMap<String, serde_json::Value>>>,
    /// Override entity AI: team index (0-based) and entity index → `ai` path string for scenario JSON.
    #[serde(default)]
    pub ai_overrides: Vec<AiOverrideSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiOverrideSpec {
    pub team: usize,
    pub entity: usize,
    pub ai: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExperimentTraceSpec {
    #[serde(default = "default_trace_max")]
    pub max_events: usize,
}

fn default_trace_max() -> usize {
    10_000
}

impl ExperimentSpec {
    pub fn from_toml_path(path: &std::path::Path) -> Result<Self, crate::GenError> {
        let raw = std::fs::read_to_string(path)?;
        toml::from_str(&raw).map_err(|e| crate::GenError::Message(e.to_string()))
    }
}
