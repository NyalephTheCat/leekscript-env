//! Scenario file format (subset used by `com.leekwars.generator.scenario.Scenario.fromFile`).
//!
//! Extra keys in JSON (e.g. `map`, `max_operations_per_entity`) are preserved in [`Scenario::extra`].

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

fn deserialize_hat_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    let v: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(match v {
        None | Some(serde_json::Value::Null) => 0,
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0) as i32,
        Some(serde_json::Value::Object(map)) => map
            .get("template")
            .and_then(|x| x.as_i64())
            .unwrap_or(0) as i32,
        _ => 0,
    })
}

fn deserialize_item_id_list<'de, D>(deserializer: D) -> Result<Vec<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Item {
        Id(i32),
        Obj { id: i32 },
    }
    let list: Vec<Item> = Vec::deserialize(deserializer)?;
    Ok(list
        .into_iter()
        .map(|x| match x {
            Item::Id(i) => i,
            Item::Obj { id } => id,
        })
        .collect())
}

fn deserialize_component_templates<'de, D>(deserializer: D) -> Result<Vec<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Comp {
        Template(i32),
        Obj { template: i32 },
    }
    let list: Vec<Comp> = Vec::deserialize(deserializer)?;
    Ok(list
        .into_iter()
        .map(|c| match c {
            Comp::Template(t) => t,
            Comp::Obj { template } => template,
        })
        .collect())
}

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
    /// Equipment-adjusted stats from API `leek/get` (`total_*`). Combat uses these when set (see [`crate::fight::world::FightWorld::from_scenario`]).
    #[serde(default)]
    pub total_life: Option<i32>,
    #[serde(default)]
    pub total_strength: Option<i32>,
    #[serde(default)]
    pub total_agility: Option<i32>,
    #[serde(default)]
    pub total_wisdom: Option<i32>,
    #[serde(default)]
    pub total_resistance: Option<i32>,
    #[serde(default)]
    pub total_science: Option<i32>,
    #[serde(default)]
    pub total_magic: Option<i32>,
    #[serde(default)]
    pub total_frequency: Option<i32>,
    #[serde(default)]
    pub total_cores: Option<i32>,
    #[serde(default)]
    pub total_ram: Option<i32>,
    #[serde(default)]
    pub total_tp: Option<i32>,
    #[serde(default)]
    pub total_mp: Option<i32>,
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
    #[serde(default, deserialize_with = "deserialize_hat_i32")]
    pub hat: i32,
    #[serde(default)]
    pub metal: bool,
    #[serde(default)]
    pub face: i32,
    #[serde(default)]
    pub title: Vec<i32>,
    #[serde(default)]
    pub xp_blocked: bool,
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
    #[serde(default, deserialize_with = "deserialize_item_id_list")]
    pub weapons: Vec<i32>,
    #[serde(default, deserialize_with = "deserialize_item_id_list")]
    pub chips: Vec<i32>,
    /// Puce / component **template** ids (API `components`); carried for export / tooling; combat loop may not use yet.
    #[serde(default, deserialize_with = "deserialize_component_templates")]
    pub components: Vec<i32>,
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

    #[test]
    fn parses_leek_get_style_entity() {
        let j = r#"{
            "id": 1,
            "name": "L",
            "type": 1,
            "level": 10,
            "life": 100,
            "total_life": 150,
            "tp": 10,
            "mp": 5,
            "total_tp": 12,
            "total_mp": 6,
            "strength": 1,
            "total_strength": 9,
            "agility": 0,
            "total_agility": 2,
            "wisdom": 0,
            "total_wisdom": 3,
            "resistance": 0,
            "total_resistance": 4,
            "science": 0,
            "total_science": 5,
            "magic": 0,
            "total_magic": 0,
            "frequency": 100,
            "total_frequency": 110,
            "cores": 1,
            "total_cores": 2,
            "ram": 10,
            "total_ram": 11,
            "farmer": 1,
            "team": 1,
            "skin": 0,
            "hat": {"template": 224, "id": 1},
            "weapons": [{"template": 38, "id": 2495072}],
            "chips": [{"template": 15, "id": 2493080}],
            "components": [{"template": 300, "id": 1}]
        }"#;
        let e: EntityInfo = serde_json::from_str(j).expect("parse entity");
        assert_eq!(e.hat, 224);
        assert_eq!(e.total_life, Some(150));
        assert_eq!(e.weapons, vec![2495072]);
        assert_eq!(e.chips, vec![2493080]);
        assert_eq!(e.components, vec![300]);
    }
}
