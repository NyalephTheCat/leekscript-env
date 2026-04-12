//! Compare your inventory to another leek's components (mirror / shopping list).

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;

use super::data::{sum_stats_maps, AcquisitionKind, GameDataIndex};
use super::export::leek_build_from_json;

/// Count item templates equipped on a leek (`weapons` / `chips` arrays).
fn count_templates_on_leek(leek: &Value, array_key: &str) -> HashMap<i64, usize> {
    let mut m = HashMap::new();
    if let Some(arr) = leek.get(array_key).and_then(|x| x.as_array()) {
        for it in arr {
            let tpl = it.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            if tpl > 0 {
                *m.entry(tpl).or_insert(0) += 1;
            }
        }
    }
    m
}

/// Count items in farmer stash (`farmer.weapons` / `chips` / `hats` / `components`).
fn count_farmer_inventory(farmer: &Value, array_key: &str) -> HashMap<i64, i64> {
    let mut m = HashMap::new();
    if let Some(arr) = farmer.get(array_key).and_then(|x| x.as_array()) {
        for it in arr {
            let tpl = it.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            let qty = it.get("quantity").and_then(|x| x.as_i64()).unwrap_or(1);
            if tpl > 0 {
                *m.entry(tpl).or_insert(0) += qty;
            }
        }
    }
    m
}

/// Count component item templates on a leek (equipped).
fn count_component_templates(leek: &Value) -> HashMap<i64, usize> {
    let mut m = HashMap::new();
    if let Some(arr) = leek.get("components").and_then(|x| x.as_array()) {
        for c in arr {
            if c.is_null() {
                continue;
            }
            let tpl = c.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            if tpl > 0 {
                *m.entry(tpl).or_insert(0) += 1;
            }
        }
    }
    m
}

pub struct MirrorReport {
    pub target_name: String,
    pub target_level: i64,
    pub target_talent: Option<i64>,
    pub inventory_checked: bool,
    pub weapons: Vec<EquipmentRow>,
    pub chips: Vec<EquipmentRow>,
    pub hat: Option<EquipmentRow>,
    pub component_diff: Vec<ComponentDiffRow>,
    pub schemes_hint: Vec<String>,
}

#[derive(Serialize)]
pub struct EquipmentRow {
    pub template: i64,
    pub name: String,
    pub need: usize,
    /// `None` when `--no-inventory` / not logged in.
    pub have: Option<i64>,
    pub missing: i64,
    pub acquisition: AcquisitionKind,
}

#[derive(Serialize)]
pub struct ComponentDiffRow {
    pub template: i64,
    pub name: String,
    pub need: usize,
    pub have: Option<i64>,
    pub missing: i64,
    pub acquisition: AcquisitionKind,
}

fn template_name(data: &GameDataIndex, tpl: i64) -> String {
    data.items
        .get(&tpl)
        .map(|i| i.name.clone())
        .unwrap_or_else(|| format!("template_{tpl}"))
}

fn rows_from_need_map(
    need: HashMap<i64, usize>,
    inv: &HashMap<i64, i64>,
    inventory_checked: bool,
    data: &GameDataIndex,
) -> Vec<EquipmentRow> {
    let mut templates: Vec<i64> = need.keys().copied().collect();
    templates.sort_unstable();
    let mut out = Vec::new();
    for tpl in templates {
        let n_need = need[&tpl];
        let have = if inventory_checked {
            Some(*inv.get(&tpl).unwrap_or(&0))
        } else {
            None
        };
        let have_n = have.unwrap_or(0);
        let missing = (n_need as i64 - have_n).max(0);
        let acquisition = data.acquisition_kind(missing, tpl);
        out.push(EquipmentRow {
            template: tpl,
            name: template_name(data, tpl),
            need: n_need,
            have,
            missing,
            acquisition,
        });
    }
    out
}

fn hat_row(
    leek: &Value,
    inv_hats: &HashMap<i64, i64>,
    inventory_checked: bool,
    data: &GameDataIndex,
) -> Option<EquipmentRow> {
    let h = leek.get("hat")?;
    if h.is_null() {
        return None;
    }
    let tpl = h.get("template").and_then(|x| x.as_i64())?;
    if tpl <= 0 {
        return None;
    }
    let need = 1usize;
    let have = if inventory_checked {
        Some(*inv_hats.get(&tpl).unwrap_or(&0))
    } else {
        None
    };
    let have_n = have.unwrap_or(0);
    let missing = (need as i64 - have_n).max(0);
    let acquisition = data.acquisition_kind(missing, tpl);
    Some(EquipmentRow {
        template: tpl,
        name: template_name(data, tpl),
        need,
        have,
        missing,
        acquisition,
    })
}

pub fn mirror_to_target(
    target_leek: &Value,
    my_farmer: Option<&Value>,
    data: &GameDataIndex,
) -> anyhow::Result<MirrorReport> {
    let built = leek_build_from_json(target_leek, Some(data))?;
    let target_name = built.name.clone();
    let target_level = built.level;
    let target_talent = built.talent;

    let inventory_checked = my_farmer.is_some();
    let inv_weapons = my_farmer
        .map(|f| count_farmer_inventory(f, "weapons"))
        .unwrap_or_default();
    let inv_chips = my_farmer
        .map(|f| count_farmer_inventory(f, "chips"))
        .unwrap_or_default();
    let inv_hats = my_farmer
        .map(|f| count_farmer_inventory(f, "hats"))
        .unwrap_or_default();
    let inv_components = my_farmer
        .map(|f| count_farmer_inventory(f, "components"))
        .unwrap_or_default();

    let weapons = rows_from_need_map(
        count_templates_on_leek(target_leek, "weapons"),
        &inv_weapons,
        inventory_checked,
        data,
    );
    let chips = rows_from_need_map(
        count_templates_on_leek(target_leek, "chips"),
        &inv_chips,
        inventory_checked,
        data,
    );
    let hat = hat_row(target_leek, &inv_hats, inventory_checked, data);

    let need = count_component_templates(target_leek);

    let mut component_diff = Vec::new();
    let mut schemes_hint = Vec::new();

    for (&tpl, &n_need) in &need {
        let name = template_name(data, tpl);
        let have = if inventory_checked {
            Some(*inv_components.get(&tpl).unwrap_or(&0))
        } else {
            None
        };
        let have_n = have.unwrap_or(0);
        let missing = (n_need as i64 - have_n).max(0);
        let acquisition = data.acquisition_kind(missing, tpl);
        component_diff.push(ComponentDiffRow {
            template: tpl,
            name,
            need: n_need,
            have,
            missing,
            acquisition,
        });

        if missing > 0 {
            let schemes = data.schemes_crafting_template(tpl);
            for s in schemes {
                let mut parts = Vec::new();
                for ing in &s.items {
                    if let Some((item_tpl, qty)) = ing {
                        let iname = data
                            .items
                            .get(item_tpl)
                            .map(|i| i.name.clone())
                            .unwrap_or_else(|| item_tpl.to_string());
                        parts.push(format!("{iname} x{qty}"));
                    }
                }
                schemes_hint.push(format!(
                    "craft {} x{} via scheme {}: {}",
                    data.items.get(&tpl).map(|i| i.name.as_str()).unwrap_or("?"),
                    s.quantity,
                    s.id,
                    parts.join(", ")
                ));
            }
        }
    }

    component_diff.sort_by(|a, b| a.template.cmp(&b.template));

    Ok(MirrorReport {
        target_name,
        target_level,
        target_talent,
        inventory_checked,
        weapons,
        chips,
        hat,
        component_diff,
        schemes_hint,
    })
}

/// Sum of stat deltas from target's equipped components (using game data).
pub fn target_component_stat_sum(target_leek: &Value, data: &GameDataIndex) -> HashMap<String, i64> {
    let maps = target_leek
        .get("components")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
        .filter_map(|c| {
            if c.is_null() {
                return None;
            }
            let tpl = c.get("template").and_then(|x| x.as_i64())?;
            data.stats_for_component_item_template(tpl)
        });
    sum_stats_maps(maps)
}
