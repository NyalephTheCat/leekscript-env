//! Run a batch of garden fights from TOML config.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use leekwars_api::Error as ApiError;
use leekwars_api::LeekWarsClient;
use rand::Rng;
use serde::Serialize;
use serde_json::Value;

use super::config::{
    AccountPlan, BatchPlan, FightKind, OpponentBias, QuotaLeft, Strategy, apply_cost,
    can_afford_any, pick_next_kind,
};
use super::stats::{BatchStats, key_farmer, key_solo, key_team};
use super::ui::{BatchLog, ColorChoice, Styles, replay_url, write_plan_preview};

/// Retries when the API returns an empty opponent list (queue not ready yet).
const GARDEN_EMPTY_RETRIES: u32 = 6;
const GARDEN_EMPTY_RETRY_SECS: u64 = 3;

/// If every opponent has recorded games and every Laplace win rate is **below** this, `smart` falls back to uniform random (avoids always picking the “least bad” same target).
const SMART_FALLBACK_MAX_WIN_RATE: f64 = 0.45;
use crate::cli::Cli;
use crate::config;

/// Options for [`run_batch`] (CLI flags + TTY behavior).
#[derive(Debug, Clone, Copy)]
pub struct BatchRunOptions {
    pub json: bool,
    pub quiet: bool,
    /// Per-fight detail: 0 = default, 1 = extra context, 2 = dump JSON snippets (not implemented).
    pub verbose: u8,
    pub no_progress: bool,
    pub color: ColorChoice,
}

impl BatchRunOptions {
    pub fn from_cli(cli: &Cli, verbose: u8, quiet: bool, no_progress: bool) -> Self {
        Self {
            json: cli.json,
            quiet,
            verbose,
            no_progress,
            color: cli.color,
        }
    }

    fn show_fight_lines(self) -> bool {
        !self.json && !self.quiet && !self.no_progress
    }

    fn use_stderr_color(self) -> bool {
        self.color.use_color(&std::io::stderr())
    }
}

fn is_transient_api_error(e: &ApiError) -> bool {
    match e {
        ApiError::Http { status, .. } => *status == 429 || (500..600).contains(status),
        ApiError::Request(re) => re.is_timeout() || re.is_connect(),
        _ => false,
    }
}

fn backoff_delay_ms(base_ms: u64, attempt_1_based: u32) -> u64 {
    let pow = (attempt_1_based.saturating_sub(1)).min(8);
    base_ms.saturating_mul(1u64 << pow).min(30_000)
}

/// Backoff after a failed request we will retry. HTTP **429** uses exponential backoff with a
/// higher ceiling (120s) and honors `Retry-After` when the server sends it (max wait 5 minutes).
/// Other transient errors keep the shorter 30s cap.
fn backoff_after_transient(plan: &BatchPlan, attempt: u32, e: &ApiError) -> u64 {
    match e {
        ApiError::Http {
            status: 429,
            retry_after_secs,
            ..
        } => {
            let base = plan.retry_base_delay_ms.max(1);
            let pow = (attempt.saturating_sub(1)).min(8);
            let exponential_ms = base.saturating_mul(1u64 << pow).min(120_000);
            let server_ms = retry_after_secs
                .and_then(|s| s.checked_mul(1000))
                .unwrap_or(0);
            server_ms.max(exponential_ms).min(300_000)
        }
        _ => backoff_delay_ms(plan.retry_base_delay_ms, attempt),
    }
}

async fn farmer_login_with_retry(
    plan: &BatchPlan,
    client: &mut LeekWarsClient,
    login: &str,
    password: &str,
) -> anyhow::Result<()> {
    let max = plan.retry_max_attempts.max(1);
    for attempt in 1..=max {
        match client.farmer_login(login, password, false).await {
            Ok(_) => return Ok(()),
            Err(e) if attempt < max && is_transient_api_error(&e) => {
                let ms = backoff_after_transient(plan, attempt, &e);
                tokio::time::sleep(Duration::from_millis(ms)).await;
            }
            Err(e) => anyhow::bail!("farmer_login: {e}"),
        }
    }
    unreachable!("farmer_login_with_retry: loop should return or bail")
}

async fn retry_api_call<T, F, Fut>(plan: &BatchPlan, label: &'static str, mut f: F) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<T, ApiError>>,
{
    let max = plan.retry_max_attempts.max(1);
    for attempt in 1..=max {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < max && is_transient_api_error(&e) => {
                let ms = backoff_after_transient(plan, attempt, &e);
                tokio::time::sleep(Duration::from_millis(ms)).await;
            }
            Err(e) => anyhow::bail!("{label}: {e}"),
        }
    }
    anyhow::bail!("{label}: exhausted retries")
}

/// Sleeps `total_secs` in 1s steps; returns early if `cancel` is set (e.g. Ctrl+C).
async fn sleep_between_fights_secs(cancel: &AtomicBool, total_secs: u64) {
    for _ in 0..total_secs {
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn run_batch(
    plan: &BatchPlan,
    cli: &Cli,
    stats: &mut BatchStats,
    opts: BatchRunOptions,
) -> anyhow::Result<BatchRunSummary> {
    plan.validate_costs()?;
    if plan.accounts.len() > 1 && (cli.login.is_some() || cli.password.is_some()) {
        anyhow::bail!(
            "multi-account batch: do not pass --login/--password (use profiles in leekwars.toml only)"
        );
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let c = Arc::clone(&cancel);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        c.store(true, Ordering::SeqCst);
    });

    let started = Instant::now();
    let mut summary = BatchRunSummary::default();
    let mut fight_n: u32 = 0;
    let styles = Styles::new(opts.use_stderr_color());
    let log = BatchLog::new(opts.quiet, opts.json);
    log.line(format!(
        "start  accounts={}  strategy={}  max_fights={}  stats={}  log={}",
        plan.accounts.len(),
        plan.strategy,
        plan.max_fights
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into()),
        plan.stats_path.display(),
        plan.log_path.display()
    ));

    for acc in &plan.accounts {
        if cancel.load(Ordering::SeqCst) {
            summary.interrupted = true;
            break;
        }
        let (login, password) = auth_for_plan_account(cli, acc)?;
        let mut client = LeekWarsClient::new().map_err(|e| anyhow::anyhow!("{e}"))?;
        farmer_login_with_retry(plan, &mut client, &login, &password).await?;

        let part = run_one_account(
            plan,
            acc,
            &mut client,
            stats,
            &mut fight_n,
            opts,
            &styles,
            &log,
            &cancel,
        )
        .await?;
        summary.fights.extend(part.fights);
        summary.wins += part.wins;
        summary.losses += part.losses;
        summary.by_kind.merge(&part.by_kind);
        if part.interrupted {
            summary.interrupted = true;
            break;
        }
    }

    summary.elapsed_secs = started.elapsed().as_secs_f64();

    if !opts.json && !opts.quiet {
        print_finish_banner(&summary, plan, opts, &styles)?;
    }

    Ok(summary)
}

/// Print resolved plan without logging in or calling the API.
pub fn print_dry_run(
    plan: &BatchPlan,
    config_path: &Path,
    json: bool,
    color: ColorChoice,
) -> anyhow::Result<()> {
    let st = Styles::new(color.use_color(&std::io::stdout()));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "config_path": config_path.display().to_string(),
                "plan": plan,
            }))?
        );
    } else {
        let mut out = std::io::stdout();
        write_plan_preview(&mut out, config_path, plan, &st)?;
    }
    Ok(())
}

fn emit_fight_line(
    styles: &Styles,
    opts: BatchRunOptions,
    n: u32,
    max: Option<u32>,
    o: &FightOutcome,
    q: &QuotaLeft,
) -> anyhow::Result<()> {
    let s = styles;
    let mut w = std::io::stderr();
    let cap = max.map(|m| format!("/{m}")).unwrap_or_default();
    let solo_note = match (&o.solo_leek_name, o.our_leek_id) {
        (Some(name), _) => format!(" ({name})"),
        (None, Some(id)) => format!(" (leek {id})"),
        _ => String::new(),
    };
    let (word, col) = if o.won { ("WIN", s.green) } else { ("LOSS", s.red) };
    let kind_col = format!("{}{}", o.kind, solo_note);
    if let Some(p) = &o.profile {
        write!(w, "{}{}{} ", s.cyan, p, s.reset)?;
    }
    let exp_note = o
        .expected_win_rate
        .map(|p| format!(" · E(win)~{:.1}%", p * 100.0))
        .unwrap_or_default();
    writeln!(
        w,
        "[{n}{cap}] {}{}{}{} · vs {} · talent {} · fight {} · {}{}",
        col,
        word,
        s.reset,
        kind_col,
        o.target_id,
        o.enemy_talent,
        o.fight_id,
        replay_url(o.fight_id),
        exp_note,
    )?;
    if opts.verbose > 0 {
        writeln!(
            w,
            "         {}left{} farmer {} · solo {} · team {}",
            s.dim,
            s.reset,
            q.farmer,
            q.solo,
            q.team
        )?;
    }
    Ok(())
}

fn print_finish_banner(
    summary: &BatchRunSummary,
    plan: &BatchPlan,
    _opts: BatchRunOptions,
    styles: &Styles,
) -> anyhow::Result<()> {
    let s = styles;
    let mut w = std::io::stderr();
    let total = summary.fights.len();
    let wr = if total > 0 {
        100.0 * (summary.wins as f64) / (total as f64)
    } else {
        0.0
    };
    writeln!(
        w,
        "\n{}Done{}: {}{} wins{} · {}{} losses{} ({:.1}% wins) · {} fights · {:.1}s{}",
        s.bold,
        s.reset,
        s.green,
        summary.wins,
        s.reset,
        s.red,
        summary.losses,
        s.reset,
        wr,
        total,
        summary.elapsed_secs,
        if summary.interrupted {
            format!("  {}{}{}", s.yellow, "· interrupted", s.reset)
        } else {
            String::new()
        }
    )?;
    if total > 0 {
        writeln!(
            w,
            "  {}By kind{}  farmer {}-{}  solo {}-{}  team {}-{}",
            s.dim,
            s.reset,
            summary.by_kind.farmer.wins,
            summary.by_kind.farmer.losses,
            summary.by_kind.solo.wins,
            summary.by_kind.solo.losses,
            summary.by_kind.team.wins,
            summary.by_kind.team.losses,
        )?;
    }
    writeln!(
        w,
        "  {}Stats{} {}   {}Log{} {}",
        s.dim,
        s.reset,
        plan.stats_path.display(),
        s.dim,
        s.reset,
        plan.log_path.display(),
    )?;
    Ok(())
}

fn auth_for_plan_account(cli: &Cli, acc: &AccountPlan) -> anyhow::Result<(String, String)> {
    let merged = acc.profile.clone().or_else(|| cli.profile.clone());
    config::resolve_credentials(config::AuthInput {
        login: &cli.login,
        password: &cli.password,
        profile: &merged,
        config: cli.config.as_deref(),
    })
}

#[derive(Debug, Clone)]
struct SoloLeekRow {
    leek_id: i64,
    display_name: Option<String>,
    remaining: u32,
    weight: f64,
}

fn init_solo_rows(
    acc: &AccountPlan,
    resolved: &HashMap<String, i64>,
) -> anyhow::Result<Vec<SoloLeekRow>> {
    let mut keys: Vec<_> = acc.solo_leeks.keys().collect();
    keys.sort();
    let mut rows = Vec::new();
    for k in keys {
        let spec = acc.solo_leeks.get(k).expect("key from sorted solo_leeks keys");
        let norm = k.trim().to_lowercase();
        let &id = resolved.get(&norm).ok_or_else(|| {
            anyhow::anyhow!(
                "solo_leeks: leek name not found on this farmer: {k:?} (known names: {})",
                resolved.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;
        rows.push(SoloLeekRow {
            leek_id: id,
            display_name: Some(k.clone()),
            remaining: spec.quota(),
            weight: spec.weight(),
        });
    }
    Ok(rows)
}

/// Weighted pick among leeks that still have at least `min_remaining` solo budget.
fn pick_solo_leek_idx(rows: &[SoloLeekRow], min_remaining: u32) -> Option<usize> {
    let cand: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.remaining >= min_remaining)
        .map(|(i, _)| i)
        .collect();
    if cand.is_empty() {
        return None;
    }
    let weights: Vec<f64> = cand.iter().map(|&i| rows[i].weight).collect();
    let sum: f64 = weights.iter().sum();
    if sum <= 0.0 {
        let j = rand::thread_rng().gen_range(0..cand.len());
        return Some(cand[j]);
    }
    let mut r = rand::thread_rng().gen_range(0.0..1.0) * sum;
    let mut last = cand[0];
    for &i in &cand {
        let w = rows[i].weight;
        last = i;
        r -= w;
        if r <= 0.0 {
            return Some(i);
        }
    }
    Some(last)
}

async fn run_one_account(
    plan: &BatchPlan,
    acc: &AccountPlan,
    client: &mut LeekWarsClient,
    stats: &mut BatchStats,
    fight_n: &mut u32,
    opts: BatchRunOptions,
    styles: &Styles,
    log: &BatchLog,
    cancel: &AtomicBool,
) -> anyhow::Result<BatchRunSummary> {
    let session = retry_api_call(plan, "farmer_get_from_token", || client.farmer_get_from_token()).await?;
    let farmer = session
        .get("farmer")
        .ok_or_else(|| anyhow::anyhow!("session has no farmer"))?;
    let our_farmer_id = farmer["id"].as_i64().context("farmer.id")?;
    let farmer_name = farmer
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("?");
    log.line(format!(
        "login  farmer_id={our_farmer_id}  name={farmer_name}  profile={}",
        acc.profile.as_deref().unwrap_or("(default)")
    ));

    let solo_leeks = resolve_solo_leeks(farmer, acc)?;
    let mut solo_rows: Option<Vec<SoloLeekRow>> = if acc.solo_leeks.is_empty() {
        None
    } else {
        Some(init_solo_rows(acc, &solo_leeks)?)
    };
    let solo_rr: Vec<(i64, Option<String>)> = if acc.solo_leeks.is_empty() {
        build_solo_round_robin(acc, &solo_leeks)
    } else {
        Vec::new()
    };
    if acc.quota.solo > 0 && solo_rr.is_empty() && solo_rows.is_none() {
        anyhow::bail!("quota.solo > 0 but no solo leeks (solo_leek_ids / solo_leek_names under [quota])");
    }
    if acc.quota.solo > 0
        && solo_rows
            .as_ref()
            .is_some_and(|r| r.is_empty())
    {
        anyhow::bail!("quota.solo > 0 but solo_leeks resolved to no leeks");
    }
    let mut solo_ix: usize = 0;

    let mut q = QuotaLeft::from_config(&acc.quota);
    let mut summary = BatchRunSummary::default();
    let cost = &plan.quota_cost;
    log.line(format!(
        "plan   quota  farmer={} solo={} team={}  weights  F={:.2} S={:.2} T={:.2}  delay {}s + {}s jitter",
        acc.quota.farmer,
        acc.quota.solo,
        acc.quota.team,
        acc.weights.farmer,
        acc.weights.solo,
        acc.weights.team,
        plan.delay_secs,
        plan.delay_jitter_secs
    ));
    if !acc.solo_leeks.is_empty() {
        log.verbose(
            opts.verbose,
            format!(
                "       solo_leeks: {}",
                acc.solo_leeks
                    .iter()
                    .map(|(n, s)| format!("{n}→q{} w{:.2}", s.quota(), s.weight()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    if plan.strategy == Strategy::Smart && acc.opponent_bias.upset_preference > 0.0 {
        log.line(format!(
            "smart  opponent_bias  upset_preference={}  talent_scale={}  (score = win_rate + k*gap/(scale+gap), gap=max(0, enemy_tal-ours))",
            acc.opponent_bias.upset_preference,
            acc.opponent_bias.talent_scale
        ));
    }

    while can_afford_any(&q, cost) {
        if cancel.load(Ordering::SeqCst) {
            summary.interrupted = true;
            log.line("interrupt  stopping (Ctrl+C)");
            break;
        }
        if let Some(max) = plan.max_fights {
            if *fight_n >= max {
                break;
            }
        }

        let Some(kind) = pick_next_kind(&q, &acc.weights, cost) else {
            break;
        };
        log.verbose(
            opts.verbose,
            format!(
                "pick   kind={kind:?}  left  F={} S={} T={}",
                q.farmer, q.solo, q.team
            ),
        );

        let _garden = retry_api_call(plan, "garden_get", || client.garden_get()).await?;

        let mut picked_solo_row: Option<usize> = None;
        let outcome = match kind {
            FightKind::Farmer => {
                let targets = farmer_opponents_retrying(plan, client, log).await?;
                let (target_id, key, talent, opp_row) = pick_farmer(
                    our_farmer_id,
                    &targets,
                    stats,
                    plan.strategy,
                    farmer_session_talent(farmer),
                    &acc.opponent_bias,
                    log,
                )?;
                let opp = extract_opponent_log_info(&opp_row);
                let us_tal = farmer_session_talent(farmer);
                let expected_win_rate = stats.historical_win_rate_if_known(&key);
                let exp_note = expected_win_rate
                    .map(|p| format!("  E(win)~{:.1}%", p * 100.0))
                    .unwrap_or_default();
                let start =
                    retry_api_call(plan, "garden_start_farmer_fight", || {
                        client.garden_start_farmer_fight(target_id)
                    })
                    .await?;
                let fight_id = extract_fight_id(&start).context("farmer start: no fight id")?;
                log.line(format!(
                    "fight  id={fight_id}  farmer_vs_farmer  we={} (tal {})  target_id={}  {}{exp_note}  waiting…",
                    farmer_name,
                    us_tal,
                    target_id,
                    format_opponent_for_log(&opp, Some(us_tal)),
                ));
                let fight = wait_fight_finished(plan, client, fight_id, log).await?;
                let won = did_we_win(&fight, our_farmer_id)?;
                stats.record(&key, won);
                FightOutcome {
                    profile: acc.profile.clone(),
                    our_farmer_id,
                    kind: "farmer".into(),
                    target_id,
                    target_key: key,
                    fight_id,
                    won,
                    enemy_talent: talent,
                    our_leek_id: None,
                    solo_leek_name: None,
                    quota_cost_applied: cost.farmer,
                    expected_win_rate,
                }
            }
            FightKind::Solo => {
                let (leek_id, leek_name) = if let Some(ref rows) = solo_rows {
                    let ix = pick_solo_leek_idx(rows, cost.solo).ok_or_else(|| {
                        anyhow::anyhow!("no solo leek with enough remaining quota (try lowering quota_cost.solo)")
                    })?;
                    picked_solo_row = Some(ix);
                    let row = &rows[ix];
                    (row.leek_id, row.display_name.clone())
                } else {
                    let (a, b) = solo_rr[solo_ix % solo_rr.len()].clone();
                    solo_ix += 1;
                    (a, b)
                };
                let (our_ln, our_lt) = our_leek_name_talent(farmer, leek_id)
                    .unwrap_or_else(|| ("?".into(), 0));
                let targets = solo_opponents_retrying(
                    plan,
                    client,
                    leek_id,
                    our_ln.as_str(),
                    log,
                )
                .await?;
                let (target_id, key, talent, opp_row) = pick_solo(
                    our_farmer_id,
                    &targets,
                    stats,
                    plan.strategy,
                    leek_id,
                    our_lt,
                    &acc.opponent_bias,
                    log,
                )?;
                let opp = extract_opponent_log_info(&opp_row);
                let expected_win_rate = stats.historical_win_rate_if_known(&key);
                let exp_note = expected_win_rate
                    .map(|p| format!("  E(win)~{:.1}%", p * 100.0))
                    .unwrap_or_default();
                let start = retry_api_call(plan, "garden_start_solo_fight", || {
                    client.garden_start_solo_fight(leek_id, target_id)
                })
                .await?;
                let fight_id = extract_fight_id(&start).context("solo start: no fight id")?;
                log.line(format!(
                    "fight  id={fight_id}  solo  us_leek={our_ln} (id {leek_id}, tal {our_lt})  enemy_leek_id={target_id}  {}{exp_note}  waiting…",
                    format_opponent_for_log(&opp, Some(our_lt)),
                ));
                let fight = wait_fight_finished(plan, client, fight_id, log).await?;
                let won = did_we_win(&fight, our_farmer_id)?;
                stats.record(&key, won);
                FightOutcome {
                    profile: acc.profile.clone(),
                    our_farmer_id,
                    kind: "solo".into(),
                    target_id,
                    target_key: key,
                    fight_id,
                    won,
                    enemy_talent: talent,
                    our_leek_id: Some(leek_id),
                    solo_leek_name: leek_name,
                    quota_cost_applied: cost.solo,
                    expected_win_rate,
                }
            }
            FightKind::Team => {
                let comp_id = acc.team_composition_id.context("team_composition_id")?;
                let targets = team_opponents_retrying(plan, client, comp_id, log).await?;
                let (target_id, key, talent, opp_row) = pick_team(
                    our_farmer_id,
                    &targets,
                    stats,
                    plan.strategy,
                    comp_id,
                    farmer_session_talent(farmer),
                    &acc.opponent_bias,
                    log,
                )?;
                let opp = extract_opponent_log_info(&opp_row);
                let expected_win_rate = stats.historical_win_rate_if_known(&key);
                let exp_note = expected_win_rate
                    .map(|p| format!("  E(win)~{:.1}%", p * 100.0))
                    .unwrap_or_default();
                let start = retry_api_call(plan, "garden_start_team_fight", || {
                    client.garden_start_team_fight(comp_id, target_id)
                })
                .await?;
                let fight_id = extract_fight_id(&start).context("team start: no fight id")?;
                log.line(format!(
                    "fight  id={fight_id}  team  our_comp={comp_id}  enemy_comp={target_id}  {}{exp_note}  waiting…",
                    format_opponent_for_log(&opp, None),
                ));
                let fight = wait_fight_finished(plan, client, fight_id, log).await?;
                let won = did_we_win(&fight, our_farmer_id)?;
                stats.record(&key, won);
                FightOutcome {
                    profile: acc.profile.clone(),
                    our_farmer_id,
                    kind: "team".into(),
                    target_id,
                    target_key: key,
                    fight_id,
                    won,
                    enemy_talent: talent,
                    our_leek_id: None,
                    solo_leek_name: None,
                    quota_cost_applied: cost.team,
                    expected_win_rate,
                }
            }
        };

        summary.by_kind.record(outcome.kind.as_str(), outcome.won);
        append_log_line(&plan.log_path, &outcome)?;
        summary.fights.push(outcome);
        *fight_n += 1;
        apply_cost(&mut q, kind, cost);
        if kind == FightKind::Solo {
            if let (Some(rows), Some(ix)) = (&mut solo_rows, picked_solo_row) {
                rows[ix].remaining = rows[ix].remaining.saturating_sub(cost.solo);
            }
        }

        if opts.show_fight_lines() {
            let last = summary
                .fights
                .last()
                .expect("just pushed");
            emit_fight_line(styles, opts, *fight_n, plan.max_fights, last, &q)?;
        }

        stats.save(&plan.stats_path)?;

        if can_afford_any(&q, cost) {
            if let Some(max) = plan.max_fights {
                if *fight_n >= max {
                    break;
                }
            }
            let extra = if plan.delay_jitter_secs > 0 {
                rand::thread_rng().gen_range(0..=plan.delay_jitter_secs)
            } else {
                0
            };
            let secs = plan.delay_secs.saturating_add(extra);
            log.line(format!(
                "sleep  {secs}s before next fight (base {}s + jitter {}{})",
                plan.delay_secs,
                extra,
                if plan.delay_jitter_secs > 0 {
                    format!(" / max {}", plan.delay_jitter_secs)
                } else {
                    String::new()
                }
            ));
            sleep_between_fights_secs(cancel, secs).await;
        }
    }

    let wins = summary.fights.iter().filter(|f| f.won).count();
    summary.wins = wins;
    summary.losses = summary.fights.len() - wins;
    log.line(format!(
        "done   account profile={}  fights={}  wins={}  losses={}",
        acc.profile.as_deref().unwrap_or("(default)"),
        summary.fights.len(),
        wins,
        summary.losses
    ));

    Ok(summary)
}

/// Explicit `solo_leek_ids` first, then each `solo_leek_names` resolved in order.
fn build_solo_round_robin(
    acc: &AccountPlan,
    resolved: &HashMap<String, i64>,
) -> Vec<(i64, Option<String>)> {
    let mut out: Vec<(i64, Option<String>)> = acc
        .solo_leek_ids
        .iter()
        .copied()
        .map(|id| (id, None))
        .collect();
    for name in &acc.solo_leek_names {
        let norm = name.trim().to_lowercase();
        if let Some(&id) = resolved.get(&norm) {
            out.push((id, Some(name.clone())));
        }
    }
    out
}

fn resolve_solo_leeks(
    farmer: &Value,
    acc: &AccountPlan,
) -> anyhow::Result<HashMap<String, i64>> {
    let mut by_lower_name: HashMap<String, i64> = HashMap::new();
    for leek in iter_farmer_leeks(farmer) {
        let id = leek.get("id").and_then(|x| x.as_i64());
        let name = leek.get("name").and_then(|x| x.as_str());
        if let (Some(id), Some(name)) = (id, name) {
            by_lower_name.insert(name.to_lowercase(), id);
        }
    }

    if acc.quota.solo == 0 {
        return Ok(by_lower_name);
    }

    for name in &acc.solo_leek_names {
        let k = name.trim().to_lowercase();
        if !by_lower_name.contains_key(&k) {
            anyhow::bail!(
                "solo leek name not found on this farmer: {name:?} (have: {})",
                by_lower_name.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }

    Ok(by_lower_name)
}

fn iter_farmer_leeks(farmer: &Value) -> Vec<Value> {
    let Some(leeks) = farmer.get("leeks") else {
        return Vec::new();
    };
    match leeks {
        Value::Array(a) => a.clone(),
        Value::Object(m) => m.values().cloned().collect(),
        _ => Vec::new(),
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct KindPair {
    pub wins: usize,
    pub losses: usize,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct ByKindSummary {
    pub farmer: KindPair,
    pub solo: KindPair,
    pub team: KindPair,
}

impl ByKindSummary {
    fn record(&mut self, kind: &str, won: bool) {
        let p = match kind {
            "farmer" => &mut self.farmer,
            "solo" => &mut self.solo,
            "team" => &mut self.team,
            _ => return,
        };
        if won {
            p.wins += 1;
        } else {
            p.losses += 1;
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.farmer.wins += other.farmer.wins;
        self.farmer.losses += other.farmer.losses;
        self.solo.wins += other.solo.wins;
        self.solo.losses += other.solo.losses;
        self.team.wins += other.team.wins;
        self.team.losses += other.team.losses;
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct BatchRunSummary {
    pub fights: Vec<FightOutcome>,
    pub wins: usize,
    pub losses: usize,
    #[serde(default)]
    pub by_kind: ByKindSummary,
    #[serde(default)]
    pub elapsed_secs: f64,
    /// Set when the run stopped early because Ctrl+C was pressed (between-fight boundary).
    #[serde(default)]
    pub interrupted: bool,
}

#[derive(Debug, Serialize)]
pub struct FightOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub our_farmer_id: i64,
    pub kind: String,
    pub target_id: i64,
    pub target_key: String,
    pub fight_id: i64,
    pub won: bool,
    pub enemy_talent: i64,
    pub our_leek_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solo_leek_name: Option<String>,
    pub quota_cost_applied: u32,
    /// Laplace P(win) from stats **before** this fight, if we had ≥1 prior game vs `target_key`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_win_rate: Option<f64>,
}

async fn farmer_opponents(plan: &BatchPlan, client: &mut LeekWarsClient) -> anyhow::Result<Vec<Value>> {
    let v = retry_api_call(plan, "garden_get_farmer_opponents", || {
        client.garden_get_farmer_opponents()
    })
    .await?;
    Ok(opponent_array(&v))
}

async fn farmer_opponents_retrying(
    plan: &BatchPlan,
    client: &mut LeekWarsClient,
    log: &BatchLog,
) -> anyhow::Result<Vec<Value>> {
    let mut last = Vec::new();
    for attempt in 1..=GARDEN_EMPTY_RETRIES {
        last = farmer_opponents(plan, client).await?;
        if !last.is_empty() {
            if attempt > 1 {
                log.line(format!(
                    "garden  farmer opponents available on attempt {attempt}/{}",
                    GARDEN_EMPTY_RETRIES
                ));
            }
            return Ok(last);
        }
        if attempt < GARDEN_EMPTY_RETRIES {
            log.line(format!(
                "garden  no farmer opponents (API returned empty list)  attempt {attempt}/{}  sleeping {}s…",
                GARDEN_EMPTY_RETRIES, GARDEN_EMPTY_RETRY_SECS
            ));
            tokio::time::sleep(Duration::from_secs(GARDEN_EMPTY_RETRY_SECS)).await;
        }
    }
    if last.is_empty() {
        log.line(format!(
            "garden  still no farmer opponents after {} attempts — cannot pick a target",
            GARDEN_EMPTY_RETRIES
        ));
    }
    Ok(last)
}

async fn solo_opponents_retrying(
    plan: &BatchPlan,
    client: &mut LeekWarsClient,
    leek_id: i64,
    leek_label: &str,
    log: &BatchLog,
) -> anyhow::Result<Vec<Value>> {
    let mut last = Vec::new();
    for attempt in 1..=GARDEN_EMPTY_RETRIES {
        let v = retry_api_call(plan, "garden_get_leek_opponents", || {
            client.garden_get_leek_opponents(leek_id)
        })
        .await?;
        last = opponent_array(&v);
        if !last.is_empty() {
            if attempt > 1 {
                log.line(format!(
                    "garden  solo opponents for {leek_label} (leek {leek_id}) OK on attempt {attempt}/{}",
                    GARDEN_EMPTY_RETRIES
                ));
            }
            return Ok(last);
        }
        if attempt < GARDEN_EMPTY_RETRIES {
            log.line(format!(
                "garden  no solo opponents for {leek_label} (leek {leek_id})  attempt {attempt}/{}  sleeping {}s…",
                GARDEN_EMPTY_RETRIES, GARDEN_EMPTY_RETRY_SECS
            ));
            tokio::time::sleep(Duration::from_secs(GARDEN_EMPTY_RETRY_SECS)).await;
        }
    }
    if last.is_empty() {
        log.line(format!(
            "garden  still no solo opponents for {leek_label} (leek {leek_id}) after {} attempts",
            GARDEN_EMPTY_RETRIES
        ));
    }
    Ok(last)
}

async fn team_opponents_retrying(
    plan: &BatchPlan,
    client: &mut LeekWarsClient,
    comp_id: i64,
    log: &BatchLog,
) -> anyhow::Result<Vec<Value>> {
    let mut last = Vec::new();
    for attempt in 1..=GARDEN_EMPTY_RETRIES {
        let v = retry_api_call(plan, "garden_get_composition_opponents", || {
            client.garden_get_composition_opponents(comp_id)
        })
        .await?;
        last = opponent_array(&v);
        if !last.is_empty() {
            if attempt > 1 {
                log.line(format!(
                    "garden  team opponents for comp {comp_id} OK on attempt {attempt}/{}",
                    GARDEN_EMPTY_RETRIES
                ));
            }
            return Ok(last);
        }
        if attempt < GARDEN_EMPTY_RETRIES {
            log.line(format!(
                "garden  no team opponents for composition {comp_id}  attempt {attempt}/{}  sleeping {}s…",
                GARDEN_EMPTY_RETRIES, GARDEN_EMPTY_RETRY_SECS
            ));
            tokio::time::sleep(Duration::from_secs(GARDEN_EMPTY_RETRY_SECS)).await;
        }
    }
    if last.is_empty() {
        log.line(format!(
            "garden  still no team opponents for composition {comp_id} after {} attempts",
            GARDEN_EMPTY_RETRIES
        ));
    }
    Ok(last)
}

/// Every row has at least one recorded game, and every Laplace win rate is below [`SMART_FALLBACK_MAX_WIN_RATE`].
fn smart_fallback_all_tracked_weak(
    stats: &BatchStats,
    rows: &[&Value],
    key_for: impl Fn(i64) -> String,
) -> bool {
    if rows.len() < 2 {
        return false;
    }
    for r in rows {
        let Some(id) = id_of(r) else {
            return false;
        };
        let key = key_for(id);
        let wl = stats.opponents.get(&key).copied().unwrap_or_default();
        if wl.wins + wl.losses == 0 {
            return false;
        }
        if stats.win_rate(&key) >= SMART_FALLBACK_MAX_WIN_RATE {
            return false;
        }
    }
    true
}

fn opponent_array(v: &Value) -> Vec<Value> {
    if let Some(a) = v.as_array() {
        return a.clone();
    }
    if let Some(a) = v.get("opponents").and_then(|x| x.as_array()) {
        return a.clone();
    }
    if let Some(a) = v.get("farmers").and_then(|x| x.as_array()) {
        return a.clone();
    }
    Vec::new()
}

fn dedup_opponents(rows: &[Value]) -> Vec<&Value> {
    let mut seen = HashSet::new();
    rows.iter()
        .filter(|r| {
            let Some(id) = id_of(r) else {
                return false;
            };
            seen.insert(id)
        })
        .collect()
}

fn talent_of(v: &Value) -> i64 {
    v.get("talent").and_then(|x| x.as_i64()).unwrap_or(0)
}

fn id_of(v: &Value) -> Option<i64> {
    v.get("id").and_then(|x| x.as_i64())
}

/// Snapshot of an opponent row from the garden API (solo leek, farmer, or team composition).
#[derive(Debug, Clone)]
struct OpponentLogInfo {
    name: String,
    farmer_name: Option<String>,
    talent: i64,
    level: Option<i64>,
}

fn extract_opponent_log_info(v: &Value) -> OpponentLogInfo {
    let talent = talent_of(v);
    let level = v.get("level").and_then(|x| x.as_i64());
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("?")
        .to_string();
    let farmer_name = v.get("farmer").and_then(|f| match f {
        Value::Object(o) => o.get("name").and_then(|x| x.as_str()).map(String::from),
        _ => None,
    });
    OpponentLogInfo {
        name,
        farmer_name,
        talent,
        level,
    }
}

fn our_leek_name_talent(farmer: &Value, leek_id: i64) -> Option<(String, i64)> {
    for leek in iter_farmer_leeks(farmer) {
        if leek.get("id").and_then(|x| x.as_i64()) == Some(leek_id) {
            let n = leek
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("?")
                .to_string();
            return Some((n, talent_of(&leek)));
        }
    }
    None
}

fn farmer_session_talent(farmer: &Value) -> i64 {
    farmer.get("talent").and_then(|x| x.as_i64()).unwrap_or(0)
}

/// Human-readable opponent + optional Δtalent (theirs − ours).
fn format_opponent_for_log(opp: &OpponentLogInfo, our_talent: Option<i64>) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(if let Some(f) = &opp.farmer_name {
        if f == &opp.name {
            format!("opp_farmer={}", opp.name)
        } else {
            format!("opp_leek={}  opp_farmer={}", opp.name, f)
        }
    } else {
        format!("opp={}", opp.name)
    });
    if let Some(lv) = opp.level {
        parts.push(format!("lv={lv}"));
    }
    parts.push(format!("tal={}", opp.talent));
    if let Some(us) = our_talent {
        let d = opp.talent - us;
        parts.push(format!("Δtal{:+} (ours {})", d, us));
    }
    parts.join("  ")
}

fn pick_farmer(
    our_farmer_id: i64,
    rows: &[Value],
    stats: &BatchStats,
    strategy: Strategy,
    our_talent: i64,
    bias: &OpponentBias,
    log: &BatchLog,
) -> anyhow::Result<(i64, String, i64, Value)> {
    let rows = dedup_opponents(rows);
    if rows.is_empty() {
        anyhow::bail!("no farmer opponents in garden after retries (empty list)");
    }
    let strategy = match strategy {
        Strategy::Random => Strategy::Random,
        Strategy::Smart => {
            if smart_fallback_all_tracked_weak(stats, &rows, |tid| key_farmer(our_farmer_id, tid)) {
                log.line(format!(
                    "pick    farmer  smart→random  all {} opponents have stats and every estimate is below {:.0}% win — choosing uniformly",
                    rows.len(),
                    SMART_FALLBACK_MAX_WIN_RATE * 100.0
                ));
                Strategy::Random
            } else {
                Strategy::Smart
            }
        }
    };
    let choice = match strategy {
        Strategy::Random => rows[rand::thread_rng().gen_range(0..rows.len())],
        Strategy::Smart => *rows
            .iter()
            .max_by(|a, b| {
                let id_a = id_of(a).unwrap();
                let id_b = id_of(b).unwrap();
                let ka = key_farmer(our_farmer_id, id_a);
                let kb = key_farmer(our_farmer_id, id_b);
                let ta = talent_of(a);
                let tb = talent_of(b);
                let sa = bias.smart_score(
                    stats.win_rate_with_min_samples(&ka, bias.min_samples),
                    ta,
                    our_talent,
                );
                let sb = bias.smart_score(
                    stats.win_rate_with_min_samples(&kb, bias.min_samples),
                    tb,
                    our_talent,
                );
                sa.total_cmp(&sb).then_with(|| tb.cmp(&ta))
            })
            .unwrap(),
    };
    let id = id_of(choice).unwrap();
    let key = key_farmer(our_farmer_id, id);
    Ok((id, key, talent_of(choice), (*choice).clone()))
}

fn pick_solo(
    our_farmer_id: i64,
    rows: &[Value],
    stats: &BatchStats,
    strategy: Strategy,
    leek_id: i64,
    our_leek_talent: i64,
    bias: &OpponentBias,
    log: &BatchLog,
) -> anyhow::Result<(i64, String, i64, Value)> {
    let rows = dedup_opponents(rows);
    if rows.is_empty() {
        anyhow::bail!("no solo opponents for this leek after retries (empty list)");
    }
    let strategy = match strategy {
        Strategy::Random => Strategy::Random,
        Strategy::Smart => {
            if smart_fallback_all_tracked_weak(stats, &rows, |tid| {
                key_solo(our_farmer_id, leek_id, tid)
            }) {
                log.line(format!(
                    "pick    solo  smart→random  all {} opponents have stats and every estimate is below {:.0}% win — choosing uniformly (leek {leek_id})",
                    rows.len(),
                    SMART_FALLBACK_MAX_WIN_RATE * 100.0
                ));
                Strategy::Random
            } else {
                Strategy::Smart
            }
        }
    };
    let choice = match strategy {
        Strategy::Random => rows[rand::thread_rng().gen_range(0..rows.len())],
        Strategy::Smart => *rows
            .iter()
            .max_by(|a, b| {
                let id_a = id_of(a).unwrap();
                let id_b = id_of(b).unwrap();
                let ka = key_solo(our_farmer_id, leek_id, id_a);
                let kb = key_solo(our_farmer_id, leek_id, id_b);
                let ta = talent_of(a);
                let tb = talent_of(b);
                let sa = bias.smart_score(
                    stats.win_rate_with_min_samples(&ka, bias.min_samples),
                    ta,
                    our_leek_talent,
                );
                let sb = bias.smart_score(
                    stats.win_rate_with_min_samples(&kb, bias.min_samples),
                    tb,
                    our_leek_talent,
                );
                sa.total_cmp(&sb).then_with(|| tb.cmp(&ta))
            })
            .unwrap(),
    };
    let id = id_of(choice).unwrap();
    let key = key_solo(our_farmer_id, leek_id, id);
    Ok((id, key, talent_of(choice), (*choice).clone()))
}

fn pick_team(
    our_farmer_id: i64,
    rows: &[Value],
    stats: &BatchStats,
    strategy: Strategy,
    comp_id: i64,
    our_talent_hint: i64,
    bias: &OpponentBias,
    log: &BatchLog,
) -> anyhow::Result<(i64, String, i64, Value)> {
    let rows = dedup_opponents(rows);
    if rows.is_empty() {
        anyhow::bail!("no team composition opponents after retries (empty list)");
    }
    let strategy = match strategy {
        Strategy::Random => Strategy::Random,
        Strategy::Smart => {
            if smart_fallback_all_tracked_weak(stats, &rows, |tid| {
                key_team(our_farmer_id, comp_id, tid)
            }) {
                log.line(format!(
                    "pick    team  smart→random  all {} opponents have stats and every estimate is below {:.0}% win — choosing uniformly (comp {comp_id})",
                    rows.len(),
                    SMART_FALLBACK_MAX_WIN_RATE * 100.0
                ));
                Strategy::Random
            } else {
                Strategy::Smart
            }
        }
    };
    let choice = match strategy {
        Strategy::Random => rows[rand::thread_rng().gen_range(0..rows.len())],
        Strategy::Smart => *rows
            .iter()
            .max_by(|a, b| {
                let id_a = id_of(a).unwrap();
                let id_b = id_of(b).unwrap();
                let ka = key_team(our_farmer_id, comp_id, id_a);
                let kb = key_team(our_farmer_id, comp_id, id_b);
                let ta = talent_of(a);
                let tb = talent_of(b);
                let sa = bias.smart_score(
                    stats.win_rate_with_min_samples(&ka, bias.min_samples),
                    ta,
                    our_talent_hint,
                );
                let sb = bias.smart_score(
                    stats.win_rate_with_min_samples(&kb, bias.min_samples),
                    tb,
                    our_talent_hint,
                );
                sa.total_cmp(&sb).then_with(|| tb.cmp(&ta))
            })
            .unwrap(),
    };
    let id = id_of(choice).unwrap();
    let key = key_team(our_farmer_id, comp_id, id);
    Ok((id, key, talent_of(choice), (*choice).clone()))
}

fn extract_fight_id(v: &Value) -> Option<i64> {
    v.get("fight")
        .and_then(|x| x.as_i64())
        .or_else(|| v.get("fight_id").and_then(|x| x.as_i64()))
}

async fn wait_fight_finished(
    plan: &BatchPlan,
    client: &LeekWarsClient,
    fight_id: i64,
    log: &BatchLog,
) -> anyhow::Result<Value> {
    let poll_start = Instant::now();
    let max_wait = Duration::from_secs(plan.fight_wait_max_secs.max(1));

    let base_ms = plan.fight_poll_interval_ms.max(250);
    let max_gap_ms = base_ms.saturating_mul(12).min(25_000).max(base_ms);

    let mut gap_ms = base_ms;
    let mut next_log_at = Duration::from_secs(30);

    loop {
        if poll_start.elapsed() >= max_wait {
            anyhow::bail!(
                "fight {fight_id} timed out waiting for winner (max {}s)",
                plan.fight_wait_max_secs
            );
        }

        let v = retry_api_call(plan, "fight_get", || client.fight_get(fight_id)).await?;
        if let Some(w) = v.get("winner").and_then(|x| x.as_i64()) {
            if (1..=2).contains(&w) {
                return Ok(v);
            }
        }

        let elapsed = poll_start.elapsed();
        if elapsed >= next_log_at {
            log.line(format!(
                "fight  id={fight_id}  still waiting…  ~{:.0}s / {}s max  (~{:.1}s until next fight_get)",
                elapsed.as_secs_f64(),
                plan.fight_wait_max_secs,
                (gap_ms as f64) / 1000.0
            ));
            next_log_at = elapsed + Duration::from_secs(35);
        }

        let remaining = max_wait.saturating_sub(poll_start.elapsed());
        if remaining.is_zero() {
            anyhow::bail!(
                "fight {fight_id} timed out waiting for winner (max {}s)",
                plan.fight_wait_max_secs
            );
        }

        let sleep_dur = Duration::from_millis(gap_ms).min(remaining);
        tokio::time::sleep(sleep_dur).await;

        gap_ms = (gap_ms.saturating_mul(3) / 2).min(max_gap_ms);
    }
}

fn did_we_win(fight: &Value, our_farmer_id: i64) -> anyhow::Result<bool> {
    let winner = fight
        .get("winner")
        .and_then(|x| x.as_i64())
        .context("fight has no winner")?;
    let side = our_side(fight, our_farmer_id)?;
    Ok(winner == side)
}

fn our_side(fight: &Value, our_farmer_id: i64) -> anyhow::Result<i64> {
    for (side, key) in [(1i64, "leeks1"), (2, "leeks2")] {
        if let Some(arr) = fight.get(key).and_then(|x| x.as_array()) {
            for leek in arr {
                if leek.get("farmer").and_then(|x| x.as_i64()).or_else(|| {
                    leek.get("farmer")
                        .and_then(|f| f.get("id"))
                        .and_then(|x| x.as_i64())
                }) == Some(our_farmer_id)
                {
                    return Ok(side);
                }
            }
        }
    }
    let f1 = fight.get("farmer1").and_then(|x| x.as_i64());
    let f2 = fight.get("farmer2").and_then(|x| x.as_i64());
    if f1 == Some(our_farmer_id) {
        return Ok(1);
    }
    if f2 == Some(our_farmer_id) {
        return Ok(2);
    }
    anyhow::bail!("could not determine our side (farmer {our_farmer_id})");
}

fn append_log_line(path: &Path, row: &FightOutcome) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = serde_json::json!({
        "ts": ts,
        "outcome": row,
    });
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", serde_json::to_string(&line)?)?;
    Ok(())
}
