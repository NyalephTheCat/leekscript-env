//! Parse `data/get-all` JSON: items, components, schemes (crafting).

use std::collections::HashMap;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct GameDataIndex {
    /// Component definition id → stat pairs (from `data.components`).
    pub components: HashMap<i64, ComponentDef>,
    /// Item template id → metadata.
    pub items: HashMap<i64, ItemInfo>,
    pub schemes: Vec<SchemeInfo>,
}

#[derive(Debug, Clone)]
pub struct ComponentDef {
    pub id: i64,
    pub name: String,
    pub stats: Vec<(String, i64)>,
}

#[derive(Debug, Clone)]
pub struct ItemInfo {
    pub id: i64,
    pub name: String,
    #[allow(dead_code)]
    pub item_type: i64,
    /// For `type == 8` (component), indexes into `components`.
    pub params: Option<i64>,
    /// From game data (`items`); used for acquisition hints.
    pub buyable: bool,
    pub market: bool,
}

#[derive(Debug, Clone)]
pub struct SchemeInfo {
    pub id: i64,
    pub result: i64,
    pub quantity: i64,
    /// Ingredient item template ids and counts; `None` is an empty forge cell.
    pub items: Vec<Option<(i64, i64)>>,
}

impl GameDataIndex {
    pub fn from_data_root(data: &Value) -> anyhow::Result<Self> {
        let mut components = HashMap::new();
        if let Some(obj) = data.get("components").and_then(|x| x.as_object()) {
            for (k, v) in obj {
                let id: i64 = k.parse().unwrap_or_else(|_| {
                    v.get("id")
                        .and_then(|x| x.as_i64())
                        .unwrap_or(0)
                });
                let name = v
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut stats = Vec::new();
                if let Some(arr) = v.get("stats").and_then(|x| x.as_array()) {
                    for pair in arr {
                        if let Some(a) = pair.as_array() {
                            let s = a
                                .first()
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string();
                            let n = a.get(1).and_then(|x| x.as_i64()).unwrap_or(0);
                            stats.push((s, n));
                        }
                    }
                }
                components.insert(
                    id,
                    ComponentDef {
                        id,
                        name,
                        stats,
                    },
                );
            }
        }

        let mut items = HashMap::new();
        if let Some(obj) = data.get("items").and_then(|x| x.as_object()) {
            for (k, v) in obj {
                let id: i64 = k.parse().unwrap_or_else(|_| v.get("id").and_then(|x| x.as_i64()).unwrap_or(0));
                let name = v
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let item_type = v.get("type").and_then(|x| x.as_i64()).unwrap_or(-1);
                let params = v.get("params").and_then(|x| x.as_i64());
                let buyable = v.get("buyable").and_then(|x| x.as_bool()).unwrap_or(false);
                let market = v.get("market").and_then(|x| x.as_bool()).unwrap_or(false);
                items.insert(
                    id,
                    ItemInfo {
                        id,
                        name,
                        item_type,
                        params,
                        buyable,
                        market,
                    },
                );
            }
        }

        let mut schemes = Vec::new();
        let scheme_values: Vec<&Value> = if let Some(obj) = data.get("schemes").and_then(|x| x.as_object()) {
            obj.values().collect()
        } else if let Some(arr) = data.get("schemes").and_then(|x| x.as_array()) {
            arr.iter().collect()
        } else {
            Vec::new()
        };
        for v in scheme_values {
            let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
            let result = v.get("result").and_then(|x| x.as_i64()).unwrap_or(0);
            let quantity = v.get("quantity").and_then(|x| x.as_i64()).unwrap_or(1);
            let mut items_parsed = Vec::new();
            if let Some(arr) = v.get("items").and_then(|x| x.as_array()) {
                for cell in arr {
                    if cell.is_null() {
                        items_parsed.push(None);
                        continue;
                    }
                    if let Some(pair) = cell.as_array() {
                        let a = pair.first().and_then(|x| x.as_i64()).unwrap_or(0);
                        let b = pair.get(1).and_then(|x| x.as_i64()).unwrap_or(0);
                        items_parsed.push(Some((a, b)));
                    }
                }
            }
            schemes.push(SchemeInfo {
                id,
                result,
                quantity,
                items: items_parsed,
            });
        }

        Ok(GameDataIndex {
            components,
            items,
            schemes,
        })
    }

    /// Stats granted by a component **item template** id (inventory / leek slot).
    pub fn stats_for_component_item_template(&self, item_template: i64) -> Option<Vec<(String, i64)>> {
        let item = self.items.get(&item_template)?;
        if item.item_type != 8 {
            return None;
        }
        let pid = item.params?;
        self.components.get(&pid).map(|c| c.stats.clone())
    }

    pub fn component_item_templates(&self) -> Vec<i64> {
        self.items
            .iter()
            .filter(|(_, i)| i.item_type == 8)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Schemes whose `result` is this item template (craft recipes producing `tpl`).
    pub fn schemes_crafting_template(&self, tpl: i64) -> Vec<&SchemeInfo> {
        self.schemes
            .iter()
            .filter(|s| s.result == tpl)
            .collect()
    }

    /// How to obtain more copies of this item template when `missing > 0` (or [`AcquisitionKind::Stock`] if not).
    pub fn acquisition_kind(&self, missing: i64, template: i64) -> AcquisitionKind {
        acquisition_kind_impl(self, missing, template)
    }
}

/// Classify how a player can obtain extra copies of an item (weapons, chips, hats, components).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionKind {
    /// `have >= need` (only meaningful when inventory was checked).
    Stock,
    /// Missing copies, but at least one forge recipe produces this template.
    Craftable,
    /// Missing copies; listed as buyable / on market in game data (and no recipe takes priority for the label).
    Market,
    /// Missing copies; not craftable from schemes and not market-buyable in data (trophies, events, …).
    Other,
}

fn acquisition_kind_impl(data: &GameDataIndex, missing: i64, template: i64) -> AcquisitionKind {
    if missing <= 0 {
        return AcquisitionKind::Stock;
    }
    if !data.schemes_crafting_template(template).is_empty() {
        return AcquisitionKind::Craftable;
    }
    if let Some(item) = data.items.get(&template) {
        if item.buyable || item.market {
            return AcquisitionKind::Market;
        }
    }
    AcquisitionKind::Other
}

pub fn sum_stats_maps(maps: impl Iterator<Item = Vec<(String, i64)>>) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for m in maps {
        for (k, v) in m {
            *out.entry(k).or_insert(0) += v;
        }
    }
    out
}
