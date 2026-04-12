//! Human-readable summaries for [`super::BatchResult`], tailored by [`super::BatchJob`] mode.

use std::collections::HashMap;
use std::fmt::Write;

use serde_json::Value;

use super::batch::{BatchJob, BatchMode, BatchResult};
use crate::Outcome;

/// Build a readable text report for a finished batch (tables, mode-specific sections).
pub fn format_batch_human(job: &BatchJob, result: &BatchResult) -> String {
    let mut s = String::new();
    let title = mode_title(job);
    let sep = "========================================================================";
    let _ = writeln!(s, "{sep}");
    let _ = writeln!(s, "  Leek Wars batch — {title}");
    let pw_line = match job.parallel_workers {
        None => format!("default ({})", job.resolved_parallel_workers()),
        Some(0) => "0 (sequential)".to_string(),
        Some(n) => format!("{n}"),
    };
    let _ = writeln!(
        s,
        "  Runs: {}  ·  parallel_workers: {}",
        result.outcomes.len(),
        pw_line
    );
    if let Some(base) = job.seed_schedule_base {
        let _ = writeln!(
            s,
            "  seed_schedule_base: {base} (run i uses seed {base} + i)"
        );
    }
    let _ = writeln!(s, "{sep}");
    let _ = writeln!(s);

    push_summary(&mut s, result);
    let _ = writeln!(s);

    match &job.mode {
        BatchMode::RoundRobin { .. } => {
            if let Some(block) = round_robin_standings_block(job, result) {
                let _ = writeln!(s, "── Round-robin standings ──");
                let _ = writeln!(s, "{block}");
                let _ = writeln!(s);
            }
        }
        BatchMode::Sweep {
            variants,
            cartesian,
            ..
        } => {
            let _ = writeln!(s, "── Sweep ──");
            if cartesian.as_ref().is_some_and(|c| !c.blocks.is_empty()) {
                let _ = writeln!(
                    s,
                    "Cartesian product over {} block(s) (hundreds of combos possible).",
                    cartesian.as_ref().map(|c| c.blocks.len()).unwrap_or(0)
                );
            } else if !variants.is_empty() {
                let _ = writeln!(
                    s,
                    "One row per hand-written variant (same base scenario, different overrides)."
                );
            } else {
                let _ = writeln!(s, "Sweep configuration (see batch job file).");
            }
            let _ = writeln!(s);
        }
        BatchMode::VersusManyEnemies { .. } => {
            let _ = writeln!(s, "── Versus many enemies ──");
            let _ = writeln!(
                s,
                "Hero lineup is fixed per job; each row is one enemy configuration."
            );
            let _ = writeln!(s);
        }
        BatchMode::Scenarios { .. } => {
            let _ = writeln!(s, "── Scenario list ──");
            let _ = writeln!(s, "One row per scenario file.");
            let _ = writeln!(s);
        }
    }

    let _ = writeln!(s, "── Runs ──");
    push_run_table(&mut s, result);
    s
}

fn mode_title(job: &BatchJob) -> &'static str {
    match &job.mode {
        BatchMode::Scenarios { .. } => "Scenarios",
        BatchMode::Sweep { .. } => "Sweep",
        BatchMode::RoundRobin { .. } => "Round-robin",
        BatchMode::VersusManyEnemies { .. } => "Versus many enemies",
    }
}

fn push_summary(s: &mut String, result: &BatchResult) {
    let _ = writeln!(s, "── Summary ──");
    let Some(sum) = result.summary.as_ref() else {
        let _ = writeln!(s, "  (no summary)");
        return;
    };
    let _ = writeln!(s, "  Total runs: {}", sum.total);
    if sum.winners.is_empty() {
        let _ = writeln!(s, "  Winning team tallies: (none)");
    } else {
        let parts: Vec<String> = sum
            .winners
            .iter()
            .map(|(t, n)| format!("team {t}: {n}"))
            .collect();
        let _ = writeln!(s, "  Winning team tallies: {}", parts.join(", "));
    }
    let _ = writeln!(
        s,
        "  Distinct outcome hashes: {}",
        sum.outcome_hashes_sha1.len()
    );
}

fn push_run_table(s: &mut String, result: &BatchResult) {
    let labels = &result.run_labels;
    let n = result.outcomes.len();
    let label_w = labels
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(4)
        .clamp(12, 52);

    let _ = writeln!(
        s,
        "  {:>3}  {:<lw$}  {:<28}  {:>5}  {}",
        "#",
        "run",
        "winner",
        "turns",
        "hash",
        lw = label_w
    );
    let rule_len = 3 + 2 + label_w + 2 + 28 + 2 + 5 + 2 + 10 + 2;
    let _ = writeln!(s, "  {}", "—".repeat(rule_len.min(80)));

    for i in 0..n {
        let label = labels.get(i).map(String::as_str).unwrap_or("?");
        let label_disp = truncate_chars(label, label_w);
        let o = &result.outcomes[i];
        let win = winner_label(o);
        let win_disp = truncate_chars(&win, 28);
        let hash = hash_short(o);
        let _ = writeln!(
            s,
            "  {:>3}  {:<lw$}  {:<28}  {:>5}  {}",
            i,
            label_disp,
            win_disp,
            o.duration,
            hash,
            lw = label_w
        );
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    t.push('…');
    t
}

fn winner_label(outcome: &Outcome) -> String {
    let w = outcome.winner;
    if w == 0 {
        return "draw / none".to_string();
    }
    let fight = &outcome.fight;
    if let Some(arr) = fight.get("leeks").and_then(|x| x.as_array()) {
        let mut names: Vec<&str> = Vec::new();
        for l in arr {
            if l.get("team").and_then(|t| t.as_i64()) == Some(i64::from(w)) {
                if let Some(n) = l.get("name").and_then(|n| n.as_str()) {
                    names.push(n);
                }
            }
        }
        if !names.is_empty() {
            return format!("team {w} ({})", names.join(", "));
        }
    }
    format!("team {w}")
}

fn hash_short(outcome: &Outcome) -> String {
    outcome
        .logs
        .get("outcome_hash_sha1")
        .and_then(|v| v.as_str())
        .map(|h| {
            if h.len() > 10 {
                format!("{}…", &h[..10])
            } else {
                h.to_string()
            }
        })
        .unwrap_or_else(|| "—".to_string())
}

fn team_of_entity(fight: &Value, entity_id: i32) -> Option<i64> {
    let arr = fight.get("leeks")?.as_array()?;
    let eid = i64::from(entity_id);
    for l in arr {
        if l.get("id")?.as_i64()? == eid {
            return l.get("team")?.as_i64();
        }
    }
    None
}

fn rr_winner_competitor(
    o: &Outcome,
    slots: &[i32],
    swapped: bool,
    i: usize,
    j: usize,
) -> Option<usize> {
    let tw = i64::from(o.winner);
    if tw == 0 {
        return None;
    }
    let ta = team_of_entity(&o.fight, slots[0])?;
    let tb = team_of_entity(&o.fight, slots[1])?;
    let (team_i, team_j) = if swapped { (tb, ta) } else { (ta, tb) };
    if tw == team_i {
        Some(i)
    } else if tw == team_j {
        Some(j)
    } else {
        None
    }
}

fn round_robin_standings_block(job: &BatchJob, result: &BatchResult) -> Option<String> {
    let BatchMode::RoundRobin {
        competitors,
        slots,
        repeat,
        swap_sides,
        ..
    } = &job.mode
    else {
        return None;
    };
    if slots.len() < 2 {
        return None;
    }

    let mut wins: HashMap<usize, u32> = HashMap::new();
    let mut games: HashMap<usize, u32> = HashMap::new();
    let reps = (*repeat).max(1);
    let mut idx = 0usize;

    for i in 0..competitors.len() {
        for j in (i + 1)..competitors.len() {
            for _r in 0..reps {
                if let Some(o) = result.outcomes.get(idx) {
                    *games.entry(i).or_insert(0) += 1;
                    *games.entry(j).or_insert(0) += 1;
                    if let Some(wi) = rr_winner_competitor(o, slots, false, i, j) {
                        *wins.entry(wi).or_insert(0) += 1;
                    }
                }
                idx += 1;

                if *swap_sides {
                    if let Some(o) = result.outcomes.get(idx) {
                        *games.entry(i).or_insert(0) += 1;
                        *games.entry(j).or_insert(0) += 1;
                        if let Some(wi) = rr_winner_competitor(o, slots, true, i, j) {
                            *wins.entry(wi).or_insert(0) += 1;
                        }
                    }
                    idx += 1;
                }
            }
        }
    }

    if idx != result.outcomes.len() {
        // Job shape changed vs outcomes; skip misleading standings.
        return None;
    }

    let mut rows: Vec<(usize, String, u32, u32)> = Vec::new();
    for (k, c) in competitors.iter().enumerate() {
        let w = *wins.get(&k).unwrap_or(&0);
        let g = *games.get(&k).unwrap_or(&0);
        rows.push((k, c.name.clone(), w, g));
    }
    rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));

    let mut block = String::new();
    for (_k, name, w, g) in rows {
        let pct = if g > 0 {
            (100.0 * f64::from(w) / f64::from(g)) as u32
        } else {
            0
        };
        let _ = writeln!(
            &mut block,
            "  {:<24}  wins {:>3}  games {:>3}  win% {:>3}",
            truncate_chars(&name, 24),
            w,
            g,
            pct
        );
    }
    Some(block)
}
