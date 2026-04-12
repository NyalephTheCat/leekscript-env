//! Capital cost tables for characteristic investment (matches Leek Wars web client `COSTS`).

/// Stats that can receive capital (same keys as `leek/spend-capital` JSON).
pub const CAPITAL_STATS: [&str; 12] = [
    "life", "strength", "wisdom", "agility", "resistance", "science", "magic", "frequency",
    "cores", "ram", "tp", "mp",
];

#[derive(Clone, Copy)]
struct CostBracket {
    step: i64,
    capital: i64,
    sup: i64,
}

fn brackets(stat: &str) -> &'static [CostBracket] {
    match stat {
        "life" => &[
            CostBracket { step: 0, capital: 1, sup: 4 },
            CostBracket { step: 1000, capital: 1, sup: 3 },
            CostBracket { step: 2000, capital: 1, sup: 2 },
        ],
        "strength" | "wisdom" | "agility" | "resistance" | "science" | "magic" => &[
            CostBracket { step: 0, capital: 1, sup: 2 },
            CostBracket { step: 200, capital: 1, sup: 1 },
            CostBracket { step: 400, capital: 2, sup: 1 },
            CostBracket { step: 600, capital: 3, sup: 1 },
        ],
        "frequency" => &[CostBracket {
            step: 0,
            capital: 1,
            sup: 1,
        }],
        "cores" | "ram" => &[
            CostBracket { step: 0, capital: 20, sup: 1 },
            CostBracket { step: 1, capital: 30, sup: 1 },
            CostBracket { step: 2, capital: 40, sup: 1 },
            CostBracket { step: 3, capital: 50, sup: 1 },
            CostBracket { step: 4, capital: 60, sup: 1 },
            CostBracket { step: 5, capital: 70, sup: 1 },
            CostBracket { step: 6, capital: 80, sup: 1 },
            CostBracket { step: 7, capital: 90, sup: 1 },
            CostBracket { step: 8, capital: 100, sup: 1 },
        ],
        "tp" => &[
            CostBracket { step: 0, capital: 30, sup: 1 },
            CostBracket { step: 1, capital: 35, sup: 1 },
            CostBracket { step: 2, capital: 40, sup: 1 },
            CostBracket { step: 3, capital: 45, sup: 1 },
            CostBracket { step: 4, capital: 50, sup: 1 },
            CostBracket { step: 5, capital: 55, sup: 1 },
            CostBracket { step: 6, capital: 60, sup: 1 },
            CostBracket { step: 7, capital: 65, sup: 1 },
            CostBracket { step: 8, capital: 70, sup: 1 },
            CostBracket { step: 9, capital: 75, sup: 1 },
            CostBracket { step: 10, capital: 80, sup: 1 },
            CostBracket { step: 11, capital: 85, sup: 1 },
            CostBracket { step: 12, capital: 90, sup: 1 },
            CostBracket { step: 13, capital: 95, sup: 1 },
            CostBracket { step: 14, capital: 100, sup: 1 },
        ],
        "mp" => &[
            CostBracket { step: 0, capital: 20, sup: 1 },
            CostBracket { step: 1, capital: 40, sup: 1 },
            CostBracket { step: 2, capital: 60, sup: 1 },
            CostBracket { step: 3, capital: 80, sup: 1 },
            CostBracket { step: 4, capital: 100, sup: 1 },
            CostBracket { step: 5, capital: 120, sup: 1 },
            CostBracket { step: 6, capital: 140, sup: 1 },
            CostBracket { step: 7, capital: 160, sup: 1 },
            CostBracket { step: 8, capital: 180, sup: 1 },
        ],
        _ => &[],
    }
}

/// Base characteristic values from level (before capital and components).
pub fn base_stat(level: i64, stat: &str) -> i64 {
    let lv = level.max(1);
    match stat {
        "life" => 100 + (lv - 1) * 3,
        "frequency" => 100,
        "cores" => 1,
        "ram" => 6,
        "tp" => 10,
        "mp" => 3,
        "strength" | "wisdom" | "agility" | "resistance" | "science" | "magic" => 0,
        _ => 0,
    }
}

/// Invested amount = `leek[stat] - base` for API values (no component bonuses).
pub fn invested_from_raw(leek_value: i64, level: i64, stat: &str) -> i64 {
    leek_value - base_stat(level, stat)
}

fn active_bracket_index(stat: &str, total_invested: i64) -> usize {
    let b = brackets(stat);
    if b.is_empty() {
        return 0;
    }
    let mut step = 0usize;
    while step < b.len() && b[step].step <= total_invested {
        step += 1;
    }
    step.saturating_sub(1)
}

/// One purchase step from current invested bonus `total_invested` (same semantics as the web UI).
pub fn next_increment(stat: &str, total_invested: i64) -> Option<(i64, i64)> {
    let b = brackets(stat);
    if b.is_empty() {
        return None;
    }
    let i = active_bracket_index(stat, total_invested);
    Some((b[i].capital, b[i].sup))
}

/// Total capital spent to reach `invested` bonus points (from 0), matching `characteristic-tooltip.vue`.
pub fn capital_spent_for_invested(stat: &str, invested: i64) -> i64 {
    if invested <= 0 {
        return 0;
    }
    let mut charac_added = 0i64;
    let mut used = 0i64;
    while charac_added < invested {
        let Some((cost, sup)) = next_increment(stat, charac_added) else {
            break;
        };
        charac_added += sup;
        used += cost;
    }
    used
}

/// Greedy: spend up to `budget` capital to move totals toward `target_totals`
/// (minimize weighted squared error per marginal capital).
pub fn greedy_allocate_capital(
    level: i64,
    budget: i64,
    weights: &std::collections::HashMap<String, f64>,
    fixed_from_components: &std::collections::HashMap<String, i64>,
    target_totals: &std::collections::HashMap<String, i64>,
) -> std::collections::HashMap<String, i64> {
    use std::collections::HashMap;
    let mut invested: HashMap<String, i64> = HashMap::new();
    for s in CAPITAL_STATS {
        invested.insert(s.to_string(), 0);
    }
    let mut remaining = budget;
    loop {
        let mut best: Option<(&str, i64, i64, f64)> = None;
        for stat in CAPITAL_STATS {
            let w = weights.get(stat).copied().unwrap_or(1.0);
            if w == 0.0 {
                continue;
            }
            let cur = *invested.get(stat).unwrap_or(&0);
            let Some((cost, sup)) = next_increment(stat, cur) else {
                continue;
            };
            if cost > remaining || cost <= 0 || sup <= 0 {
                continue;
            }
            let base = base_stat(level, stat);
            let comp = *fixed_from_components.get(stat).unwrap_or(&0);
            let tgt = *target_totals.get(stat).unwrap_or(&0);
            let cur_total = base + cur + comp;
            let err_before = (tgt - cur_total).pow(2);
            let err_after = (tgt - (cur_total + sup)).pow(2);
            let gain = w * (err_before - err_after) as f64 / cost as f64;
            let replace = match best {
                None => true,
                Some((_, _, _, g)) => gain > g + 1e-9,
            };
            if replace {
                best = Some((stat, cost, sup, gain));
            }
        }
        let Some((bs, cost, sup, _)) = best else {
            break;
        };
        *invested.get_mut(bs).unwrap() += sup;
        remaining -= cost;
    }
    invested
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_first_points() {
        assert_eq!(next_increment("strength", 0), Some((1, 2)));
        assert_eq!(next_increment("strength", 199), Some((1, 2)));
        assert_eq!(next_increment("strength", 200), Some((1, 1)));
    }

    #[test]
    fn capital_monotonic() {
        let c400 = capital_spent_for_invested("strength", 400);
        let c401 = capital_spent_for_invested("strength", 401);
        assert!(c401 > c400);
    }
}
