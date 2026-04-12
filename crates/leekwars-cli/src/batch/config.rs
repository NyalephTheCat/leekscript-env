//! TOML config for `leekwars batch run` (single-account legacy or `[[accounts]]`).

use std::collections::HashMap;
use std::path::PathBuf;

use rand::Rng;
use serde::{Deserialize, Serialize};

fn one_f64() -> f64 {
    1.0
}

/// Per-leek solo budget / relative pick weight (`[quota.solo_leeks]` or top-level `[solo_leeks]`).
///
/// TOML examples:
/// - `MyLeek = 10` — quota only, weight defaults to `1.0`
/// - `MyLeek = { quota = 10, weight = 2.0 }` — quota and weight when choosing which leek fights solo next
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SoloLeekSpec {
    QuotaOnly(u32),
    Detailed {
        quota: u32,
        #[serde(default = "one_f64")]
        weight: f64,
    },
}

impl SoloLeekSpec {
    pub fn quota(&self) -> u32 {
        match self {
            SoloLeekSpec::QuotaOnly(q) => *q,
            SoloLeekSpec::Detailed { quota, .. } => *quota,
        }
    }

    /// Relative weight when the next fight kind is solo (among leeks that still have quota).
    pub fn weight(&self) -> f64 {
        match self {
            SoloLeekSpec::QuotaOnly(_) => 1.0,
            SoloLeekSpec::Detailed { weight, .. } => *weight,
        }
    }
}

/// Top-level file (legacy fields + optional `[[accounts]]`).
#[derive(Debug, Deserialize)]
pub struct BatchFile {
    #[serde(default = "default_delay")]
    pub delay_secs: u64,
    /// Extra random seconds in `0..=delay_jitter_secs` added after each fight (on top of `delay_secs`).
    #[serde(default)]
    pub delay_jitter_secs: u64,
    pub max_fights: Option<u32>,

    /// Max time to wait for `fight/get` to report a winner.
    #[serde(default = "default_fight_wait_max_secs")]
    pub fight_wait_max_secs: u64,
    /// Initial delay **between** `fight/get` polls while a fight is running; spacing grows with exponential backoff (see runner) to avoid hammering the API.
    #[serde(default = "default_fight_poll_ms")]
    pub fight_poll_interval_ms: u64,

    /// Retries for transient API failures (5xx, timeouts, 429). Set `retry_max_attempts = 1` to disable.
    #[serde(default = "default_retry_max_attempts")]
    pub retry_max_attempts: u32,
    #[serde(default = "default_retry_base_ms")]
    pub retry_base_delay_ms: u64,

    /// Budget spent per fight type (team often > 1).
    #[serde(default)]
    pub quota_cost: QuotaCost,

    #[serde(default)]
    pub strategy: Strategy,

    #[serde(default)]
    pub opponent_bias: OpponentBias,

    #[serde(default = "default_stats_path")]
    pub stats_path: PathBuf,

    #[serde(default = "default_log_path")]
    pub log_path: PathBuf,

    /// Per-leek solo overrides (names as keys). If set, overrides `quota.solo` (sum of quotas) and
    /// `solo_leek_names` / `solo_leek_ids` for solo mode. Can also live under `[quota]` — see merge in `into_plan`.
    #[serde(default)]
    pub solo_leeks: HashMap<String, SoloLeekSpec>,

    /// Multi-account: run each entry in order (login as `profile` from `leekwars.toml`).
    #[serde(default)]
    pub accounts: Vec<AccountSection>,

    // --- legacy (used when `accounts` is empty) ---
    /// May include `solo_leek_ids`, `team_composition_id`, `solo_leek_names` inside `[quota]`.
    pub quota: Option<QuotaSpec>,
    /// Root-level fallback if not set under `[quota]`.
    pub solo_leek_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub solo_leek_names: Vec<String>,
    pub team_composition_id: Option<i64>,
    #[serde(default)]
    pub weights: Weights,
}

/// `[quota]` / per-account quota: numeric budget plus optional leek ids and team composition.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct QuotaSpec {
    #[serde(default)]
    pub farmer: u32,
    #[serde(default)]
    pub solo: u32,
    #[serde(default)]
    pub team: u32,
    #[serde(default)]
    pub solo_leek_ids: Vec<i64>,
    #[serde(default)]
    pub solo_leek_names: Vec<String>,
    #[serde(default)]
    pub solo_leeks: HashMap<String, SoloLeekSpec>,
    pub team_composition_id: Option<i64>,
}

impl QuotaSpec {
    fn to_numeric(&self) -> Quota {
        Quota {
            farmer: self.farmer,
            solo: self.solo,
            team: self.team,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct AccountSection {
    /// Profile name in `leekwars.toml` `[accounts.NAME]` (required if multiple `[[accounts]]`).
    pub profile: Option<String>,
    pub quota: QuotaSpec,
    #[serde(default)]
    pub weights: Weights,
    /// Overrides top-level `[opponent_bias]` for this account when set.
    #[serde(default)]
    pub opponent_bias: Option<OpponentBias>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct QuotaCost {
    #[serde(default = "one_u32")]
    pub farmer: u32,
    #[serde(default = "one_u32")]
    pub solo: u32,
    #[serde(default = "one_u32")]
    pub team: u32,
}

fn one_u32() -> u32 {
    1
}

impl Default for QuotaCost {
    fn default() -> Self {
        Self {
            farmer: 1,
            solo: 1,
            team: 1,
        }
    }
}

fn default_delay() -> u64 {
    30
}

fn default_fight_wait_max_secs() -> u64 {
    180
}

fn default_fight_poll_ms() -> u64 {
    2500
}

fn default_retry_max_attempts() -> u32 {
    3
}

fn default_retry_base_ms() -> u64 {
    400
}

fn default_stats_path() -> PathBuf {
    PathBuf::from("leekwars-batch-stats.json")
}

fn default_log_path() -> PathBuf {
    PathBuf::from("leekwars-batch-log.jsonl")
}

#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy)]
pub struct Quota {
    #[serde(default)]
    pub farmer: u32,
    #[serde(default)]
    pub solo: u32,
    #[serde(default)]
    pub team: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct Weights {
    #[serde(default = "one")]
    pub farmer: f64,
    #[serde(default = "one")]
    pub solo: f64,
    #[serde(default = "one")]
    pub team: f64,
}

fn one() -> f64 {
    1.0
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            farmer: 1.0,
            solo: 1.0,
            team: 1.0,
        }
    }
}

fn default_bias_talent_scale() -> f64 {
    250.0
}

/// Tunes `strategy = "smart"` opponent choice toward stronger enemies (Elo-style upside).
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq)]
pub struct OpponentBias {
    /// Extra weight for opponents with **higher talent than you** (only the gap above 0 counts).
    /// `0` keeps the old rule: best tracked win rate, then prefer higher enemy talent on ties.
    /// Try `0.15`–`0.5` to favour riskier, higher-upside fights while still respecting `win_rate`.
    #[serde(default)]
    pub upset_preference: f64,
    /// Dampens the talent gap: appeal is `gap / (talent_scale + gap)` in `[0, 1)`.
    /// Larger values make the upset term saturate more slowly (stronger enemies matter longer).
    #[serde(default = "default_bias_talent_scale")]
    pub talent_scale: f64,
    /// Until this many **recorded** games (wins+losses) exist for an opponent key, smart pick uses
    /// a neutral `0.5` rate for that key (exploration). `0` disables (always use Laplace `win_rate`).
    #[serde(default)]
    pub min_samples: u64,
}

impl Default for OpponentBias {
    fn default() -> Self {
        Self {
            upset_preference: 0.0,
            talent_scale: default_bias_talent_scale(),
            min_samples: 0,
        }
    }
}

impl OpponentBias {
    /// Combined score for picking an opponent (higher is better). `wr` is Laplace-smoothed win rate in (0,1).
    pub fn smart_score(&self, wr: f64, enemy_talent: i64, our_talent: i64) -> f64 {
        let gap = (enemy_talent as f64 - our_talent as f64).max(0.0);
        let t = self.talent_scale.max(1.0);
        let upset_appeal = gap / (t + gap);
        wr + self.upset_preference * upset_appeal
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    #[default]
    Smart,
    Random,
}

impl std::fmt::Display for Strategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Strategy::Smart => write!(f, "smart"),
            Strategy::Random => write!(f, "random"),
        }
    }
}

/// Normalized plan for the runner.
#[derive(Debug, Clone, Serialize)]
pub struct BatchPlan {
    pub delay_secs: u64,
    pub delay_jitter_secs: u64,
    pub max_fights: Option<u32>,
    pub fight_wait_max_secs: u64,
    pub fight_poll_interval_ms: u64,
    pub retry_max_attempts: u32,
    pub retry_base_delay_ms: u64,
    pub quota_cost: QuotaCost,
    pub strategy: Strategy,
    pub opponent_bias: OpponentBias,
    pub stats_path: PathBuf,
    pub log_path: PathBuf,
    pub accounts: Vec<AccountPlan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountPlan {
    /// `None` = use CLI `--profile` / default (legacy single block).
    pub profile: Option<String>,
    pub quota: Quota,
    pub weights: Weights,
    pub solo_leek_ids: Vec<i64>,
    pub solo_leek_names: Vec<String>,
    /// Non-empty ⇒ solo fights use these names only, with per-leek quota and weighted leek choice.
    pub solo_leeks: HashMap<String, SoloLeekSpec>,
    pub team_composition_id: Option<i64>,
    /// Resolved from file (and optional per-`[[accounts]]` override).
    pub opponent_bias: OpponentBias,
}

impl BatchFile {
    pub fn load(path: &std::path::Path) -> anyhow::Result<BatchPlan> {
        let text = std::fs::read_to_string(path)?;
        let raw: BatchFile = toml::from_str(&text)?;
        raw.into_plan()
    }

    fn into_plan(self) -> anyhow::Result<BatchPlan> {
        if self.accounts.is_empty() {
            let qt = self
                .quota
                .ok_or_else(|| anyhow::anyhow!("set [quota] or use [[accounts]] blocks"))?;
            let merged_solo_leeks = if !qt.solo_leeks.is_empty() {
                qt.solo_leeks.clone()
            } else {
                self.solo_leeks.clone()
            };
            validate_solo_leeks_map(&merged_solo_leeks)?;

            let mut quota = qt.to_numeric();
            if !merged_solo_leeks.is_empty() {
                quota.solo = merged_solo_leeks.values().map(SoloLeekSpec::quota).sum();
            }
            if quota.farmer == 0 && quota.solo == 0 && quota.team == 0 {
                anyhow::bail!("at least one non-zero quota.farmer, quota.solo, or quota.team");
            }
            validate_quota_weights(&quota, &self.weights)?;

            let mut solo_leek_ids = if !qt.solo_leek_ids.is_empty() {
                qt.solo_leek_ids.clone()
            } else {
                self.solo_leek_ids.unwrap_or_default()
            };
            let mut solo_leek_names = if !qt.solo_leek_names.is_empty() {
                qt.solo_leek_names.clone()
            } else {
                self.solo_leek_names
            };
            if !merged_solo_leeks.is_empty() {
                let mut keys: Vec<_> = merged_solo_leeks.keys().cloned().collect();
                keys.sort();
                solo_leek_names = keys;
                solo_leek_ids.clear();
            }
            let team_composition_id = qt.team_composition_id.or(self.team_composition_id);
            let acc = AccountPlan {
                profile: None,
                quota,
                weights: self.weights,
                solo_leek_ids,
                solo_leek_names,
                solo_leeks: merged_solo_leeks,
                team_composition_id,
                opponent_bias: self.opponent_bias,
            };
            validate_account(&acc)?;
            return Ok(BatchPlan {
                delay_secs: self.delay_secs,
                delay_jitter_secs: self.delay_jitter_secs,
                max_fights: self.max_fights,
                fight_wait_max_secs: self.fight_wait_max_secs,
                fight_poll_interval_ms: self.fight_poll_interval_ms,
                retry_max_attempts: self.retry_max_attempts,
                retry_base_delay_ms: self.retry_base_delay_ms,
                quota_cost: self.quota_cost,
                strategy: self.strategy,
                opponent_bias: self.opponent_bias,
                stats_path: self.stats_path,
                log_path: self.log_path,
                accounts: vec![acc],
            });
        }

        if self.accounts.len() > 1 {
            for a in &self.accounts {
                if a.profile.is_none() {
                    anyhow::bail!(
                        "each [[accounts]] must set profile = \"...\" when using multiple accounts"
                    );
                }
            }
        }

        let mut accounts = Vec::new();
        for a in self.accounts {
            let merged_solo_leeks = a.quota.solo_leeks.clone();
            validate_solo_leeks_map(&merged_solo_leeks)?;

            let mut qn = a.quota.to_numeric();
            let mut solo_leek_ids = a.quota.solo_leek_ids;
            let mut solo_leek_names = a.quota.solo_leek_names;
            if !merged_solo_leeks.is_empty() {
                let mut keys: Vec<_> = merged_solo_leeks.keys().cloned().collect();
                keys.sort();
                solo_leek_names = keys;
                solo_leek_ids.clear();
                qn.solo = merged_solo_leeks.values().map(SoloLeekSpec::quota).sum();
            }
            if qn.farmer == 0 && qn.solo == 0 && qn.team == 0 {
                anyhow::bail!("account has empty quota");
            }
            validate_quota_weights(&qn, &a.weights)?;
            let acc = AccountPlan {
                profile: a.profile,
                quota: qn,
                weights: a.weights,
                solo_leek_ids,
                solo_leek_names,
                solo_leeks: merged_solo_leeks,
                team_composition_id: a.quota.team_composition_id,
                opponent_bias: a.opponent_bias.unwrap_or(self.opponent_bias),
            };
            validate_account(&acc)?;
            accounts.push(acc);
        }

        Ok(BatchPlan {
            delay_secs: self.delay_secs,
            delay_jitter_secs: self.delay_jitter_secs,
            max_fights: self.max_fights,
            fight_wait_max_secs: self.fight_wait_max_secs,
            fight_poll_interval_ms: self.fight_poll_interval_ms,
            retry_max_attempts: self.retry_max_attempts,
            retry_base_delay_ms: self.retry_base_delay_ms,
            quota_cost: self.quota_cost,
            strategy: self.strategy,
            opponent_bias: self.opponent_bias,
            stats_path: self.stats_path,
            log_path: self.log_path,
            accounts,
        })
    }
}

fn validate_solo_leeks_map(m: &HashMap<String, SoloLeekSpec>) -> anyhow::Result<()> {
    if m.is_empty() {
        return Ok(());
    }
    let sum: u32 = m.values().map(SoloLeekSpec::quota).sum();
    if sum == 0 {
        anyhow::bail!("solo_leeks: sum of quotas must be > 0");
    }
    for (name, spec) in m {
        if spec.quota() == 0 {
            anyhow::bail!("solo_leeks: quota for {name:?} must be > 0");
        }
        if spec.weight() <= 0.0 {
            anyhow::bail!("solo_leeks: weight for {name:?} must be > 0");
        }
    }
    Ok(())
}

fn validate_quota_weights(q: &Quota, w: &Weights) -> anyhow::Result<()> {
    if q.farmer > 0 && w.farmer <= 0.0 {
        anyhow::bail!("quota.farmer > 0 requires weights.farmer > 0");
    }
    if q.solo > 0 && w.solo <= 0.0 {
        anyhow::bail!("quota.solo > 0 requires weights.solo > 0");
    }
    if q.team > 0 && w.team <= 0.0 {
        anyhow::bail!("quota.team > 0 requires weights.team > 0");
    }
    Ok(())
}

fn validate_account(a: &AccountPlan) -> anyhow::Result<()> {
    if a.quota.farmer == 0 && a.quota.solo == 0 && a.quota.team == 0 {
        anyhow::bail!("account has empty quota");
    }
    if a.quota.solo > 0 && a.solo_leek_ids.is_empty() && a.solo_leek_names.is_empty() {
        anyhow::bail!("quota.solo > 0 requires solo_leek_ids and/or solo_leek_names");
    }
    if a.quota.team > 0 && a.team_composition_id.is_none() {
        anyhow::bail!("quota.team > 0 requires team_composition_id");
    }
    Ok(())
}

impl BatchPlan {
    pub fn validate_costs(&self) -> anyhow::Result<()> {
        let c = &self.quota_cost;
        if c.farmer == 0 || c.solo == 0 || c.team == 0 {
            anyhow::bail!("quota_cost.* must all be >= 1");
        }
        Ok(())
    }
}

/// Remaining **budget units** per axis (team fights subtract `quota_cost.team` each).
#[derive(Debug, Clone)]
pub struct QuotaLeft {
    pub farmer: u32,
    pub solo: u32,
    pub team: u32,
}

impl QuotaLeft {
    pub fn from_config(q: &Quota) -> Self {
        Self {
            farmer: q.farmer,
            solo: q.solo,
            team: q.team,
        }
    }

    pub fn total_left(&self) -> u32 {
        self.farmer + self.solo + self.team
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FightKind {
    Farmer,
    Solo,
    Team,
}

impl FightKind {
    pub fn weight(self, w: &Weights) -> f64 {
        match self {
            FightKind::Farmer => w.farmer,
            FightKind::Solo => w.solo,
            FightKind::Team => w.team,
        }
    }
}

pub fn kinds_affordable(q: &QuotaLeft, cost: &QuotaCost) -> Vec<FightKind> {
    let mut v = Vec::new();
    if q.farmer >= cost.farmer {
        v.push(FightKind::Farmer);
    }
    if q.solo >= cost.solo {
        v.push(FightKind::Solo);
    }
    if q.team >= cost.team {
        v.push(FightKind::Team);
    }
    v
}

pub fn can_afford_any(q: &QuotaLeft, cost: &QuotaCost) -> bool {
    !kinds_affordable(q, cost).is_empty()
}

pub fn pick_next_kind(q: &QuotaLeft, w: &Weights, cost: &QuotaCost) -> Option<FightKind> {
    let kinds = kinds_affordable(q, cost);
    if kinds.is_empty() {
        return None;
    }
    let weighted: Vec<(FightKind, f64)> = kinds
        .iter()
        .copied()
        .map(|k| (k, k.weight(w)))
        .filter(|(_, x)| *x > 0.0)
        .collect();
    if weighted.is_empty() {
        return None;
    }
    let sum: f64 = weighted.iter().map(|(_, x)| x).sum();
    let mut r = rand::thread_rng().gen_range(0.0..1.0) * sum;
    let mut last = weighted[0].0;
    for (k, wt) in weighted {
        last = k;
        r -= wt;
        if r <= 0.0 {
            return Some(k);
        }
    }
    Some(last)
}

pub fn apply_cost(q: &mut QuotaLeft, kind: FightKind, cost: &QuotaCost) {
    match kind {
        FightKind::Farmer => q.farmer = q.farmer.saturating_sub(cost.farmer),
        FightKind::Solo => q.solo = q.solo.saturating_sub(cost.solo),
        FightKind::Team => q.team = q.team.saturating_sub(cost.team),
    }
}
