//! Load `data/weapons.json` from the official generator (keyed by **weapon template id**: `1`, `12`, …).

use crate::error::GenError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use super::chips::ChipEffect;

#[derive(Debug, Clone)]
pub struct WeaponStats {
    pub item_id: i32,
    /// Official generator: weapon template id (used in action logs / UI), from `weapons.json`.
    pub template_id: i32,
    pub name: String,
    pub cost: i32,
    pub min_range: i32,
    pub max_range: i32,
    /// Whether the attack needs line-of-sight (`weapons.json` `los`).
    pub los: bool,
    /// Launch type (`weapons.json` `launch_type`), Official generator `Attack.LAUNCH_TYPE_*`.
    pub launch_type: i32,
    /// Official generator: `Attack.getArea()` type (`weapons.json` `area`).
    pub area: i32,
    /// Full effect list (same shape as chips.json).
    pub effects: Vec<ChipEffect>,
    /// Passive effects (Official generator `Weapon.getPassiveEffects()` / `weapons.json` `passive_effects`).
    pub passive_effects: Vec<ChipEffect>,
}

#[derive(Debug, Deserialize)]
struct WeaponEntry {
    item: i32,
    template: i32,
    name: String,
    cost: i32,
    min_range: i32,
    max_range: i32,
    los: serde_json::Value,
    launch_type: i32,
    area: i32,
    #[serde(default)]
    effects: Vec<WeaponEffectEntry>,
    #[serde(default)]
    passive_effects: Vec<WeaponEffectEntry>,
}

#[derive(Debug, Deserialize)]
struct WeaponEffectEntry {
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
    #[serde(default, rename = "type")]
    ty: i32,
}

fn json_as_f64(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        serde_json::Value::Bool(b) if *b => 1.0,
        _ => 0.0,
    }
}

fn json_as_bool(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
        serde_json::Value::String(s) => matches!(s.as_str(), "true" | "True" | "1"),
        _ => false,
    }
}

/// Build `template_id -> stats` (last template wins if duplicates).
pub fn load_weapons_json(path: &Path) -> Result<HashMap<i32, WeaponStats>, GenError> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let root: HashMap<String, WeaponEntry> = serde_json::from_str(&raw)?;
    let mut by_template = HashMap::new();
    for (_tpl_key, w) in root {
        let effects = w
            .effects
            .into_iter()
            .map(|e| ChipEffect {
                id: e.id,
                value1: json_as_f64(&e.value1),
                value2: json_as_f64(&e.value2),
                turns: e.turns,
                targets: e.targets,
                modifiers: e.modifiers,
                r#type: e.ty,
            })
            .collect::<Vec<_>>();
        let passive_effects = w
            .passive_effects
            .into_iter()
            .map(|e| ChipEffect {
                id: e.id,
                value1: json_as_f64(&e.value1),
                value2: json_as_f64(&e.value2),
                turns: e.turns,
                targets: e.targets,
                modifiers: e.modifiers,
                r#type: e.ty,
            })
            .collect::<Vec<_>>();
        by_template.insert(
            w.template,
            WeaponStats {
                item_id: w.item,
                template_id: w.template,
                name: w.name,
                cost: w.cost,
                min_range: w.min_range,
                max_range: w.max_range,
                los: json_as_bool(&w.los),
                launch_type: w.launch_type,
                area: w.area,
                effects,
                passive_effects,
            },
        );
    }
    Ok(by_template)
}
