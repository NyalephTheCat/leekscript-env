//! Shapes for approximating fights locally: strip irrelevant fields, keep stats + loadout + AI metadata.

use std::thread;
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::api::{fetch_composition_rich_tooltip, fetch_leek_public, ApiError, RetryPolicy};
use ureq::Agent;

fn hat_template_from_api(v: Option<&Value>) -> i32 {
    match v {
        None | Some(Value::Null) => 0,
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) as i32,
        Some(Value::Object(o)) => o
            .get("template")
            .and_then(|x| x.as_i64())
            .unwrap_or(0) as i32,
        _ => 0,
    }
}

fn item_ids_from_api(arr: Option<&Value>) -> Vec<Value> {
    arr.and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|ent| ent.get("id").cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn component_templates_from_api(arr: Option<&Value>) -> Vec<Value> {
    arr.and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|ent| ent.get("template").cloned())
                .collect()
        })
        .unwrap_or_default()
}

/// One `scenario.entities[row][col]` object compatible with `leek_wars_gen`’s entity schema, derived from public `leek/get` JSON.
///
/// Uses **item** `id`s for `weapons` / `chips` (matches the fight DB), **template** ids for `components`, and copies base + `total_*` stats when present.
pub fn scenario_entity_from_leek_get(raw: &Value) -> Value {
    let farmer_id = raw
        .get("farmer")
        .and_then(|f| f.get("id"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0);

    let ai_path = raw
        .get("ai")
        .and_then(|a| a.get("path"))
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();

    let ai_version = raw
        .get("ai")
        .and_then(|a| a.get("version"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0) as i32;

    let mut ent = Map::new();
    ent.insert("id".into(), raw.get("id").cloned().unwrap_or(json!(0)));
    ent.insert("name".into(), raw.get("name").cloned().unwrap_or(json!("")));
    ent.insert("type".into(), json!(1));
    ent.insert("level".into(), raw.get("level").cloned().unwrap_or(json!(1)));
    ent.insert("life".into(), raw.get("life").cloned().unwrap_or(json!(1)));
    ent.insert("tp".into(), raw.get("tp").cloned().unwrap_or(json!(0)));
    ent.insert("mp".into(), raw.get("mp").cloned().unwrap_or(json!(0)));
    ent.insert(
        "strength".into(),
        raw.get("strength").cloned().unwrap_or(json!(0)),
    );
    ent.insert(
        "agility".into(),
        raw.get("agility").cloned().unwrap_or(json!(0)),
    );
    ent.insert(
        "wisdom".into(),
        raw.get("wisdom").cloned().unwrap_or(json!(0)),
    );
    ent.insert(
        "resistance".into(),
        raw.get("resistance").cloned().unwrap_or(json!(0)),
    );
    ent.insert(
        "science".into(),
        raw.get("science").cloned().unwrap_or(json!(0)),
    );
    ent.insert("magic".into(), raw.get("magic").cloned().unwrap_or(json!(0)));
    ent.insert(
        "frequency".into(),
        raw.get("frequency").cloned().unwrap_or(json!(0)),
    );
    ent.insert("cores".into(), raw.get("cores").cloned().unwrap_or(json!(0)));
    ent.insert("ram".into(), raw.get("ram").cloned().unwrap_or(json!(0)));
    for key in [
        "total_life",
        "total_strength",
        "total_agility",
        "total_wisdom",
        "total_resistance",
        "total_science",
        "total_magic",
        "total_frequency",
        "total_cores",
        "total_ram",
        "total_tp",
        "total_mp",
    ] {
        if let Some(v) = raw.get(key) {
            if !v.is_null() {
                ent.insert(key.into(), v.clone());
            }
        }
    }
    ent.insert("farmer".into(), json!(farmer_id));
    ent.insert("team".into(), json!(0));
    ent.insert("dead".into(), json!(false));
    ent.insert("skin".into(), raw.get("skin").cloned().unwrap_or(json!(0)));
    ent.insert(
        "hat".into(),
        json!(hat_template_from_api(raw.get("hat"))),
    );
    ent.insert(
        "metal".into(),
        raw.get("metal").cloned().unwrap_or(json!(false)),
    );
    ent.insert("face".into(), raw.get("face").cloned().unwrap_or(json!(0)));
    if let Some(t) = raw.get("title").filter(|v| !v.is_null()) {
        ent.insert("title".into(), t.clone());
    }
    if let Some(x) = raw.get("xp_blocked").filter(|v| !v.is_null()) {
        ent.insert("xp_blocked".into(), x.clone());
    }
    ent.insert("orientation".into(), json!(0));
    ent.insert("ai".into(), Value::String(ai_path));
    ent.insert("ai_folder".into(), json!(0));
    ent.insert("ai_version".into(), json!(ai_version));
    ent.insert("ai_strict".into(), json!(false));
    ent.insert("ai_owner".into(), json!(0));
    ent.insert("weapons".into(), json!(item_ids_from_api(raw.get("weapons"))));
    ent.insert("chips".into(), json!(item_ids_from_api(raw.get("chips"))));
    ent.insert(
        "components".into(),
        json!(component_templates_from_api(raw.get("components"))),
    );
    Value::Object(ent)
}

/// Subset of `leek/get` useful for building scenarios / tuning AIs against meta opponents.
pub fn leek_sim_profile(raw: &Value) -> Value {
    let weapon_templates = raw
        .get("weapons")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|w| w.get("template").cloned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let chip_templates = raw
        .get("chips")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("template").cloned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let components = raw
        .get("components")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();

    json!({
        "kind": "leek_sim_profile",
        "id": raw.get("id"),
        "name": raw.get("name"),
        "level": raw.get("level"),
        "xp": raw.get("xp"),
        "up_xp": raw.get("up_xp"),
        "down_xp": raw.get("down_xp"),
        "remaining_xp": raw.get("remaining_xp"),
        "xp_blocked": raw.get("xp_blocked"),
        "talent": raw.get("talent"),
        "max_talent": raw.get("max_talent"),
        "talent_more": raw.get("talent_more"),
        "ranking": raw.get("ranking"),
        "capital": raw.get("capital"),
        "in_garden": raw.get("in_garden"),
        "fight_record": {
            "victories": raw.get("victories"),
            "draws": raw.get("draws"),
            "defeats": raw.get("defeats"),
            "ratio": raw.get("ratio"),
        },
        "characteristics": {
            "life": raw.get("life"),
            "strength": raw.get("strength"),
            "wisdom": raw.get("wisdom"),
            "agility": raw.get("agility"),
            "resistance": raw.get("resistance"),
            "science": raw.get("science"),
            "magic": raw.get("magic"),
            "frequency": raw.get("frequency"),
            "cores": raw.get("cores"),
            "ram": raw.get("ram"),
            "tp": raw.get("tp"),
            "mp": raw.get("mp"),
        },
        "totals": {
            "total_life": raw.get("total_life"),
            "total_strength": raw.get("total_strength"),
            "total_wisdom": raw.get("total_wisdom"),
            "total_agility": raw.get("total_agility"),
            "total_resistance": raw.get("total_resistance"),
            "total_science": raw.get("total_science"),
            "total_magic": raw.get("total_magic"),
            "total_frequency": raw.get("total_frequency"),
            "total_cores": raw.get("total_cores"),
            "total_ram": raw.get("total_ram"),
            "total_tp": raw.get("total_tp"),
            "total_mp": raw.get("total_mp"),
        },
        "weapon_templates": weapon_templates,
        "chip_templates": chip_templates,
        "components": components,
        "weapon_slot": raw.get("weapon"),
        "max_weapons": raw.get("max_weapons"),
        "max_chips": raw.get("max_chips"),
        "hat": raw.get("hat"),
        "skin": raw.get("skin"),
        "metal": raw.get("metal"),
        "face": raw.get("face"),
        "title": raw.get("title"),
        "farmer": raw.get("farmer"),
        "ai_meta": raw.get("ai"),
    })
}

/// One leek: optional full `leek/get` plus derived profile.
pub fn leek_sim_export_body(raw: &Value, profile_only: bool) -> Value {
    if profile_only {
        leek_sim_profile(raw)
    } else {
        json!({
            "kind": "leek_sim_export",
            "raw": raw,
            "profile": leek_sim_profile(raw),
        })
    }
}

fn composition_leek_ids(summary: &Value) -> Vec<u64> {
    summary
        .get("leeks")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("id").and_then(|v| v.as_u64()))
                .collect()
        })
        .unwrap_or_default()
}

/// Full bundle: composition tooltip + `leek/get` for each roster leek (for chips/weapons).
pub fn fetch_composition_sim_bundle(
    agent: &Agent,
    base: &str,
    composition_id: u64,
    retry: &RetryPolicy,
    fetch_leek_sheets: bool,
    profile_only: bool,
    gap: Duration,
) -> Result<Value, ApiError> {
    let summary = fetch_composition_rich_tooltip(agent, base, composition_id, retry)?;
    if !fetch_leek_sheets {
        return Ok(json!({
            "kind": "composition_sim_bundle",
            "composition_id": composition_id,
            "summary_only": true,
            "summary": summary,
        }));
    }

    let ids = composition_leek_ids(&summary);
    let mut leeks = Vec::with_capacity(ids.len());
    for (i, id) in ids.iter().enumerate() {
        if i > 0 && !gap.is_zero() {
            thread::sleep(gap);
        }
        let raw = fetch_leek_public(agent, base, *id, retry)?;
        leeks.push(json!({
            "leek_id": id,
            "sheet": leek_sim_export_body(&raw, profile_only),
        }));
    }

    Ok(json!({
        "kind": "composition_sim_bundle",
        "composition_id": composition_id,
        "summary": summary,
        "leeks": leeks,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_keeps_templates() {
        let raw = json!({
            "id": 1,
            "name": "x",
            "level": 50,
            "weapons": [{"id": 10, "template": 42}],
            "chips": [{"id": 11, "template": 7}],
            "components": [],
            "life": 100,
            "total_life": 200,
            "ai": {"name": "bot", "level": 1, "valid": true}
        });
        let p = leek_sim_profile(&raw);
        assert_eq!(p["weapon_templates"], json!([42]));
        assert_eq!(p["chip_templates"], json!([7]));
        assert_eq!(p["ai_meta"]["name"], json!("bot"));
    }

    #[test]
    fn scenario_entity_maps_item_ids_and_totals() {
        let raw = json!({
            "id": 99,
            "name": "N",
            "level": 3,
            "life": 100,
            "total_life": 150,
            "tp": 10,
            "total_tp": 12,
            "mp": 5,
            "total_mp": 6,
            "strength": 1,
            "total_strength": 5,
            "agility": 0,
            "wisdom": 0,
            "resistance": 0,
            "science": 0,
            "magic": 0,
            "frequency": 100,
            "cores": 1,
            "ram": 10,
            "farmer": {"id": 7, "name": "F"},
            "skin": 2,
            "hat": {"template": 224, "id": 1},
            "weapons": [{"template": 1, "id": 1001}],
            "chips": [{"template": 2, "id": 2002}],
            "components": [{"template": 300, "id": 3}],
            "ai": {"name": "A", "path": "ia/A", "version": 4}
        });
        let ent = scenario_entity_from_leek_get(&raw);
        assert_eq!(ent["weapons"], json!([1001]));
        assert_eq!(ent["chips"], json!([2002]));
        assert_eq!(ent["components"], json!([300]));
        assert_eq!(ent["hat"], json!(224));
        assert_eq!(ent["total_life"], json!(150));
        assert_eq!(ent["farmer"], json!(7));
        assert_eq!(ent["ai"], json!("ia/A"));
        assert_eq!(ent["ai_version"], json!(4));
    }
}
