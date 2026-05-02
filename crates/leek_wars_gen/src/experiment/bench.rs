//! API-backed PvP batches: composition vs composition, composition vs leek, multi-seed win rates.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use console::Style;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::{json, Value};
use ureq::Agent;

use crate::error::GenError;
use crate::experiment::metrics::RunMetrics;
use crate::experiment::planner::RunTask;
use crate::experiment::execute_run_task;

use lw_meta::{
    fetch_composition_sim_bundle, fetch_leek_public, scenario_entity_from_leek_get, RetryPolicy,
};

/// `composition:123` / `compo:123` / `leek:456` (prefixes case-insensitive).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BenchSide {
    Composition(u64),
    Leek(u64),
}

impl FromStr for BenchSide {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let pos = s.find(':').ok_or_else(|| {
            String::from(
                "expected SIDE like composition:ID or leek:ID (example: composition:42, leek:99)",
            )
        })?;
        let (kind, id_s) = s.split_at(pos);
        let id_s = id_s.trim_start_matches(':').trim();
        let id: u64 = id_s
            .parse()
            .map_err(|_| format!("invalid id in {:?} (expected positive integer)", id_s))?;
        match kind.trim().to_ascii_lowercase().as_str() {
            "composition" | "compo" | "c" | "team" => Ok(BenchSide::Composition(id)),
            "leek" | "l" => Ok(BenchSide::Leek(id)),
            other => Err(format!(
                "unknown side kind {:?} (use composition: or leek:)",
                other
            )),
        }
    }
}

impl std::fmt::Display for BenchSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenchSide::Composition(id) => write!(f, "composition:{id}"),
            BenchSide::Leek(id) => write!(f, "leek:{id}"),
        }
    }
}

/// Inner `farmer` object from `farmer/get` (`{ "farmer": { ... } }` or already the inner object).
pub fn farmer_inner<'a>(body: &'a Value) -> &'a Value {
    body.get("farmer").unwrap_or(body)
}

/// `(composition_id, name)` when the API includes `team.compositions` (often requires session token).
pub fn list_team_compositions(farmer: &Value) -> Vec<(u64, String)> {
    let Some(team) = farmer.get("team").filter(|t| !t.is_null()) else {
        return Vec::new();
    };
    let Some(arr) = team.get("compositions").and_then(|c| c.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for c in arr {
        let Some(id) = c.get("id").and_then(|x| x.as_u64()) else {
            continue;
        };
        let name = c
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string();
        out.push((id, name));
    }
    out
}

/// Leeks listed on the farmer profile (`farmer.leeks` map) for `leek:ID` test targets.
pub fn list_farmer_leeks(farmer: &Value) -> Vec<(u64, String)> {
    let Some(map) = farmer.get("leeks").and_then(|x| x.as_object()) else {
        return Vec::new();
    };
    let mut out: Vec<(u64, String)> = map
        .values()
        .filter_map(|v| {
            let id = v.get("id").and_then(|x| x.as_u64())?;
            let name = v
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("?")
                .to_string();
            Some((id, name))
        })
        .collect();
    out.sort_by_key(|x| x.0);
    out
}

fn entities_row_from_leek_raw(raw: &Value) -> Result<Vec<Value>, GenError> {
    Ok(vec![scenario_entity_from_leek_get(raw)])
}

fn entities_row_from_composition_bundle(bundle: &Value) -> Result<Vec<Value>, GenError> {
    let leeks = bundle
        .get("leeks")
        .and_then(|x| x.as_array())
        .ok_or_else(|| GenError::Message("composition bundle missing leeks[]".into()))?;
    if leeks.is_empty() {
        return Err(GenError::Message("composition has no leeks".into()));
    }
    let mut row = Vec::with_capacity(leeks.len());
    for entry in leeks {
        let sheet = entry
            .get("sheet")
            .ok_or_else(|| GenError::Message("composition leek entry missing sheet".into()))?;
        let raw = sheet.get("raw").ok_or_else(|| {
            GenError::Message(
                "leek sheet missing raw (re-fetch with full export, not profile_only)".into(),
            )
        })?;
        row.push(scenario_entity_from_leek_get(raw));
    }
    Ok(row)
}

/// Fetch API data for one side and produce entity row + display label.
pub fn fetch_side_row(
    agent: &Agent,
    api_base: &str,
    side: &BenchSide,
    retry: &RetryPolicy,
    gap: Duration,
) -> Result<(Vec<Value>, String), GenError> {
    match side {
        BenchSide::Composition(id) => {
            let bundle = fetch_composition_sim_bundle(
                agent,
                api_base,
                *id,
                retry,
                true,
                false,
                gap,
            )?;
            let label = bundle
                .get("summary")
                .and_then(|s| s.get("name"))
                .and_then(|x| x.as_str())
                .map(|s| format!("{} [{id}]", s))
                .unwrap_or_else(|| format!("composition {id}"));
            let row = entities_row_from_composition_bundle(&bundle)?;
            Ok((row, label))
        }
        BenchSide::Leek(id) => {
            let raw = fetch_leek_public(agent, api_base, *id, retry)?;
            let label = raw
                .get("name")
                .and_then(|x| x.as_str())
                .map(|s| format!("{} [{id}]", s))
                .unwrap_or_else(|| format!("leek {id}"));
            let row = entities_row_from_leek_raw(&raw)?;
            Ok((row, label))
        }
    }
}

/// Build scenario JSON: `entities[0]` vs `entities[1]`, team indices 0 and 1 in the fight engine.
pub fn build_pvp_scenario_value(
    row0: Vec<Value>,
    row1: Vec<Value>,
    team0_label: &str,
    team1_label: &str,
    random_seed: i32,
) -> Result<Value, GenError> {
    if row0.is_empty() || row1.is_empty() {
        return Err(GenError::Message("both teams need at least one leek".into()));
    }

    let fid0 = row0[0]
        .get("farmer")
        .and_then(|x| x.as_i64())
        .unwrap_or(1) as i32;
    let fid1 = row1[0]
        .get("farmer")
        .and_then(|x| x.as_i64())
        .unwrap_or(2) as i32;
    let fid1 = if fid1 == fid0 { fid0.saturating_add(1).max(1) } else { fid1 };

    let mut r0 = row0;
    let mut r1 = row1;
    for e in &mut r0 {
        if let Some(o) = e.as_object_mut() {
            o.insert("team".into(), json!(1));
            o.insert("farmer".into(), json!(fid0));
            o.insert("cell".into(), Value::Null);
        }
    }
    for e in &mut r1 {
        if let Some(o) = e.as_object_mut() {
            o.insert("team".into(), json!(2));
            o.insert("farmer".into(), json!(fid1));
            o.insert("cell".into(), Value::Null);
        }
    }

    Ok(json!({
        "max_turns": 64,
        "random_seed": random_seed,
        "draw_check_life": true,
        "farmers": [
            { "id": fid0, "name": team0_label, "country": "?" },
            { "id": fid1, "name": team1_label, "country": "?" }
        ],
        "teams": [
            { "id": 1, "name": team0_label },
            { "id": 2, "name": team1_label }
        ],
        "entities": [ r0, r1 ]
    }))
}

/// Force one team row (`entities[team_idx]`) to use this AI path.
pub fn apply_team_ai_override(
    scenario: &mut Value,
    team_idx: usize,
    ai_path: &str,
) -> Result<(), GenError> {
    let entities = scenario
        .get_mut("entities")
        .and_then(|e| e.as_array_mut())
        .ok_or_else(|| GenError::Message("scenario.entities must be array".into()))?;
    let Some(team) = entities.get_mut(team_idx).and_then(|t| t.as_array_mut()) else {
        return Err(GenError::Message(format!("scenario.entities[{team_idx}] missing")));
    };
    for ent in team {
        if let Some(o) = ent.as_object_mut() {
            o.insert("ai".into(), json!(ai_path));
        }
    }
    Ok(())
}

/// Force every team-0 entity to use this AI path (e.g. local `ai/v2/My.leek` under `--ai-root`).
pub fn apply_team0_ai_override(scenario: &mut Value, ai_path: &str) -> Result<(), GenError> {
    apply_team_ai_override(scenario, 0, ai_path)
}

/// Force every team-1 entity to use this AI path.
pub fn apply_team1_ai_override(scenario: &mut Value, ai_path: &str) -> Result<(), GenError> {
    apply_team_ai_override(scenario, 1, ai_path)
}

/// Force every entity on every team to use this AI path (mirror / same-script tests).
pub fn apply_all_ai_override(scenario: &mut Value, ai_path: &str) -> Result<(), GenError> {
    let entities = scenario
        .get_mut("entities")
        .and_then(|e| e.as_array_mut())
        .ok_or_else(|| GenError::Message("scenario.entities must be array".into()))?;
    for team in entities.iter_mut() {
        let Some(team_arr) = team.as_array_mut() else {
            continue;
        };
        for ent in team_arr {
            if let Some(o) = ent.as_object_mut() {
                o.insert("ai".into(), json!(ai_path));
            }
        }
    }
    Ok(())
}

/// One completed fight in a batch.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BenchFightRecord {
    pub arm: String,
    pub seed: i32,
    pub ok: bool,
    pub winner: Option<i64>,
    pub duration: Option<i64>,
    pub error: Option<String>,
}

pub struct PvpBenchParams<'a> {
    pub generator_root: &'a Path,
    /// Parent directory of `leekwars-ai/` when scripts are outside `generator_root`.
    pub ai_scripts_root: Option<&'a Path>,
    pub jobs: usize,
    pub show_progress: bool,
}

/// Run many fights; `scenarios` entries are `(arm_label, scenario_json)`.
pub fn run_pvp_batch(
    params: &PvpBenchParams<'_>,
    scenarios: Vec<(String, Value)>,
) -> Result<Vec<BenchFightRecord>, GenError> {
    let arms: Vec<String> = scenarios.iter().map(|(a, _)| a.clone()).collect();
    let mut tasks: Vec<RunTask> = Vec::with_capacity(scenarios.len());
    for (i, (arm, scenario_value)) in scenarios.into_iter().enumerate() {
        tasks.push(RunTask {
            run_id: i,
            arm_name: arm,
            seed: 0,
            tunables: HashMap::new(),
            scenario_value,
            overlay_sources: HashMap::new(),
        });
    }

    let n = tasks.len();
    if n == 0 {
        return Ok(Vec::new());
    }

    let jobs = params.jobs.max(1);
    let pb = if params.show_progress {
        let p = ProgressBar::new(n as u64);
        p.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} fights",
            )
            .unwrap()
            .progress_chars("#>-"),
        );
        Some(p)
    } else {
        None
    };

    let (tx, rx) = channel::<Result<(usize, RunMetrics), GenError>>();
    let root = params.generator_root.to_path_buf();
    thread::scope(|s| {
        let chunk_size = (n + jobs - 1) / jobs;
        for chunk in tasks.chunks(chunk_size.max(1)) {
            let tx = tx.clone();
            let chunk: Vec<RunTask> = chunk.to_vec();
            let root = root.clone();
            s.spawn(move || {
                for task in chunk {
                    let id = task.run_id;
                    let arm = task.arm_name.clone();
                    let res = execute_run_task(
                        &task,
                        root.as_path(),
                        None,
                        params.ai_scripts_root,
                    )
                    .map(|o| (id, o.metrics));
                    if tx.send(res.map_err(|e| GenError::Message(format!("{arm}: {e}")))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
    });

    let mut metrics_by_id: Vec<Option<RunMetrics>> = vec![None; n];
    for r in rx {
        let (id, m) = r?;
        metrics_by_id[id] = Some(m);
        if let Some(ref p) = pb {
            p.inc(1);
        }
    }
    if let Some(p) = pb {
        p.finish_with_message("done");
    }

    let mut records = Vec::with_capacity(n);
    for (i, slot) in metrics_by_id.into_iter().enumerate() {
        let arm = arms
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("run_{i}"));
        let m = match slot {
            Some(m) => m,
            None => {
                records.push(BenchFightRecord {
                    arm,
                    seed: 0,
                    ok: false,
                    winner: None,
                    duration: None,
                    error: Some("missing worker result".into()),
                });
                continue;
            }
        };
        let ok = m.error.is_none();
        records.push(BenchFightRecord {
            arm,
            seed: 0,
            ok,
            winner: m.winner,
            duration: m.duration,
            error: m.error.clone(),
        });
    }

    Ok(records)
}

/// Print colored win-rate summary for team 0 (first row in `entities`). Groups by arm prefix before `| seed=`.
pub fn print_pvp_summary(records: &[BenchFightRecord], team0_name: &str, team1_name: &str) {
    let title = Style::new().cyan().bold();
    let win = Style::new().green();
    let loss = Style::new().red();
    let draw = Style::new().yellow();
    eprintln!();
    eprintln!(
        "{}",
        title.apply_to(format!("── PvP batch: {} (team 0) vs {} (team 1) ──", team0_name, team1_name))
    );

    let mut by_arm: HashMap<String, (u64, u64, u64)> = HashMap::new();
    for r in records {
        let key = r
            .arm
            .split("| seed=")
            .next()
            .unwrap_or(&r.arm)
            .trim()
            .to_string();
        let e = by_arm.entry(key).or_insert((0, 0, 0));
        if !r.ok {
            continue;
        }
        match r.winner {
            Some(0) => e.0 += 1,
            Some(1) => e.1 += 1,
            _ => e.2 += 1,
        }
    }

    let mut keys: Vec<_> = by_arm.keys().cloned().collect();
    keys.sort();
    for k in keys {
        let (w0, w1, dr) = by_arm[&k];
        let dec = w0 + w1 + dr;
        let rate = if dec == 0 {
            0.0
        } else {
            (w0 as f64 / dec as f64) * 100.0
        };
        let rate_s = format!("{rate:.1}%");
        let rate_styled = if rate >= 55.0 {
            win.apply_to(rate_s).to_string()
        } else if rate <= 45.0 {
            loss.apply_to(rate_s).to_string()
        } else {
            draw.apply_to(rate_s).to_string()
        };
        eprintln!(
            "  {:<40}  {}  (n={}  W/L/D={}/{}/{})",
            k, rate_styled, dec, w0, w1, dr
        );
    }
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn farmer_inner_and_leeks() {
        let body = json!({
            "farmer": {
                "name": "F",
                "leeks": {
                    "1": {"id": 10, "name": "A"},
                    "2": {"id": 2, "name": "B"}
                },
                "team": {
                    "compositions": [
                        {"id": 100, "name": "Solo"},
                        {"id": 200, "name": "Duo"}
                    ]
                }
            }
        });
        let fin = farmer_inner(&body);
        assert_eq!(fin["name"], json!("F"));
        let comps = list_team_compositions(fin);
        assert_eq!(comps.len(), 2);
        let leeks = list_farmer_leeks(fin);
        assert_eq!(leeks.len(), 2);
        assert_eq!(leeks[0].0, 2);
        assert_eq!(leeks[1].0, 10);
    }
}