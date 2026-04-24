//! Load `data/chips.json` from the official generator (keyed by chip id).

use crate::error::GenError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ChipEffect {
    pub id: i32,
    pub value1: f64,
    pub value2: f64,
    pub turns: i32,
    pub targets: i32,
    pub modifiers: i32,
    pub r#type: i32,
}

#[derive(Debug, Clone)]
pub struct ChipStats {
    pub chip_id: i32,
    pub name: String,
    pub template_id: i32,
    pub cost: i32,
    pub min_range: i32,
    pub max_range: i32,
    pub los: bool,
    pub launch_type: i32,
    pub area: i32,
    pub effects: Vec<ChipEffect>,
    /// Official generator: `Chip.getCooldown()` (0 = none; -1 → stored as `State.MAX_TURNS + 2`).
    pub cooldown: i32,
    pub team_cooldown: bool,
    /// Turns before first use at fight start (Official generator `Chip.initialCooldown`); applied as `initial + 1` in `FightWorld::from_scenario`.
    pub initial_cooldown: i32,
    /// Official generator: `Attack.getMaxUses()`; `-1` = unlimited (field often omitted in JSON).
    pub max_uses: i32,
}

#[derive(Debug, Deserialize)]
struct ChipEntry {
    id: i32,
    name: String,
    template: i32,
    cost: i32,
    min_range: i32,
    max_range: i32,
    los: serde_json::Value,
    launch_type: i32,
    area: i32,
    #[serde(default)]
    cooldown: i32,
    #[serde(default)]
    team_cooldown: bool,
    #[serde(default)]
    initial_cooldown: i32,
    #[serde(default = "default_max_uses_unlimited")]
    max_uses: i32,
    #[serde(default)]
    effects: Vec<ChipEffectEntry>,
}

fn default_max_uses_unlimited() -> i32 {
    -1
}

#[derive(Debug, Deserialize)]
struct ChipEffectEntry {
    id: i32,
    #[serde(default)]
    value1: serde_json::Value,
    #[serde(default)]
    value2: serde_json::Value,
    #[serde(default)]
    turns: i32,
    #[serde(default)]
    targets: i32,
    #[serde(default)]
    modifiers: i32,
    #[serde(default)]
    r#type: i32,
}

fn json_as_bool(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
        serde_json::Value::String(s) => matches!(s.as_str(), "true" | "True" | "1"),
        _ => false,
    }
}

fn json_as_f64(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        serde_json::Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

pub fn load_chips_json(path: &Path) -> Result<HashMap<i32, ChipStats>, GenError> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let root: HashMap<String, ChipEntry> = serde_json::from_str(&raw)?;
    let mut by_id = HashMap::new();
    for (_k, c) in root {
        let effects = c
            .effects
            .into_iter()
            .map(|e| ChipEffect {
                id: e.id,
                value1: json_as_f64(&e.value1),
                value2: json_as_f64(&e.value2),
                turns: e.turns,
                targets: e.targets,
                modifiers: e.modifiers,
                r#type: e.r#type,
            })
            .collect::<Vec<_>>();
        by_id.insert(
            c.id,
            ChipStats {
                chip_id: c.id,
                name: c.name,
                template_id: c.template,
                cost: c.cost,
                min_range: c.min_range,
                max_range: c.max_range,
                los: json_as_bool(&c.los),
                launch_type: c.launch_type,
                area: c.area,
                cooldown: c.cooldown,
                team_cooldown: c.team_cooldown,
                initial_cooldown: c.initial_cooldown,
                max_uses: c.max_uses,
                effects,
            },
        );
    }
    Ok(by_id)
}
