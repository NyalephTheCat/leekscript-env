//! Black-box optimization helpers for **scenario entities** (stats, and by extension loadouts you patch in JSON).
//!
//! ## How this relates to “GD + fuzzing”
//! - The Rust fight loop is a **discrete, stochastic black box**: there is no smooth loss surface for true gradient descent on win/loss.
//! - **Finite-difference / coordinate search** (implemented here) is the practical analogue: probe ± steps per stat, keep improvements — same spirit as zeroth-order optimization.
//! - **Fuzzing** matches **discrete** choices (weapon/chip ids, AI const grids): sample neighbors, hill-climb or use existing [`crate::fuzz`] / experiment grids.
//! - For real “gradients”, you’d need a differentiable surrogate (ML) or many samples to estimate ∂P(win)/∂stat — out of scope here.
//!
//! Typical loop: build a base [`serde_json::Value`] scenario (e.g. from a [`super::meta`] snapshot), fix the opponent, run
//! [`super::batch::execute_run_task`] inside `eval`, and maximize [`fitness_team_win`] for your team index.

use std::collections::HashMap;

use crate::error::GenError;
use rand::Rng;
use serde_json::{json, Value};

use super::metrics::RunMetrics;

/// Core stats on [`crate::scenario::EntityInfo`] that are safe to perturb in JSON scenarios.
///
/// Includes API `total_*` fields so tuning matches equipped stats when scenarios embed `leek/get`-style data.
pub const STAT_FIELDS: &[&str] = &[
    "life",
    "total_life",
    "tp",
    "mp",
    "total_tp",
    "total_mp",
    "strength",
    "total_strength",
    "agility",
    "total_agility",
    "wisdom",
    "total_wisdom",
    "resistance",
    "total_resistance",
    "science",
    "total_science",
    "magic",
    "total_magic",
    "frequency",
    "total_frequency",
    "cores",
    "total_cores",
    "ram",
    "total_ram",
];

/// `winner` in outcome JSON is the winning **team index** (same convention as the Rust engine).
#[must_use]
pub fn fitness_team_win(metrics: &RunMetrics, my_team_index: i64) -> f64 {
    if metrics.error.is_some() {
        return f64::NEG_INFINITY;
    }
    match metrics.winner {
        Some(w) if w == my_team_index => 1.0,
        Some(_) => 0.0,
        None => 0.5,
    }
}

/// Same as [`fitness_team_win`], with a small bonus for shorter fights when you win (tie-break only).
#[must_use]
pub fn fitness_team_win_fast(
    metrics: &RunMetrics,
    my_team_index: i64,
    duration_weight: f64,
) -> f64 {
    let base = fitness_team_win(metrics, my_team_index);
    if base < 1.0 {
        return base;
    }
    let d = metrics.duration.unwrap_or(0).max(1) as f64;
    base + duration_weight / d
}

fn entity_value_mut(
    scenario: &mut Value,
    team: usize,
    slot: usize,
) -> Result<&mut Value, GenError> {
    let teams = scenario
        .get_mut("entities")
        .and_then(|e| e.as_array_mut())
        .ok_or_else(|| GenError::Message("scenario.entities must be an array".into()))?;
    let row = teams
        .get_mut(team)
        .and_then(|t| t.as_array_mut())
        .ok_or_else(|| GenError::Message(format!("entities[{team}] must be an array")))?;
    row.get_mut(slot)
        .ok_or_else(|| GenError::Message(format!("entities[{team}][{slot}] missing")))
}

/// Add `delta` to integer stat `field` on one entity (clamped so result ≥ `min_value`).
pub fn add_entity_stat(
    scenario: &mut Value,
    team: usize,
    slot: usize,
    field: &str,
    delta: i64,
    min_value: i64,
) -> Result<(), GenError> {
    let ent = entity_value_mut(scenario, team, slot)?;
    let obj = ent
        .as_object_mut()
        .ok_or_else(|| GenError::Message("entity must be an object".into()))?;
    let cur = obj
        .get(field)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let next = (cur + delta).max(min_value);
    obj.insert(field.to_string(), json!(next));
    Ok(())
}

/// Replace `weapons` / `chips` arrays (template ids) on an entity.
pub fn set_entity_loadout(
    scenario: &mut Value,
    team: usize,
    slot: usize,
    weapons: &[i32],
    chips: &[i32],
) -> Result<(), GenError> {
    let ent = entity_value_mut(scenario, team, slot)?;
    let obj = ent
        .as_object_mut()
        .ok_or_else(|| GenError::Message("entity must be an object".into()))?;
    obj.insert("weapons".to_string(), json!(weapons));
    obj.insert("chips".to_string(), json!(chips));
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CoordinateSearchConfig {
    /// Max outer rounds (each round scans all dimensions twice: +step and −step).
    pub max_rounds: usize,
    /// Absolute delta applied to each integer stat when probing.
    pub step: i64,
    /// Minimum value when clamping (usually `1` for life, `0` for the rest).
    pub min_stat: i64,
}

impl Default for CoordinateSearchConfig {
    fn default() -> Self {
        Self {
            max_rounds: 8,
            step: 5,
            min_stat: 0,
        }
    }
}

/// Zeroth-order coordinate search: for each stat field, try +step and −step; keep the scenario with best `eval` score.
///
/// `eval` should clone internally if it needs to persist the scenario; this function passes owned `Value` each time.
pub fn coordinate_search_stats<F>(
    scenario: Value,
    team: usize,
    slot: usize,
    mut eval: F,
    cfg: &CoordinateSearchConfig,
) -> Result<(Value, f64), GenError>
where
    F: FnMut(Value) -> Result<f64, GenError>,
{
    let min_life = 1_i64;
    let mut best = scenario;
    let mut best_score = eval(best.clone())?;

    for _ in 0..cfg.max_rounds {
        let mut improved = false;
        for &field in STAT_FIELDS {
            let min_v = if field == "life" || field == "total_life" {
                min_life
            } else {
                cfg.min_stat
            };
            for sign in [-1_i64, 1_i64] {
                let mut trial = best.clone();
                add_entity_stat(&mut trial, team, slot, field, sign * cfg.step, min_v)?;
                let score = eval(trial.clone())?;
                if score > best_score + 1e-9 {
                    best_score = score;
                    best = trial;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
    Ok((best, best_score))
}

#[derive(Debug, Clone)]
pub struct HillClimbConfig {
    pub max_iters: usize,
    /// Per-iteration: try this many random single-stat nudges.
    pub proposals_per_iter: usize,
    pub step_mag: i64,
    pub min_stat: i64,
}

impl Default for HillClimbConfig {
    fn default() -> Self {
        Self {
            max_iters: 50,
            proposals_per_iter: 12,
            step_mag: 8,
            min_stat: 0,
        }
    }
}

/// Random single-stat proposals (fuzz-style); accept first improvement each iteration, then resample around new best.
pub fn hill_climb_stats<R, F>(
    scenario: Value,
    team: usize,
    slot: usize,
    rng: &mut R,
    mut eval: F,
    cfg: &HillClimbConfig,
) -> Result<(Value, f64), GenError>
where
    R: Rng,
    F: FnMut(Value) -> Result<f64, GenError>,
{
    let min_life = 1_i64;
    let mut best_score = eval(scenario.clone())?;
    let mut best = scenario;

    let fields: Vec<&str> = STAT_FIELDS.to_vec();
    for _ in 0..cfg.max_iters {
        let mut improved = false;
        for _ in 0..cfg.proposals_per_iter {
            let field = fields[rng.gen_range(0..fields.len())];
            let delta = rng.gen_range(-cfg.step_mag..=cfg.step_mag);
            if delta == 0 {
                continue;
            }
            let min_v = if field == "life" || field == "total_life" {
                min_life
            } else {
                cfg.min_stat
            };
            let mut trial = best.clone();
            add_entity_stat(&mut trial, team, slot, field, delta, min_v)?;
            let score = eval(trial.clone())?;
            if score > best_score + 1e-9 {
                best_score = score;
                best = trial;
                improved = true;
                break;
            }
        }
        if !improved {
            break;
        }
    }
    Ok((best, best_score))
}

/// Serialize best scenario + score for NDJSON logging from an external driver.
#[must_use]
pub fn best_to_record(best: &Value, score: f64, label: &str) -> Value {
    json!({
        "optimizer_record": label,
        "score": score,
        "scenario": best,
    })
}

/// Histogram of win/loss from repeated noisy evaluations (Monte Carlo estimate of P(win)).
#[must_use]
pub fn win_rate_summary(scores: &[f64], win_threshold: f64) -> HashMap<&'static str, f64> {
    let n = scores.len().max(1) as f64;
    let wins = scores.iter().filter(|&&s| s >= win_threshold).count() as f64;
    let mut m = HashMap::new();
    m.insert("n", scores.len() as f64);
    m.insert("win_rate", wins / n);
    m.insert("mean_score", scores.iter().sum::<f64>() / n);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mini_scenario() -> Value {
        json!({
            "max_turns": 8,
            "random_seed": 1,
            "farmers": [],
            "teams": [],
            "entities": [[
                {"id": 1, "name": "a", "level": 50, "life": 100, "tp": 10, "mp": 6, "strength": 10, "ai": "x.leek", "weapons": [], "chips": []}
            ],[
                {"id": 2, "name": "b", "level": 50, "life": 100, "tp": 10, "mp": 6, "strength": 10, "ai": "y.leek", "weapons": [], "chips": []}
            ]]
        })
    }

    #[test]
    fn patch_life_and_loadout() {
        let mut s = mini_scenario();
        add_entity_stat(&mut s, 0, 0, "life", 50, 1).unwrap();
        assert_eq!(s["entities"][0][0]["life"], json!(150));
        set_entity_loadout(&mut s, 0, 0, &[101], &[202, 203]).unwrap();
        assert_eq!(s["entities"][0][0]["weapons"], json!([101]));
    }

    #[test]
    fn coordinate_improves_under_fake_eval() {
        let s = mini_scenario();
        let mut calls = 0usize;
        let (best, score) = coordinate_search_stats(
            s,
            0,
            0,
            |v| {
                calls += 1;
                let life = v["entities"][0][0]["life"].as_i64().unwrap_or(0);
                Ok(life as f64)
            },
            &CoordinateSearchConfig {
                max_rounds: 3,
                step: 10,
                min_stat: 0,
            },
        )
        .unwrap();
        assert!(score > 100.0);
        assert!(best["entities"][0][0]["life"].as_i64().unwrap() > 100);
        assert!(calls >= 2);
    }
}
