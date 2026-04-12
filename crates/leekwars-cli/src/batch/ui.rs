//! Terminal styling and human-readable batch output (stderr).

use std::io::{IsTerminal, Write};
use std::path::Path;
use std::time::Instant;

use super::config::BatchPlan;

/// Timestamped lines on stderr (`[batch   12.3s] …`) while a batch run is active.
/// Disabled when `--quiet` or `--json` (machine-readable stdout only).
#[derive(Debug, Clone)]
pub struct BatchLog {
    started: Instant,
    enabled: bool,
}

impl BatchLog {
    pub fn new(quiet: bool, json: bool) -> Self {
        Self {
            started: Instant::now(),
            enabled: !quiet && !json,
        }
    }

    pub fn line(&self, msg: impl std::fmt::Display) {
        if !self.enabled {
            return;
        }
        let t = self.started.elapsed().as_secs_f64();
        eprintln!("[batch {:>7.1}s] {}", t, msg);
    }

    /// Extra detail when `-v` / `--verbose` is set.
    pub fn verbose(&self, verbose: u8, msg: impl std::fmt::Display) {
        if verbose == 0 || !self.enabled {
            return;
        }
        self.line(msg);
    }
}

/// When to emit ANSI colors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub fn use_color(self, stream: &impl IsTerminal) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => stream.is_terminal(),
        }
    }
}

pub struct Styles {
    pub dim: &'static str,
    pub bold: &'static str,
    pub green: &'static str,
    pub red: &'static str,
    pub yellow: &'static str,
    pub cyan: &'static str,
    pub blue: &'static str,
    pub magenta: &'static str,
    pub reset: &'static str,
}

impl Styles {
    pub fn new(color: bool) -> Self {
        if color {
            Self {
                dim: "\x1b[2m",
                bold: "\x1b[1m",
                green: "\x1b[32m",
                red: "\x1b[31m",
                yellow: "\x1b[33m",
                cyan: "\x1b[36m",
                blue: "\x1b[34m",
                magenta: "\x1b[35m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                dim: "",
                bold: "",
                green: "",
                red: "",
                yellow: "",
                cyan: "",
                blue: "",
                magenta: "",
                reset: "",
            }
        }
    }
}

/// Human-readable plan summary (dry-run / debugging).
pub fn write_plan_preview(
    w: &mut impl Write,
    config_path: &Path,
    plan: &BatchPlan,
    styles: &Styles,
) -> std::io::Result<()> {
    let s = styles;
    writeln!(
        w,
        "{b}Batch plan{b0} {c}{}{c0}",
        config_path.display(),
        b = s.bold,
        b0 = s.reset,
        c = s.cyan,
        c0 = s.reset
    )?;
    writeln!(
        w,
        "  {d}strategy{d0} {}   {d}delay{d0} {}s base + {}s jitter   {d}max fights{d0} {}",
        plan.strategy,
        plan.delay_secs,
        plan.delay_jitter_secs,
        plan.max_fights
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into()),
        d = s.dim,
        d0 = s.reset
    )?;
    writeln!(
        w,
        "  {d}fight wait{d0} ≤{}s   {d}fight_get{d0} {}ms initial gap (exponential backoff, max ~{}ms)   {d}API retry{d0} {} tries, base {}ms",
        plan.fight_wait_max_secs,
        plan.fight_poll_interval_ms,
        (plan.fight_poll_interval_ms.saturating_mul(12)).min(25_000),
        plan.retry_max_attempts,
        plan.retry_base_delay_ms,
        d = s.dim,
        d0 = s.reset
    )?;
    writeln!(
        w,
        "  {d}stats{d0} {}   {d}log{d0} {}",
        plan.stats_path.display(),
        plan.log_path.display(),
        d = s.dim,
        d0 = s.reset
    )?;
    writeln!(
        w,
        "  {d}quota cost per fight{d0} farmer={} solo={} team={}",
        plan.quota_cost.farmer,
        plan.quota_cost.solo,
        plan.quota_cost.team,
        d = s.dim,
        d0 = s.reset
    )?;
    writeln!(
        w,
        "  {d}opponent bias (smart){d0} upset_preference={}  talent_scale={}  min_samples={}",
        plan.opponent_bias.upset_preference,
        plan.opponent_bias.talent_scale,
        plan.opponent_bias.min_samples,
        d = s.dim,
        d0 = s.reset
    )?;

    for (i, acc) in plan.accounts.iter().enumerate() {
        writeln!(
            w,
            "  {b}Account {}{b0} profile={}",
            i + 1,
            acc.profile
                .as_deref()
                .unwrap_or("(default from CLI / leekwars.toml)"),
            b = s.bold,
            b0 = s.reset
        )?;
        writeln!(
            w,
            "    {d}quota{d0} farmer={} solo={} team={}",
            acc.quota.farmer,
            acc.quota.solo,
            acc.quota.team,
            d = s.dim,
            d0 = s.reset
        )?;
        writeln!(
            w,
            "    {d}weights{d0} farmer={:.2} solo={:.2} team={:.2}",
            acc.weights.farmer,
            acc.weights.solo,
            acc.weights.team,
            d = s.dim,
            d0 = s.reset
        )?;
        if !acc.solo_leeks.is_empty() {
            writeln!(
                w,
                "    {d}solo_leeks{d0} (per-leek quota / weight for solo fights)",
                d = s.dim,
                d0 = s.reset
            )?;
            let mut keys: Vec<_> = acc.solo_leeks.keys().collect();
            keys.sort();
            for k in keys {
                let spec = &acc.solo_leeks[k];
                writeln!(
                    w,
                    "      {}{}{}  quota={}  weight={:.2}",
                    s.dim,
                    k,
                    s.reset,
                    spec.quota(),
                    spec.weight()
                )?;
            }
        } else if !acc.solo_leek_ids.is_empty() || !acc.solo_leek_names.is_empty() {
            writeln!(
                w,
                "    {d}solo leeks{d0} ids={:?} names={:?}",
                acc.solo_leek_ids,
                acc.solo_leek_names,
                d = s.dim,
                d0 = s.reset
            )?;
        }
        if let Some(tid) = acc.team_composition_id {
            writeln!(
                w,
                "    {d}team composition id{d0} {}",
                tid,
                d = s.dim,
                d0 = s.reset
            )?;
        }
        if acc.opponent_bias != plan.opponent_bias {
            writeln!(
                w,
                "    {d}opponent_bias (override){d0} upset={}  scale={}  min_samples={}",
                acc.opponent_bias.upset_preference,
                acc.opponent_bias.talent_scale,
                acc.opponent_bias.min_samples,
                d = s.dim,
                d0 = s.reset
            )?;
        }
    }
    Ok(())
}

pub fn replay_url(fight_id: i64) -> String {
    format!("https://leekwars.com/fight/{fight_id}")
}
