//! CLI wiring for `leekwars build`.

use std::collections::HashMap;
use std::io::stdout;

use anyhow::Context as _;
use leekwars_api::LeekWarsClient;
use serde_json::Value;

use super::apply;
use super::capital_cost::CAPITAL_STATS;
use super::compare::{mirror_to_target, target_component_stat_sum, ComponentDiffRow, EquipmentRow};
use super::data::{AcquisitionKind, GameDataIndex};
use super::export;
use super::optimize::{invested_map_from_leek_json, target_totals_from_leek_json, OptimizeInput, optimize};
use crate::batch::ui::Styles;
use crate::cli::{BuildCmd, Cli};

async fn load_game_data(client: &LeekWarsClient) -> anyhow::Result<GameDataIndex> {
    let all = client.data_get_all().await.context("data/get-all")?;
    GameDataIndex::from_data_root(&all.data)
}

fn parse_i64_kv(s: &str) -> anyhow::Result<HashMap<String, i64>> {
    let mut m = HashMap::new();
    if s.trim().is_empty() {
        return Ok(m);
    }
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("expected stat=value in {:?}", part))?;
        let key = k.trim().to_string();
        let n: i64 = v.trim().parse().context("stat value")?;
        m.insert(key, n);
    }
    Ok(m)
}

fn parse_f64_kv(s: &str) -> anyhow::Result<HashMap<String, f64>> {
    let mut m = HashMap::new();
    if s.trim().is_empty() {
        return Ok(m);
    }
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("expected stat=value in {:?}", part))?;
        let key = k.trim().to_string();
        let n: f64 = v.trim().parse().context("weight value")?;
        m.insert(key, n);
    }
    Ok(m)
}

pub async fn run_build(cmd: &BuildCmd, cli: &Cli) -> anyhow::Result<()> {
    let client = crate::session::client()?;
    match cmd {
        BuildCmd::Export {
            leek,
            farmer,
            out,
            no_game_data,
        } => {
            let data = if *no_game_data {
                None
            } else {
                Some(load_game_data(&client).await?)
            };
            let data_ref = data.as_ref();
            let doc = match (leek, farmer) {
                (Some(lid), None) => export::build_leek_export(&client, *lid, data_ref).await?,
                (None, Some(fid)) => export::build_farmer_export(&client, *fid, data_ref).await?,
                _ => anyhow::bail!("specify exactly one of --leek ID or --farmer ID"),
            };
            let text = export::to_toml(&doc)?;
            match out {
                Some(path) => {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(path, &text)?;
                    eprintln!("Wrote {}", path.display());
                }
                None => print!("{text}"),
            }
        }
        BuildCmd::Mirror {
            leek: leek_id,
            no_inventory,
        } => {
            let data = load_game_data(&client).await?;
            let target = client.leek_get(*leek_id).await?;
            let my_farmer: Option<Value> = if *no_inventory {
                None
            } else {
                let (login, password) = crate::session::auth(cli)?;
                let mut c = crate::session::client()?;
                c.farmer_login(&login, &password, false).await?;
                let session = c.farmer_get_from_token().await?;
                session.get("farmer").cloned()
            };

            let farmer_ref = my_farmer.as_ref();
            let report = mirror_to_target(&target, farmer_ref, &data)?;

            let comp_stats = target_component_stat_sum(&target, &data);
            let invested = invested_map_from_leek_json(&target);

            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "target": { "name": report.target_name, "level": report.target_level, "talent": report.target_talent },
                        "inventory_checked": report.inventory_checked,
                        "weapons": report.weapons,
                        "chips": report.chips,
                        "hat": report.hat,
                        "target_component_stat_bonus": comp_stats,
                        "target_pointbuy_invested": invested,
                        "components": report.component_diff,
                        "craft_hints": report.schemes_hint,
                    })
                );
            } else {
                let styles = Styles::new(cli.color.use_color(&stdout()));
                let s = &styles;
                println!(
                    "{b}Target{b0} {n} — level {lv} — talent {tal:?}",
                    b = s.bold,
                    b0 = s.reset,
                    n = report.target_name,
                    lv = report.target_level,
                    tal = report.target_talent
                );
                if !report.inventory_checked {
                    println!(
                        "{}! Log in (omit --no-inventory) to compare weapons/chips/hats/components to your stash.{r}",
                        s.yellow,
                        r = s.reset
                    );
                }
                print_mirror_equipment_section(s, "Weapons", &report.weapons);
                print_mirror_equipment_section(s, "Chips", &report.chips);
                println!("\n{b}Hat{b0}", b = s.bold, b0 = s.reset);
                match &report.hat {
                    Some(hat) => print_mirror_equipment_rows(s, std::slice::from_ref(hat)),
                    None => println!("  {d}(none){d0}", d = s.dim, d0 = s.reset),
                }
                println!("\n{b}Equipped component stat bonus{b0} (game data):", b = s.bold, b0 = s.reset);
                let mut keys: Vec<_> = comp_stats.keys().cloned().collect();
                keys.sort();
                for k in keys {
                    println!("  {}{}: {}{}", s.cyan, k, comp_stats[&k], s.reset);
                }
                println!(
                    "\n{b}Components{b0} (template = item id in market / inventory)",
                    b = s.bold,
                    b0 = s.reset
                );
                if report.component_diff.is_empty() {
                    println!("  {d}(none equipped){d0}", d = s.dim, d0 = s.reset);
                } else {
                    for r in &report.component_diff {
                        print_mirror_component_row(s, r);
                    }
                }
                if !report.schemes_hint.is_empty() {
                    println!(
                        "\n{b}Forge recipes{b0} (for missing templates):",
                        b = s.bold,
                        b0 = s.reset
                    );
                    for line in &report.schemes_hint {
                        println!("  {}{}{}", s.dim, line, s.reset);
                    }
                }
                println!(
                    "\n{d}Source:{d0} {g}in stock{r}  {y}craftable{r}  {b}market{r}  {m}other/special{r}  ({d}forge / buy flags from game data; not live market prices{r})",
                    d = s.dim,
                    d0 = s.reset,
                    g = s.green,
                    y = s.yellow,
                    b = s.blue,
                    m = s.magenta,
                    r = s.reset,
                );
                println!(
                    "\n{b}Their point-buy (invested){b0} — match via capital / restat, then mirror loadout:",
                    b = s.bold,
                    b0 = s.reset
                );
                for stat in CAPITAL_STATS {
                    if let Some(v) = invested.get(stat) {
                        if *v != 0 {
                            println!("  {}: {}", stat, v);
                        }
                    }
                }
            }
        }
        BuildCmd::Optimize {
            target_leek,
            totals,
            level,
            capital,
            weights,
            all_component_templates,
            allow_components,
            restarts,
            hill_steps,
            seed,
        } => {
            let data = load_game_data(&client).await?;
            let (target_totals, level_val) = if let Some(lid) = target_leek {
                let lj = client.leek_get(*lid).await?;
                let lv = lj
                    .get("level")
                    .and_then(|x| x.as_i64())
                    .unwrap_or(1);
                (target_totals_from_leek_json(&lj), lv)
            } else {
                let lv = level.ok_or_else(|| anyhow::anyhow!("--level is required with --totals"))?;
                let m = parse_i64_kv(
                    totals
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("--totals or --target-leek"))?,
                )?;
                for s in CAPITAL_STATS {
                    if !m.contains_key(s) {
                        anyhow::bail!(
                            "--totals must include {s} (and all other capital stats); or use --target-leek",
                        );
                    }
                }
                (m, lv)
            };

            let w = match weights {
                Some(s) => parse_f64_kv(s)?,
                None => HashMap::new(),
            };

            let allowed = if *all_component_templates {
                data.component_item_templates()
            } else if let Some(s) = allow_components {
                let mut v = Vec::new();
                for p in s.split(',') {
                    let p = p.trim();
                    if p.is_empty() {
                        continue;
                    }
                    v.push(p.parse::<i64>().context("component template id")?);
                }
                if v.is_empty() {
                    anyhow::bail!("--allow-components produced an empty list");
                }
                v
            } else {
                anyhow::bail!("pass --all-component-templates or --allow-components id,id,…");
            };

            let inp = OptimizeInput {
                level: level_val,
                capital_budget: *capital,
                target_totals,
                weights: w,
                allowed_templates: allowed,
                data,
                restarts: *restarts,
                hill_steps: *hill_steps,
                seed: seed.unwrap_or(0xC0FFEE),
            };
            let res = optimize(inp);
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                    "score": res.score,
                    "capital_used": res.capital_used,
                    "slots": res.slots,
                    "invested": res.invested,
                    "predicted_totals": res.predicted_totals,
                }))?);
            } else {
                println!("Weighted MSE score: {:.4} (lower is better)", res.score);
                println!("Capital used (estimated): {}", res.capital_used);
                println!("\nComponent slots (item template id per slot, empty = null):");
                for (i, s) in res.slots.iter().enumerate() {
                    println!("  [{}] {:?}", i, s);
                }
                println!("\nPoint-buy (invested) after optimization:");
                let mut ks: Vec<_> = res.invested.keys().cloned().collect();
                ks.sort();
                for k in ks {
                    let v = res.invested[&k];
                    if v != 0 {
                        println!("  {}: {}", k, v);
                    }
                }
                println!("\nPredicted totals:");
                let mut ks: Vec<_> = res.predicted_totals.keys().cloned().collect();
                ks.sort();
                for k in ks {
                    println!("  {}: {}", k, res.predicted_totals[&k]);
                }
            }
        }
        BuildCmd::Apply {
            leek,
            target,
            backup_dir,
            dry_run,
            yes,
        } => {
            if !dry_run && !yes {
                anyhow::bail!(
                    "refusing to change your leek without --yes (use --dry-run to validate stash vs target only)"
                );
            }
            let (login, password) = crate::session::auth(cli)?;
            let mut c = crate::session::client()?;
            c.farmer_login(&login, &password, false).await?;
            let report = apply::apply_mirror_loadout(
                &c,
                *leek,
                *target,
                backup_dir.clone(),
                *dry_run,
            )
            .await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "backup_path": report.backup_path.as_ref().map(|p| p.display().to_string()),
                        "dry_run": report.dry_run,
                        "weapons_placed": report.weapons_placed,
                        "chips_placed": report.chips_placed,
                        "hat_set": report.hat_set,
                        "components_placed": report.components_placed,
                    }))?
                );
            } else {
                let st = Styles::new(cli.color.use_color(&stdout()));
                if report.dry_run {
                    eprintln!(
                        "{g}Dry-run OK:{r} your stash (inventory + what this leek is wearing) can supply the target loadout.",
                        g = st.green,
                        r = st.reset
                    );
                } else {
                    if let Some(ref p) = report.backup_path {
                        eprintln!(
                            "{g}Backed up previous loadout to{r} {p}",
                            g = st.green,
                            r = st.reset,
                            p = p.display()
                        );
                    }
                    eprintln!(
                        "{b}Applied{r} — weapons: {w}  chips: {c}  hat: {h}  components: {co}",
                        b = st.bold,
                        r = st.reset,
                        w = report.weapons_placed,
                        c = report.chips_placed,
                        h = if report.hat_set { "yes" } else { "no" },
                        co = report.components_placed
                    );
                }
                if backup_dir.is_none() && !report.dry_run {
                    eprintln!(
                        "{d}(default backup dir: {}){r}",
                        apply::default_apply_backup_dir().display(),
                        d = st.dim,
                        r = st.reset
                    );
                }
            }
        }
    }
    Ok(())
}

fn acquisition_style(styles: &Styles, k: AcquisitionKind) -> &'static str {
    match k {
        AcquisitionKind::Stock => styles.green,
        AcquisitionKind::Craftable => styles.yellow,
        AcquisitionKind::Market => styles.blue,
        AcquisitionKind::Other => styles.magenta,
    }
}

fn acquisition_word(k: AcquisitionKind) -> &'static str {
    match k {
        AcquisitionKind::Stock => "in stock",
        AcquisitionKind::Craftable => "craftable",
        AcquisitionKind::Market => "market",
        AcquisitionKind::Other => "other/special",
    }
}

fn fmt_have(h: Option<i64>) -> String {
    h.map(|n| n.to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn print_mirror_equipment_section(styles: &Styles, title: &str, rows: &[EquipmentRow]) {
    let s = styles;
    println!("\n{b}{title}{b0}", b = s.bold, b0 = s.reset);
    if rows.is_empty() {
        println!("  {d}(none equipped){d0}", d = s.dim, d0 = s.reset);
        return;
    }
    print_mirror_equipment_rows(styles, rows);
}

fn print_mirror_equipment_rows(styles: &Styles, rows: &[EquipmentRow]) {
    let s = styles;
    for r in rows {
        let sc = acquisition_style(s, r.acquisition);
        let word = acquisition_word(r.acquisition);
        println!(
            "  {d}{tid:>5}{d0}  {name:<38}  need {need}  have {have}  miss {miss}  {sc}{word}{z}",
            d = s.dim,
            d0 = s.reset,
            tid = r.template,
            name = r.name,
            need = r.need,
            have = fmt_have(r.have),
            miss = r.missing,
            sc = sc,
            word = word,
            z = s.reset,
        );
    }
}

fn print_mirror_component_row(styles: &Styles, r: &ComponentDiffRow) {
    let s = styles;
    let sc = acquisition_style(s, r.acquisition);
    let word = acquisition_word(r.acquisition);
    println!(
        "  {d}{tid:>5}{d0}  {name:<38}  need x{need}  have {have}  miss {miss}  {sc}{word}{z}",
        d = s.dim,
        d0 = s.reset,
        tid = r.template,
        name = r.name,
        need = r.need,
        have = fmt_have(r.have),
        miss = r.missing,
        sc = sc,
        word = word,
        z = s.reset,
    );
}
