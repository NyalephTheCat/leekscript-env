//! Heuristic optimizer: component slots + capital toward target totals.

use std::collections::HashMap;

use rand::Rng;
use rand::SeedableRng;
use serde_json::Value;

use super::capital_cost::{
    base_stat, capital_spent_for_invested, greedy_allocate_capital, invested_from_raw, CAPITAL_STATS,
};
use super::data::{sum_stats_maps, GameDataIndex};

pub struct OptimizeInput {
    pub level: i64,
    pub capital_budget: i64,
    pub target_totals: HashMap<String, i64>,
    pub weights: HashMap<String, f64>,
    /// Item template ids allowed in slots (component items).
    pub allowed_templates: Vec<i64>,
    pub data: GameDataIndex,
    /// Random restarts (each runs a short hill climb).
    pub restarts: u32,
    /// Steps per restart (neighbor moves).
    pub hill_steps: u32,
    pub seed: u64,
}

#[derive(Debug, Clone)]
pub struct OptimizeResult {
    /// Component item template per slot (`None` = empty).
    pub slots: [Option<i64>; 8],
    pub invested: HashMap<String, i64>,
    pub predicted_totals: HashMap<String, i64>,
    pub score: f64,
    pub capital_used: i64,
}

fn mse(
    weights: &HashMap<String, f64>,
    got: &HashMap<String, i64>,
    target: &HashMap<String, i64>,
) -> f64 {
    let mut s = 0.0;
    for stat in CAPITAL_STATS {
        let w = weights.get(stat).copied().unwrap_or(1.0);
        let g = *got.get(stat).unwrap_or(&0) as f64;
        let t = *target.get(stat).unwrap_or(&0) as f64;
        s += w * (g - t).powi(2);
    }
    s
}

fn evaluate(input: &OptimizeInput, slots: &[Option<i64>; 8]) -> OptimizeResult {
    let comp_maps = slots.iter().filter_map(|t| {
        let tpl = (*t)?;
        input.data.stats_for_component_item_template(tpl)
    });
    let comp_sum = sum_stats_maps(comp_maps);
    let invested = greedy_allocate_capital(
        input.level,
        input.capital_budget,
        &input.weights,
        &comp_sum,
        &input.target_totals,
    );
    let mut predicted = HashMap::new();
    let mut cap_used = 0i64;
    for stat in CAPITAL_STATS {
        let b = base_stat(input.level, stat);
        let inv = *invested.get(stat).unwrap_or(&0);
        let c = *comp_sum.get(stat).unwrap_or(&0);
        predicted.insert(stat.to_string(), b + inv + c);
        cap_used += capital_spent_for_invested(stat, inv);
    }
    let score = mse(&input.weights, &predicted, &input.target_totals);
    OptimizeResult {
        slots: *slots,
        invested,
        predicted_totals: predicted,
        score,
        capital_used: cap_used,
    }
}

/// Randomized hill-climbing with restarts.
pub fn optimize(input: OptimizeInput) -> OptimizeResult {
    let mut rng = rand::rngs::StdRng::seed_from_u64(input.seed);
    let choices: Vec<Option<i64>> = std::iter::once(None)
        .chain(input.allowed_templates.iter().copied().map(Some))
        .collect();
    if choices.is_empty() {
        let slots = [None; 8];
        return evaluate(&input, &slots);
    }

    let mut best: Option<OptimizeResult> = None;
    let restarts = input.restarts.max(1);
    let steps = input.hill_steps.max(50);

    for _restart in 0..restarts {
        let mut slots: [Option<i64>; 8] = [None; 8];
        for s in &mut slots {
            *s = choices[rng.gen_range(0..choices.len())].clone();
        }
        let mut cur = evaluate(&input, &slots);
        let mut local_best = cur.clone();

        for _ in 0..steps {
            let mut next = slots;
            let slot = rng.gen_range(0..8);
            next[slot] = choices[rng.gen_range(0..choices.len())].clone();
            let trial = evaluate(&input, &next);
            if trial.score < cur.score - 1e-6 {
                cur = trial.clone();
                slots = next;
                if trial.score < local_best.score {
                    local_best = trial;
                }
            }
        }

        match &best {
            None => best = Some(local_best),
            Some(b) if local_best.score < b.score => best = Some(local_best),
            _ => {}
        }
    }

    best.unwrap_or_else(|| {
        let slots = [None; 8];
        evaluate(&input, &slots)
    })
}

/// Build target totals from a reference leek JSON (`leek/get`) using `total_*` fields.
pub fn target_totals_from_leek_json(leek: &Value) -> HashMap<String, i64> {
    let level = leek.get("level").and_then(|x| x.as_i64()).unwrap_or(1);
    let mut m = HashMap::new();
    for stat in CAPITAL_STATS {
        let key = format!("total_{stat}");
        let v = leek
            .get(key.as_str())
            .and_then(|x| x.as_i64())
            .or_else(|| leek.get(stat).and_then(|x| x.as_i64()))
            .unwrap_or_else(|| base_stat(level, stat));
        m.insert(stat.to_string(), v);
    }
    m
}

/// Invested stats implied by raw API characteristics (no components).
pub fn invested_map_from_leek_json(leek: &Value) -> HashMap<String, i64> {
    let level = leek.get("level").and_then(|x| x.as_i64()).unwrap_or(1);
    let mut m = HashMap::new();
    for stat in CAPITAL_STATS {
        let raw = leek
            .get(stat)
            .and_then(|x| x.as_i64())
            .unwrap_or_else(|| base_stat(level, stat));
        m.insert(stat.to_string(), invested_from_raw(raw, level, stat));
    }
    m
}
