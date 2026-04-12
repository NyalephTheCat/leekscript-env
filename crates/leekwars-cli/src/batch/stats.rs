//! Persistent win/loss per opponent key (JSON) for smart opponent selection.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

const STATS_VERSION: u32 = 1;

fn default_version() -> u32 {
    STATS_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BatchStats {
    #[serde(default = "default_version")]
    pub version: u32,
    /// Per-opponent aggregate wins/losses.
    pub opponents: HashMap<String, WinLoss>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct WinLoss {
    pub wins: u64,
    pub losses: u64,
}

impl BatchStats {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.is_file() {
            return Ok(Self {
                version: STATS_VERSION,
                opponents: HashMap::new(),
            });
        }
        let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
        let mut s: BatchStats =
            serde_json::from_str(&text).with_context(|| path.display().to_string())?;
        if s.version == 0 {
            s.version = STATS_VERSION;
        }
        Ok(s)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = self.clone();
        out.version = STATS_VERSION;
        let text = serde_json::to_string_pretty(&out)?;
        std::fs::write(path, text).with_context(|| path.display().to_string())?;
        Ok(())
    }

    /// Laplace-smoothed win rate in (0,1), higher = historically better for us.
    pub fn win_rate(&self, key: &str) -> f64 {
        let wl = self.opponents.get(key).copied().unwrap_or_default();
        let w = wl.wins as f64;
        let l = wl.losses as f64;
        (w + 1.0) / (w + l + 2.0)
    }

    /// If we have at least one recorded game against `key`, returns the Laplace-smoothed win rate
    /// in `(0, 1)` — the best **pre-fight** estimate for logging. Otherwise `None` (no history).
    pub fn historical_win_rate_if_known(&self, key: &str) -> Option<f64> {
        let wl = self.opponents.get(key).copied().unwrap_or_default();
        if wl.wins + wl.losses == 0 {
            None
        } else {
            Some(self.win_rate(key))
        }
    }

    /// Like [`win_rate`], but if `min_samples > 0` and wins+losses for `key` are below that total,
    /// returns `0.5` (uninformative) so smart picking explores until enough games are recorded.
    pub fn win_rate_with_min_samples(&self, key: &str, min_samples: u64) -> f64 {
        if min_samples == 0 {
            return self.win_rate(key);
        }
        let wl = self.opponents.get(key).copied().unwrap_or_default();
        let n = wl.wins + wl.losses;
        if n < min_samples {
            0.5
        } else {
            self.win_rate(key)
        }
    }

    pub fn record(&mut self, key: &str, won: bool) {
        let e = self.opponents.entry(key.to_string()).or_default();
        if won {
            e.wins += 1;
        } else {
            e.losses += 1;
        }
    }

    pub fn overall(&self) -> (u64, u64) {
        self.opponents
            .values()
            .fold((0u64, 0u64), |acc, wl| (acc.0 + wl.wins, acc.1 + wl.losses))
    }

    pub fn clear(&mut self) {
        self.opponents.clear();
    }
}

/// Stats keys are scoped by our farmer id so multiple accounts can share one JSON file.
pub fn key_solo(our_farmer_id: i64, our_leek_id: i64, target_leek_id: i64) -> String {
    format!("m{our_farmer_id}:solo:L{our_leek_id}:opp:{target_leek_id}")
}

pub fn key_farmer(our_farmer_id: i64, target_farmer_id: i64) -> String {
    format!("m{our_farmer_id}:farm:{target_farmer_id}")
}

pub fn key_team(
    our_farmer_id: i64,
    our_composition_id: i64,
    target_composition_id: i64,
) -> String {
    format!("m{our_farmer_id}:team:{our_composition_id}:opp:{target_composition_id}")
}

/// Human-readable table of opponent history.
pub fn print_table(stats: &BatchStats, path: &Path) {
    println!("File: {}", path.display());
    let (w, l) = stats.overall();
    let pct = if w + l > 0 {
        100.0 * (w as f64) / ((w + l) as f64)
    } else {
        0.0
    };
    println!("Overall: {w} wins, {l} losses ({pct:.1}% wins)\n");
    let mut keys: Vec<_> = stats.opponents.keys().cloned().collect();
    keys.sort();
    println!(
        "{:<50} {:>6} {:>6} {:>8}",
        "opponent_key", "wins", "loss", "est_wr%"
    );
    for k in keys {
        let wl = stats.opponents.get(&k).copied().unwrap_or_default();
        let est = stats.win_rate(&k) * 100.0;
        println!("{:<50} {:>6} {:>6} {:>7.1}%", k, wl.wins, wl.losses, est);
    }
}
