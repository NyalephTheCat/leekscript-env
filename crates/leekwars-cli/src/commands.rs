//! Command dispatch: maps [`crate::cli::Cli`] to API calls and output.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow};
use leekscript::{LanguageOptions, prepare_merged_check_unit};
use leekwars_api::AiExportOptions;
use serde_json::Value;

use crate::batch::{config as batch_config, run as batch_run, stats as batch_stats};
use crate::build::cli as build_cli;
use crate::cli::{BatchCmd, Cli, Command, EncyclopediaCmd, EquipmentAction, FightAction};
use crate::config;
use crate::output::{
    print_farmer_inventory, print_garden_summary, print_leek_equipment, print_leek_summary,
    print_opponents_table,
};
use crate::session;

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    match &cli.command {
        Command::Login => {
            let (login, password) = session::auth(&cli)?;
            let mut c = session::client()?;
            c.farmer_login(&login, &password, false).await?;
            let v = c.farmer_get_from_token().await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if let Some(f) = v.get("farmer") {
                println!(
                    "Logged in as {} (id {})",
                    f["name"].as_str().unwrap_or("?"),
                    f["id"].as_i64().unwrap_or(0)
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&v)?);
            }
        }
        Command::Profiles => {
            let (path, cfg) = config::load_resolved_config(cli.config.as_deref())?;
            if !cli.json {
                println!("# {}", path.display());
                if let Some(d) = &cfg.default_profile {
                    println!("# default_profile = \"{d}\"");
                }
            }
            let mut names: Vec<_> = cfg.accounts.keys().cloned().collect();
            names.sort();
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "path": path.display().to_string(),
                        "default_profile": cfg.default_profile,
                        "profiles": names,
                    }))?
                );
            } else {
                for n in names {
                    println!("{n}");
                }
            }
        }
        Command::DataVersion => {
            let c = session::client()?;
            let v = c.data_version().await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "master_version": v.master_version,
                    }))?
                );
            } else {
                println!("{}", v.master_version);
            }
        }
        Command::Farmer { farmer_id } => {
            let c = session::client()?;
            let v = c.farmer_get(*farmer_id).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("?");
                let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(*farmer_id);
                println!("Farmer {name} (id {id})");
                if let Some(talent) = v.get("talent").and_then(|x| x.as_i64()) {
                    println!("talent {talent}");
                }
                eprintln!("(use --json for the full JSON payload)");
            }
        }
        Command::AiExport { dir } => {
            let (login, password) = session::auth(&cli)?;
            let mut c = session::client()?;
            c.farmer_login(&login, &password, false).await?;
            let session = c.farmer_get_from_token().await?;
            let farmer = session
                .get("farmer")
                .ok_or_else(|| anyhow!("session has no farmer"))?;
            eprintln!("Exporting AIs to {} …", dir.display());
            let report = c
                .export_farmer_ais_to_directory(farmer, dir.as_path(), AiExportOptions::default())
                .await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "written": report.written, "paths": report.paths, "failures": report.failures })
                );
            } else {
                for p in &report.paths {
                    println!("{}", p.display());
                }
                eprintln!("Wrote {} file(s).", report.written);
                for (id, msg) in &report.failures {
                    eprintln!("  id {id}: {msg}");
                }
            }
            if !report.failures.is_empty() {
                anyhow::bail!("{} export failure(s)", report.failures.len());
            }
        }
        Command::AiDownload { ai_id, out } => {
            let (login, password) = session::auth(&cli)?;
            let mut c = session::client()?;
            c.farmer_login(&login, &password, false).await?;
            let bytes = c.ai_download(*ai_id).await?;
            match out {
                Some(path) => {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(path, &bytes)?;
                    eprintln!("Wrote {} bytes to {}", bytes.len(), path.display());
                }
                None => {
                    print!("{}", String::from_utf8_lossy(&bytes));
                }
            }
        }
        Command::AiUpload {
            ai_id,
            path,
            merge,
            merge_root,
        } => {
            let (login, password) = session::auth(&cli)?;
            let mut c = session::client()?;
            c.farmer_login(&login, &password, false).await?;
            let code = if *merge {
                let root = merge_root.clone().unwrap_or_else(|| {
                    path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .to_path_buf()
                });
                let prep = prepare_merged_check_unit(
                    &root,
                    path.as_path(),
                    LanguageOptions::default(),
                    &[],
                    None,
                )
                .map_err(|e| anyhow!("merge: {e}"))?;
                prep.combined
            } else {
                std::fs::read_to_string(path).with_context(|| path.display().to_string())?
            };
            let v = c.ai_save(*ai_id, &code).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                eprintln!("Saved AI {ai_id} ({} bytes).", code.len());
            }
        }
        Command::Garden { leek } => {
            let (login, password) = session::auth(&cli)?;
            let mut c = session::client()?;
            c.farmer_login(&login, &password, false).await?;
            let garden = c.garden_get().await?;
            let opponents = if let Some(lid) = leek {
                c.garden_get_leek_opponents(*lid).await?
            } else {
                c.garden_get_farmer_opponents().await?
            };
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "garden": garden,
                        "opponents": opponents,
                    }))?
                );
            } else {
                print_garden_summary(&garden, leek.is_some())?;
                print_opponents_table(&opponents)?;
            }
        }
        Command::Fight { action } => match action {
            FightAction::Get { fight_id, logs } => {
                let c = session::client()?;
                let v = if *logs {
                    c.fight_get_logs(*fight_id).await?
                } else {
                    c.fight_get(*fight_id).await?
                };
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&v)?);
                } else {
                    println!("{}", serde_json::to_string_pretty(&v)?);
                }
            }
            FightAction::Solo { leek_id, target_id } => {
                let (login, password) = session::auth(&cli)?;
                let mut c = session::client()?;
                c.farmer_login(&login, &password, false).await?;
                let v = c.garden_start_solo_fight(*leek_id, *target_id).await?;
                print_garden_fight_start_response(&cli, &v)?;
            }
            FightAction::Farmer { target_id } => {
                let (login, password) = session::auth(&cli)?;
                let mut c = session::client()?;
                c.farmer_login(&login, &password, false).await?;
                let v = c.garden_start_farmer_fight(*target_id).await?;
                print_garden_fight_start_response(&cli, &v)?;
            }
        },
        Command::Encyclopedia { cmd } => match cmd {
            EncyclopediaCmd::Fetch { locale, out } => {
                let c = session::client()?;
                let v = c.encyclopedia_get_all_locale(locale).await?;
                let text = serde_json::to_string_pretty(&v)?;
                if let Some(path) = out {
                    std::fs::write(path, &text)?;
                    eprintln!("Wrote {} to {}", locale, path.display());
                } else {
                    print!("{text}");
                }
            }
            EncyclopediaCmd::Search { locale, query } => {
                let c = session::client()?;
                let v = c.encyclopedia_get_all_locale(locale).await?;
                let q = query.as_str().to_lowercase();
                let mut hits: Vec<(String, String, i64)> = Vec::new();
                if let Value::Object(map) = &v {
                    for (slug, meta) in map {
                        let title = meta.get("title").and_then(|t| t.as_str()).unwrap_or("");
                        let id = meta.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
                        if slug.to_lowercase().contains(&q) || title.to_lowercase().contains(&q) {
                            hits.push((slug.clone(), title.to_string(), id));
                        }
                    }
                }
                hits.sort_by(|a, b| a.1.cmp(&b.1));
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&hits)?);
                } else {
                    for (slug, title, id) in &hits {
                        println!("{id:5}  {title}  ({slug})");
                    }
                    eprintln!("{} match(es).", hits.len());
                }
            }
            EncyclopediaCmd::Page { locale, slug } => {
                let c = session::client()?;
                let v = c.encyclopedia_get_page(locale, slug.as_str()).await?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&v)?);
                } else if let Some(content) = v.get("content").and_then(|x| x.as_str()) {
                    let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("?");
                    println!("# {title}\n\n{content}");
                } else {
                    println!("{}", serde_json::to_string_pretty(&v)?);
                }
            }
        },
        Command::Leek {
            leek_id,
            equipment_only,
        } => {
            let c = session::client()?;
            let v = c.leek_get(*leek_id).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if *equipment_only {
                print_leek_equipment(&v)?;
            } else {
                print_leek_summary(&v);
                print_leek_equipment(&v)?;
            }
        }
        Command::Inventory => {
            let (login, password) = session::auth(&cli)?;
            let mut c = session::client()?;
            c.farmer_login(&login, &password, false).await?;
            let v = c.farmer_get_from_token().await?;
            let farmer = v
                .get("farmer")
                .ok_or_else(|| anyhow!("session has no farmer"))?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(farmer)?);
            } else {
                print_farmer_inventory(farmer)?;
            }
        }
        Command::Equipment { action } => {
            let (login, password) = session::auth(&cli)?;
            let mut c = session::client()?;
            c.farmer_login(&login, &password, false).await?;
            let v = match action {
                EquipmentAction::AddWeapon { leek_id, weapon_id } => {
                    c.leek_add_weapon(*leek_id, *weapon_id).await?
                }
                EquipmentAction::RemoveWeapon { weapon_id } => {
                    c.leek_remove_weapon(*weapon_id).await?
                }
                EquipmentAction::AddChip { leek_id, chip_id } => {
                    c.leek_add_chip(*leek_id, *chip_id).await?
                }
                EquipmentAction::RemoveChip { chip_id } => c.leek_remove_chip(*chip_id).await?,
                EquipmentAction::SetHat {
                    leek_id,
                    hat_template_id,
                } => c.leek_set_hat(*leek_id, *hat_template_id).await?,
                EquipmentAction::RemoveHat { leek_id } => c.leek_remove_hat(*leek_id).await?,
            };
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Command::Build { cmd } => {
            build_cli::run_build(cmd, &cli).await?;
        }
        Command::Batch { cmd } => match cmd {
            BatchCmd::Run {
                config,
                dry_run,
                verbose,
                quiet,
                no_progress,
                max_fights,
            } => {
                let mut plan = batch_config::BatchFile::load(config.as_path())?;
                if let Some(n) = max_fights {
                    plan.max_fights = Some(*n);
                }
                if *dry_run {
                    batch_run::print_dry_run(&plan, config.as_path(), cli.json, cli.color)?;
                    return Ok(());
                }
                let mut stats = batch_stats::BatchStats::load(&plan.stats_path)?;
                let opts =
                    batch_run::BatchRunOptions::from_cli(&cli, *verbose, *quiet, *no_progress);
                let summary = batch_run::run_batch(&plan, &cli, &mut stats, opts).await?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&summary)?);
                }
                if summary.interrupted {
                    std::process::exit(130);
                }
            }
            BatchCmd::Stats { stats } => {
                let path = batch_stats_path(stats.as_ref());
                let s = batch_stats::BatchStats::load(&path)?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&s)?);
                } else {
                    batch_stats::print_table(&s, &path);
                }
            }
            BatchCmd::Reset { stats, yes } => {
                if !yes {
                    anyhow::bail!("refusing to clear stats without --yes");
                }
                let path = batch_stats_path(stats.as_ref());
                let mut s = batch_stats::BatchStats::load(&path)?;
                s.clear();
                s.save(&path)?;
                eprintln!("Cleared opponent stats in {}", path.display());
            }
        },
    }
    Ok(())
}

fn batch_stats_path(stats: Option<&PathBuf>) -> PathBuf {
    stats
        .cloned()
        .or_else(|| {
            std::env::var("LEEKWARS_BATCH_STATS")
                .ok()
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from("leekwars-batch-stats.json"))
}

fn print_garden_fight_start_response(cli: &Cli, v: &Value) -> anyhow::Result<()> {
    if cli.json {
        println!("{}", serde_json::to_string_pretty(v)?);
    } else {
        println!("{}", serde_json::to_string_pretty(v)?);
        if let Some(fid) = v
            .get("fight_id")
            .and_then(|x| x.as_i64())
            .or_else(|| v.get("fight").and_then(|x| x.as_i64()))
        {
            eprintln!("Fight id: {fid} (replay: https://leekwars.com/fight/{fid})");
        }
    }
    Ok(())
}
