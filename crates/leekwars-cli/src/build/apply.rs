//! Apply another leek’s equipment layout to your leek (with TOML backup).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use leekwars_api::LeekWarsClient;
use serde_json::Value;

use super::data::GameDataIndex;
use super::export;

fn default_backup_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("leekwars")
        .join("backups")
}

fn farmer_owns_leek(farmer: &Value, leek_id: i64) -> bool {
    if let Some(obj) = farmer.get("leeks").and_then(|x| x.as_object()) {
        if obj.contains_key(&leek_id.to_string()) {
            return true;
        }
        for v in obj.values() {
            if v.get("id").and_then(|x| x.as_i64()) == Some(leek_id) {
                return true;
            }
        }
    }
    if let Some(arr) = farmer.get("leeks").and_then(|x| x.as_array()) {
        for v in arr {
            if v.get("id").and_then(|x| x.as_i64()) == Some(leek_id) {
                return true;
            }
        }
    }
    false
}

/// template id → inventory row ids (FIFO from pooled pushes).
#[derive(Debug, Default)]
struct InventoryPool {
    weapons: HashMap<i64, Vec<i64>>,
    chips: HashMap<i64, Vec<i64>>,
    components: HashMap<i64, Vec<i64>>,
}

fn pool_push_template(
    pool: &mut HashMap<i64, Vec<i64>>,
    template: i64,
    inventory_row_id: i64,
    quantity: i64,
) {
    if template <= 0 || inventory_row_id <= 0 {
        return;
    }
    let n = quantity.max(1);
    let e = pool.entry(template).or_default();
    for _ in 0..n {
        e.push(inventory_row_id);
    }
}

fn pool_take(pool: &mut HashMap<i64, Vec<i64>>, template: i64) -> Option<i64> {
    let v = pool.get_mut(&template)?;
    v.pop()
}

/// Items currently on the leek return to stash when stripped; include them for dry-run checks.
fn push_equipped_into_pool(leek: &Value, out: &mut InventoryPool) {
    for w in leek
        .get("weapons")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
    {
        let id = w.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        let tpl = w.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
        pool_push_template(&mut out.weapons, tpl, id, 1);
    }
    for ch in leek
        .get("chips")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
    {
        let id = ch.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        let tpl = ch.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
        pool_push_template(&mut out.chips, tpl, id, 1);
    }
    for cell in leek
        .get("components")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
    {
        if cell.is_null() {
            continue;
        }
        let id = cell.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        let tpl = cell.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
        pool_push_template(&mut out.components, tpl, id, 1);
    }
}

fn fill_pool_from_farmer(farmer: &Value, out: &mut InventoryPool) {
    for it in farmer
        .get("weapons")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
    {
        let id = it.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        let tpl = it.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
        let qty = it.get("quantity").and_then(|x| x.as_i64()).unwrap_or(1);
        pool_push_template(&mut out.weapons, tpl, id, qty);
    }
    for it in farmer
        .get("chips")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
    {
        let id = it.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        let tpl = it.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
        let qty = it.get("quantity").and_then(|x| x.as_i64()).unwrap_or(1);
        pool_push_template(&mut out.chips, tpl, id, qty);
    }
    for it in farmer
        .get("components")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
    {
        let id = it.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        let tpl = it.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
        let qty = it.get("quantity").and_then(|x| x.as_i64()).unwrap_or(1);
        pool_push_template(&mut out.components, tpl, id, qty);
    }
}

async fn strip_leek_loadout(c: &LeekWarsClient, leek: &Value) -> anyhow::Result<()> {
    let leek_id = leek
        .get("id")
        .and_then(|x| x.as_i64())
        .ok_or_else(|| anyhow::anyhow!("leek has no id"))?;

    if let Some(arr) = leek.get("weapons").and_then(|x| x.as_array()) {
        for w in arr {
            let wid = w.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
            if wid > 0 {
                c.leek_remove_weapon(wid).await?;
            }
        }
    }
    if let Some(arr) = leek.get("chips").and_then(|x| x.as_array()) {
        for ch in arr {
            let cid = ch.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
            if cid > 0 {
                c.leek_remove_chip(cid).await?;
            }
        }
    }
    if let Some(h) = leek.get("hat") {
        if !h.is_null() {
            c.leek_remove_hat(leek_id).await?;
        }
    }
    let comp_ids: Vec<i64> = leek
        .get("components")
        .and_then(|x| x.as_array())
        .into_iter()
        .flatten()
        .filter_map(|cell| {
            if cell.is_null() {
                return None;
            }
            cell.get("id").and_then(|x| x.as_i64())
        })
        .collect();
    for cid in comp_ids {
        if cid > 0 {
            c.leek_remove_component(cid).await?;
        }
    }
    Ok(())
}

pub struct ApplyReport {
    pub backup_path: Option<PathBuf>,
    pub dry_run: bool,
    pub weapons_placed: usize,
    pub chips_placed: usize,
    pub hat_set: bool,
    pub components_placed: usize,
}

pub async fn apply_mirror_loadout(
    c: &LeekWarsClient,
    my_leek_id: i64,
    target_leek_id: i64,
    backup_dir: Option<PathBuf>,
    dry_run: bool,
) -> anyhow::Result<ApplyReport> {
    let session = c.farmer_get_from_token().await?;
    let farmer = session
        .get("farmer")
        .ok_or_else(|| anyhow::anyhow!("session has no farmer"))?;
    if !farmer_owns_leek(farmer, my_leek_id) {
        anyhow::bail!("leek {my_leek_id} is not in your account (check --leek)");
    }

    let my_leek = c.leek_get(my_leek_id).await?;
    let target = c.leek_get(target_leek_id).await?;

    let data = c.data_get_all().await.context("data/get-all")?;
    let gd = GameDataIndex::from_data_root(&data.data)?;

    let backup_path = if dry_run {
        None
    } else {
        let dir = backup_dir.unwrap_or_else(default_backup_dir);
        std::fs::create_dir_all(&dir).with_context(|| dir.display().to_string())?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let path = dir.join(format!("leek-{my_leek_id}-before-apply-{ts}.toml"));
        let leek_doc = export::leek_build_from_json(&my_leek, Some(&gd))?;
        let file = export::BuildFileV1 {
            schema_version: 1,
            kind: "leek",
            farmer: None,
            leeks: vec![leek_doc],
        };
        let text = export::to_toml(&file)?;
        std::fs::write(&path, &text).with_context(|| path.display().to_string())?;
        Some(path)
    };

    if dry_run {
        dry_run_plan(farmer, &my_leek, &target)?;
        return Ok(ApplyReport {
            backup_path: None,
            dry_run: true,
            weapons_placed: 0,
            chips_placed: 0,
            hat_set: false,
            components_placed: 0,
        });
    }

    strip_leek_loadout(c, &my_leek).await?;
    let session = c.farmer_get_from_token().await?;
    let farmer = session
        .get("farmer")
        .ok_or_else(|| anyhow::anyhow!("session has no farmer"))?;

    let mut pool = InventoryPool::default();
    fill_pool_from_farmer(farmer, &mut pool);

    let mut weapons_placed = 0usize;
    let mut chips_placed = 0usize;
    let mut hat_set = false;
    let mut components_placed = 0usize;

    if let Some(arr) = target.get("weapons").and_then(|x| x.as_array()) {
        for w in arr {
            let tpl = w.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            let inv = pool_take(&mut pool.weapons, tpl).ok_or_else(|| {
                anyhow::anyhow!(
                    "not enough weapons with template {tpl} in inventory after clearing loadout (need one more for target layout)"
                )
            })?;
            c.leek_add_weapon(my_leek_id, inv).await?;
            weapons_placed += 1;
        }
    }

    if let Some(arr) = target.get("chips").and_then(|x| x.as_array()) {
        for ch in arr {
            let tpl = ch.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            let inv = pool_take(&mut pool.chips, tpl).ok_or_else(|| {
                anyhow::anyhow!(
                    "not enough chips with template {tpl} in inventory (need one more for target layout)"
                )
            })?;
            c.leek_add_chip(my_leek_id, inv).await?;
            chips_placed += 1;
        }
    }

    if let Some(h) = target.get("hat") {
        if !h.is_null() {
            let tpl = h
                .get("template")
                .and_then(|x| x.as_i64())
                .ok_or_else(|| anyhow::anyhow!("hat has no template"))?;
            c.leek_set_hat(my_leek_id, tpl).await?;
            hat_set = true;
        }
    }

    if let Some(arr) = target.get("components").and_then(|x| x.as_array()) {
        for (idx, cell) in arr.iter().take(8).enumerate() {
            if cell.is_null() {
                continue;
            }
            let tpl = cell
                .get("template")
                .and_then(|x| x.as_i64())
                .unwrap_or(0);
            let inv = pool_take(&mut pool.components, tpl).ok_or_else(|| {
                anyhow::anyhow!(
                    "not enough components with template {tpl} in inventory for slot {idx}"
                )
            })?;
            c.leek_add_component(my_leek_id, inv, idx as i64).await?;
            components_placed += 1;
        }
    }

    Ok(ApplyReport {
        backup_path,
        dry_run: false,
        weapons_placed,
        chips_placed,
        hat_set,
        components_placed,
    })
}

/// Simulate stash after a strip: farmer inventory plus what your leek is wearing, then check target is satisfiable.
fn dry_run_plan(farmer: &Value, my_leek: &Value, target: &Value) -> anyhow::Result<()> {
    let mut pool = InventoryPool::default();
    fill_pool_from_farmer(farmer, &mut pool);
    push_equipped_into_pool(my_leek, &mut pool);

    if let Some(arr) = target.get("weapons").and_then(|x| x.as_array()) {
        for w in arr {
            let tpl = w.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            pool_take(&mut pool.weapons, tpl).ok_or_else(|| {
                anyhow::anyhow!(
                    "dry-run: insufficient weapon template {tpl} in stash (inventory + current loadout)"
                )
            })?;
        }
    }

    if let Some(arr) = target.get("chips").and_then(|x| x.as_array()) {
        for ch in arr {
            let tpl = ch.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            pool_take(&mut pool.chips, tpl).ok_or_else(|| {
                anyhow::anyhow!(
                    "dry-run: insufficient chip template {tpl} in stash (inventory + current loadout)"
                )
            })?;
        }
    }

    if let Some(arr) = target.get("components").and_then(|x| x.as_array()) {
        for (idx, cell) in arr.iter().take(8).enumerate() {
            if cell.is_null() {
                continue;
            }
            let tpl = cell.get("template").and_then(|x| x.as_i64()).unwrap_or(0);
            pool_take(&mut pool.components, tpl).ok_or_else(|| {
                anyhow::anyhow!(
                    "dry-run: insufficient component template {tpl} for slot {idx} (stash + current loadout)"
                )
            })?;
        }
    }

    Ok(())
}

/// Default directory for `build apply` backups (`--backup-dir` overrides).
pub fn default_apply_backup_dir() -> PathBuf {
    default_backup_dir()
}
