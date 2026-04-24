//! Scenario file format (subset used by `com.leekwars.generator.scenario.Scenario.fromFile`).
//!
//! Extra keys in JSON (e.g. `map`, `max_operations_per_entity`) are preserved in [`Scenario::extra`].

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Root document for a fight scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    #[serde(default = "default_max_turns", rename = "max_turns")]
    pub max_turns: i32,
    #[serde(rename = "random_seed")]
    pub random_seed: Option<i32>,
    pub farmers: Vec<FarmerInfo>,
    pub teams: Vec<TeamInfo>,
    pub entities: Vec<Vec<EntityInfo>>,
    /// Official generator: `Scenario.drawCheckLife`: if no unique surviving team, pick by higher total team life (teams 0 vs 1).
    #[serde(default, rename = "draw_check_life")]
    pub draw_check_life: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn default_max_turns() -> i32 {
    64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarmerInfo {
    pub id: i32,
    pub name: String,
    pub country: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamInfo {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInfo {
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub r#type: i32,
    pub level: i32,
    pub life: i32,
    pub tp: i32,
    pub mp: i32,
    pub strength: i32,
    #[serde(default)]
    pub agility: i32,
    #[serde(default)]
    pub wisdom: i32,
    #[serde(default)]
    pub resistance: i32,
    #[serde(default)]
    pub science: i32,
    #[serde(default)]
    pub magic: i32,
    #[serde(default)]
    pub frequency: i32,
    #[serde(default)]
    pub cores: i32,
    #[serde(default)]
    pub ram: i32,
    #[serde(default)]
    pub farmer: i32,
    #[serde(default)]
    pub team: i32,
    #[serde(default)]
    pub dead: bool,
    #[serde(default)]
    pub skin: i32,
    #[serde(default)]
    pub hat: i32,
    #[serde(default)]
    pub metal: bool,
    #[serde(default)]
    pub face: i32,
    #[serde(default)]
    pub orientation: i32,
    #[serde(default)]
    pub ai: String,
    #[serde(default, rename = "ai_folder")]
    pub ai_folder: i32,
    #[serde(default, rename = "ai_path")]
    pub ai_path: Option<String>,
    #[serde(default, rename = "ai_version")]
    pub ai_version: i32,
    #[serde(default, rename = "ai_strict")]
    pub ai_strict: bool,
    #[serde(default, rename = "ai_owner")]
    pub ai_owner: i32,
    #[serde(default)]
    pub weapons: Vec<i32>,
    #[serde(default)]
    pub chips: Vec<i32>,
    #[serde(default)]
    pub cell: Option<i32>,
}

impl Scenario {
    /// `map.width` / `map.height` from JSON when present (official scenarios embed a `map` object).
    pub fn map_size(&self) -> (i32, i32) {
        self.extra
            .get("map")
            .and_then(|v| v.as_object())
            .and_then(|o| {
                let w = o.get("width")?.as_i64()? as i32;
                let h = o.get("height")?.as_i64()? as i32;
                Some((w, h))
            })
            .unwrap_or((17, 17))
    }

    /// Grid size used by the official generator’s `State.init` → `Map.generateMap(..., 18, 18, ...)`.
    ///
    /// `Scenario.fromFile` in the official generator does not copy the JSON `map` object into the scenario, so `Main`
    /// runs fights on this fixed size regardless of `map.width` / `map.height` in the file.
    /// The Rust engine uses the same dimensions when simulating that jar path.
    pub fn engine_map_size_java_main(&self) -> (i32, i32) {
        let _ = self;
        (18, 18)
    }

    /// Parse `map.obstacles` when present (official generator `Actions.addMap` serializes it).
    ///
    /// Returns `cell_id -> obstacle_size_or_id` (we store the raw JSON integer value).
    pub fn map_obstacles(&self) -> BTreeMap<i32, i32> {
        let mut out = BTreeMap::new();
        let Some(map) = self.extra.get("map").and_then(|v| v.as_object()) else {
            return out;
        };
        let Some(obstacles) = map.get("obstacles").and_then(|v| v.as_object()) else {
            return out;
        };
        for (k, v) in obstacles {
            let Ok(cell_id) = k.parse::<i32>() else {
                continue;
            };
            let val = v.as_i64().unwrap_or(0) as i32;
            out.insert(cell_id, val);
        }
        out
    }

    /// `map.type` when present (official generator `Map.getType()`), defaulting to `0`.
    pub fn map_type(&self) -> i32 {
        self.extra
            .get("map")
            .and_then(|v| v.as_object())
            .and_then(|o| o.get("type"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32
    }

    pub fn from_path(path: &Path) -> Result<Self, crate::GenError> {
        let bytes = std::fs::read(path)?;
        let mut s: Scenario = serde_json::from_slice(&bytes)?;
        if s.random_seed.is_none() {
            // Match `new Scenario()` in the official generator: 1 ..= MAX_VALUE
            s.random_seed = Some(
                1 + (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| (d.as_nanos() % i32::MAX as u128) as i32)
                    .unwrap_or(1)
                    .abs()
                    % i32::MAX),
            );
        }
        Ok(s)
    }

    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scenario1_fixture_shape() {
        let j = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../leek-wars-generator/test/scenario/scenario1.json"
        ));
        let s: Scenario = serde_json::from_str(j).expect("parse");
        assert_eq!(s.farmers.len(), 2);
        assert_eq!(s.entities.len(), 2);
        assert_eq!(s.random_seed, Some(1_234_567));
    }
}
