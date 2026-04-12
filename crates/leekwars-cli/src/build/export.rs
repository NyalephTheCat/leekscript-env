//! Export leek / farmer builds to TOML.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use super::data::GameDataIndex;
use super::capital_cost::{base_stat, capital_spent_for_invested, CAPITAL_STATS};

const SCHEMA: u32 = 1;

#[derive(Serialize)]
pub struct BuildFileV1 {
    pub schema_version: u32,
    /// `"leek"` or `"farmer"`
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub farmer: Option<FarmerHeader>,
    pub leeks: Vec<LeekBuildV1>,
}

#[derive(Serialize)]
pub struct FarmerHeader {
    pub id: i64,
    pub name: String,
}

#[derive(Serialize)]
pub struct LeekBuildV1 {
    pub id: i64,
    pub name: String,
    pub level: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub talent: Option<i64>,
    /// Raw characteristics from `leek/get` (capital + base, **without** component bonuses).
    pub characteristics: BTreeMap<String, i64>,
    /// Full totals including components (`total_*` from API).
    pub totals: BTreeMap<String, i64>,
    /// Same keys as `leek/spend-capital` `characteristics` JSON — point-buy amounts only.
    pub pointbuy: BTreeMap<String, i64>,
    /// Estimated capital already spent on the current pointbuy (informational).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_capital_spent: Option<i64>,
    pub weapons: Vec<EquipRow>,
    pub chips: Vec<EquipRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hat: Option<HatRow>,
    /// Equipped components with slot index (0..8). Empty slots omitted.
    pub components: Vec<ComponentSlotExport>,
}

#[derive(Serialize)]
pub struct ComponentSlotExport {
    pub slot: usize,
    #[serde(flatten)]
    pub row: ComponentRow,
}

#[derive(Serialize)]
pub struct EquipRow {
    pub instance_id: i64,
    pub template: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct HatRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<i64>,
    pub template: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct ComponentRow {
    pub instance_id: i64,
    pub template: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<BTreeMap<String, i64>>,
}

fn json_i64(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_i64())
}

fn item_name(data: Option<&GameDataIndex>, template: i64) -> Option<String> {
    data?.items.get(&template).map(|i| i.name.clone())
}

pub fn leek_build_from_json(leek: &Value, data: Option<&GameDataIndex>) -> anyhow::Result<LeekBuildV1> {
    let id = json_i64(leek, "id").unwrap_or(0);
    let name = leek
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let level = json_i64(leek, "level").unwrap_or(1);

    let mut characteristics = BTreeMap::new();
    let mut totals = BTreeMap::new();
    let mut pointbuy = BTreeMap::new();
    let mut spent = 0i64;

    for stat in CAPITAL_STATS {
        let raw = json_i64(leek, stat).unwrap_or(base_stat(level, stat));
        characteristics.insert(stat.to_string(), raw);
        let tot_key = format!("total_{stat}");
        let tot = leek
            .get(tot_key.as_str())
            .and_then(|x| x.as_i64())
            .unwrap_or(raw);
        totals.insert(stat.to_string(), tot);
        let inv = raw - base_stat(level, stat);
        pointbuy.insert(stat.to_string(), inv);
        spent += capital_spent_for_invested(stat, inv);
    }

    let mut weapons = Vec::new();
    if let Some(arr) = leek.get("weapons").and_then(|x| x.as_array()) {
        for w in arr {
            let instance_id = json_i64(w, "id").unwrap_or(0);
            let template = json_i64(w, "template").unwrap_or(0);
            weapons.push(EquipRow {
                instance_id,
                template,
                name: item_name(data, template),
            });
        }
    }

    let mut chips = Vec::new();
    if let Some(arr) = leek.get("chips").and_then(|x| x.as_array()) {
        for w in arr {
            let instance_id = json_i64(w, "id").unwrap_or(0);
            let template = json_i64(w, "template").unwrap_or(0);
            chips.push(EquipRow {
                instance_id,
                template,
                name: item_name(data, template),
            });
        }
    }

    let hat = leek.get("hat").and_then(|h| {
        if h.is_null() {
            return None;
        }
        let template = json_i64(h, "template")?;
        Some(HatRow {
            instance_id: json_i64(h, "id"),
            template,
            name: item_name(data, template),
        })
    });

    let mut components: Vec<ComponentSlotExport> = Vec::new();
    if let Some(arr) = leek.get("components").and_then(|x| x.as_array()) {
        for (slot, c) in arr.iter().enumerate() {
            if c.is_null() {
                continue;
            }
            let instance_id = json_i64(c, "id").unwrap_or(0);
            let template = json_i64(c, "template").unwrap_or(0);
            let stats = data.and_then(|d| d.stats_for_component_item_template(template));
            let stats_map = stats.map(|pairs| pairs.into_iter().collect());
            components.push(ComponentSlotExport {
                slot,
                row: ComponentRow {
                    instance_id,
                    template,
                    name: item_name(data, template),
                    stats: stats_map,
                },
            });
        }
    }

    Ok(LeekBuildV1 {
        id,
        name,
        level,
        talent: json_i64(leek, "talent"),
        characteristics,
        totals,
        pointbuy,
        estimated_capital_spent: Some(spent),
        weapons,
        chips,
        hat,
        components,
    })
}

pub fn farmer_leek_ids(farmer: &Value) -> Vec<i64> {
    let mut ids = Vec::new();
    if let Some(obj) = farmer.get("leeks").and_then(|x| x.as_object()) {
        for (k, v) in obj {
            if let Ok(id) = k.parse::<i64>() {
                ids.push(id);
            } else if let Some(id) = json_i64(v, "id") {
                ids.push(id);
            }
        }
    } else if let Some(arr) = farmer.get("leeks").and_then(|x| x.as_array()) {
        for v in arr {
            if let Some(id) = json_i64(v, "id") {
                ids.push(id);
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub async fn build_farmer_export(
    client: &leekwars_api::LeekWarsClient,
    farmer_id: i64,
    data: Option<&GameDataIndex>,
) -> anyhow::Result<BuildFileV1> {
    let v = client.farmer_get(farmer_id).await?;
    let farmer = v
        .get("farmer")
        .ok_or_else(|| anyhow::anyhow!("farmer/get/{farmer_id}: missing farmer"))?;
    let fid = json_i64(farmer, "id").unwrap_or(farmer_id);
    let name = farmer
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();

    let ids = farmer_leek_ids(farmer);
    let mut leeks = Vec::new();
    for lid in ids {
        let lj = client.leek_get(lid).await?;
        leeks.push(leek_build_from_json(&lj, data)?);
    }

    Ok(BuildFileV1 {
        schema_version: SCHEMA,
        kind: "farmer",
        farmer: Some(FarmerHeader { id: fid, name }),
        leeks,
    })
}

pub async fn build_leek_export(
    client: &leekwars_api::LeekWarsClient,
    leek_id: i64,
    data: Option<&GameDataIndex>,
) -> anyhow::Result<BuildFileV1> {
    let lj = client.leek_get(leek_id).await?;
    let leek = leek_build_from_json(&lj, data)?;
    Ok(BuildFileV1 {
        schema_version: SCHEMA,
        kind: "leek",
        farmer: None,
        leeks: vec![leek],
    })
}

pub fn to_toml(doc: &BuildFileV1) -> anyhow::Result<String> {
    Ok(toml::to_string_pretty(doc)?)
}
