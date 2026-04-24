//! Port of `com.leekwars.generator.state.StartOrder.compute` (entity play order).

use super::rng::TurnOrderRng;
use super::world::FightWorld;

/// Build round-robin play order from teams (each inner list sorted by frequency desc).
pub fn compute_turn_order(world: &FightWorld, rng: &mut impl TurnOrderRng) -> Vec<i32> {
    let team_count = world.team_fids.len();
    if team_count == 0 {
        return Vec::new();
    }

    let total_entities: usize = world.team_fids.iter().map(|t| t.len()).sum();
    if total_entities == 0 {
        return Vec::new();
    }

    let mut teams: Vec<Vec<i32>> = world
        .team_fids
        .iter()
        .map(|fids| {
            let mut v = fids.clone();
            // Java `Collections.sort` is stable; tie-break `fid` so equal frequencies keep scenario order.
            v.sort_by(|&a, &b| {
                let fa = world.entity(a).map(|e| e.frequency).unwrap_or(0);
                let fb = world.entity(b).map(|e| e.frequency).unwrap_or(0);
                fb.cmp(&fa).then(a.cmp(&b))
            });
            v
        })
        .collect();

    let mut frequencies: Vec<i32> = Vec::new();
    let mut sum = 0i32;
    for t in &teams {
        let f = t
            .first()
            .and_then(|&fid| world.entity(fid))
            .map(|e| e.frequency)
            .unwrap_or(0);
        frequencies.push(f);
        sum += f;
    }

    let mut probas: Vec<f64> = Vec::new();
    let mut psum = 0.0f64;
    for f in frequencies {
        let f = f as f64;
        let sum_f = sum as f64;
        let p = 1.0 / (1.0 + 10f64.powf((sum_f - f) / 100.0));
        probas.push(p);
        psum += p;
    }
    for p in &mut probas {
        *p /= psum;
    }
    psum = 1.0;

    let mut team_order: Vec<usize> = Vec::new();
    let mut remaining: Vec<usize> = (0..team_count).collect();

    for _ in 0..team_count {
        let mut v = rng.next_double01();
        for i in 0..remaining.len() {
            let team = remaining[i];
            let p = probas[team];
            if v <= p {
                team_order.push(team);
                psum -= p;
                remaining.remove(i);
                break;
            }
            v -= p;
        }

        for j in 0..team_count {
            probas[j] /= psum;
        }
        psum = 1.0;
    }

    let mut order = Vec::with_capacity(total_entities);
    let mut current_team_i = 0usize;
    while order.len() != total_entities {
        let team = team_order[current_team_i];
        if !teams[team].is_empty() {
            order.push(teams[team].remove(0));
        }
        current_team_i = (current_team_i + 1) % team_count;
    }

    order
}
