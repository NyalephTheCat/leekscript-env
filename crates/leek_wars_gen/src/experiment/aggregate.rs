//! Online aggregates for terminal summary.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ArmAggregate {
    pub arm_name: String,
    pub n_ok: u64,
    pub n_err: u64,
    pub wins_team0: u64,
    pub wins_other: u64,
    pub duration_sum: f64,
    pub duration_count: u64,
    /// Count fights that hit max duration with no winner skew (optional heuristic: duration >= 64).
    pub long_fights: u64,
}

impl ArmAggregate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            arm_name: name.into(),
            ..Default::default()
        }
    }

    pub fn record_ok(&mut self, winner: Option<i64>, duration: Option<i64>) {
        self.n_ok += 1;
        match winner {
            Some(0) => self.wins_team0 += 1,
            Some(_) => self.wins_other += 1,
            None => {}
        }
        if let Some(d) = duration {
            self.duration_sum += d as f64;
            self.duration_count += 1;
            if d >= 64 {
                self.long_fights += 1;
            }
        }
    }

    pub fn record_err(&mut self) {
        self.n_err += 1;
    }

    pub fn win_rate_team0(&self) -> f64 {
        let denom = self.wins_team0 + self.wins_other;
        if denom == 0 {
            0.0
        } else {
            self.wins_team0 as f64 / denom as f64
        }
    }

    pub fn mean_duration(&self) -> f64 {
        if self.duration_count == 0 {
            0.0
        } else {
            self.duration_sum / self.duration_count as f64
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct ExperimentAggregate {
    pub by_arm: HashMap<String, ArmAggregate>,
}

impl ExperimentAggregate {
    pub fn record(
        &mut self,
        arm: &str,
        ok: bool,
        winner: Option<i64>,
        duration: Option<i64>,
    ) {
        let e = self
            .by_arm
            .entry(arm.to_string())
            .or_insert_with(|| ArmAggregate::new(arm));
        if ok {
            e.record_ok(winner, duration);
        } else {
            e.record_err();
        }
    }

    pub fn print_table(&self) {
        eprintln!();
        eprintln!("── experiment summary ──");
        let mut names: Vec<_> = self.by_arm.keys().cloned().collect();
        names.sort();
        for name in names {
            let a = &self.by_arm[&name];
            eprintln!(
                "  {:<24} ok={} err={} win0%={:.1} mean_dur={:.1} long>={}",
                a.arm_name,
                a.n_ok,
                a.n_err,
                a.win_rate_team0() * 100.0,
                a.mean_duration(),
                a.long_fights
            );
        }
    }
}
