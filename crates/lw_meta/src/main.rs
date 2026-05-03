//! `lw-meta` CLI — thin wrapper around the `lw_meta` library.

use std::io::{self, Write};
use std::time::Duration;

use clap::{Parser, Subcommand};
use lw_meta::{
    fetch_composition_sim_bundle, fetch_leek_public, fetch_ranking_rows, fetch_service_catalog,
    filter_catalog, leek_sim_export_body, meta_agent, CompositionRankingRow, LeekRankingRow,
    MetaExport, RankingRowsJob, RetryPolicy, ServiceCatalogExport, TeamRankingRow,
    DEFAULT_API_BASE,
};

#[derive(Parser, Debug)]
#[command(
    name = "lw-meta",
    about = "Leek Wars meta: rankings, leek/composition sheets for local fight approximation"
)]
struct Cli {
    #[arg(long, default_value = DEFAULT_API_BASE, env = "LEEKWARS_API_BASE")]
    api_base: String,

    /// Bearer token for authenticated endpoints (`service/get-all`). Same as Leek Wars web session.
    #[arg(long, env = "LEEKWARS_TOKEN")]
    token: Option<String>,

    /// Total HTTP attempts per call when the server returns 429 / 503 (exponential backoff between tries).
    #[arg(long, default_value_t = 12, env = "LEEKWARS_MAX_ATTEMPTS")]
    max_attempts: u32,

    /// First backoff delay after HTTP 429/503; doubles each retry until `--backoff-max-ms`.
    #[arg(long, default_value_t = 1000, env = "LEEKWARS_BACKOFF_INITIAL_MS")]
    backoff_initial_ms: u64,

    /// Upper bound for backoff (and cap for `Retry-After` when the server sends it).
    #[arg(long, default_value_t = 120_000, env = "LEEKWARS_BACKOFF_MAX_MS")]
    backoff_max_ms: u64,

    /// Optional pause between ranking page requests (reduces chance of 429 when scraping many pages).
    #[arg(long, default_value_t = 0, env = "LEEKWARS_REQUEST_GAP_MS")]
    request_gap_ms: u64,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Dump API service descriptors (needs `LEEKWARS_TOKEN`; lists modules like `ranking`, `team-composition`).
    Services {
        #[arg(long = "module", short = 'm')]
        modules: Vec<String>,
        #[arg(long)]
        raw: bool,
    },
    Ranking {
        #[command(subcommand)]
        kind: RankingKind,
    },
    /// `leek/get` or composition bundle (tooltip + per-leek sheets) for offline scenarios.
    Entity {
        #[command(subcommand)]
        kind: EntityKind,
    },
}

#[derive(Subcommand, Debug)]
enum EntityKind {
    /// Full public sheet (`leek/get/{id}`): stats, weapon/chip templates, components, AI metadata.
    Leek {
        id: u64,
        /// Emit only [`lw_meta::leek_sim_profile`] JSON (no raw API body).
        #[arg(long)]
        profile_only: bool,
    },
    /// Composition roster + optional `leek/get` per member for full loadouts.
    Composition {
        id: u64,
        /// Only `team/composition-rich-tooltip` (no `leek/get` calls).
        #[arg(long)]
        summary_only: bool,
        /// In bundle mode: each leek is profile-only (smaller); ignored if `--summary-only`.
        #[arg(long)]
        profile_only: bool,
    },
}

#[derive(Subcommand, Debug)]
enum RankingKind {
    Leeks {
        #[arg(long, default_value = "talent")]
        order: String,
        #[arg(long)]
        leek_level: Option<u32>,
        #[arg(long, default_value_t = 500)]
        top: usize,
        #[arg(long)]
        country: Option<String>,
        #[arg(long)]
        include_inactive: bool,
    },
    Teams {
        #[arg(long, default_value = "talent")]
        order: String,
        #[arg(long, default_value_t = 500)]
        top: usize,
        #[arg(long)]
        country: Option<String>,
        #[arg(long)]
        include_inactive: bool,
    },
    Compositions {
        #[arg(long, default_value = "talent")]
        order: String,
        #[arg(long, default_value_t = 500)]
        top: usize,
        #[arg(long)]
        country: Option<String>,
        #[arg(long)]
        include_inactive: bool,
    },
    All {
        #[arg(long, default_value = "talent")]
        order: String,
        #[arg(long)]
        leek_level: Option<u32>,
        #[arg(long, default_value_t = 500)]
        top: usize,
        #[arg(long)]
        country: Option<String>,
        #[arg(long)]
        include_inactive: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let agent = meta_agent();
    let retry = RetryPolicy::from_ms(cli.max_attempts, cli.backoff_initial_ms, cli.backoff_max_ms);
    let gap = Duration::from_millis(cli.request_gap_ms);

    match cli.command {
        Command::Services { modules, raw } => {
            let token = cli.token.ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "service/get-all requires a bearer token; set LEEKWARS_TOKEN or pass --token",
                )
            })?;
            let services = fetch_service_catalog(&agent, &cli.api_base, &token, &retry)?;
            let default_filter = ["ranking", "team", "team-composition", "leek"]
                .map(String::from)
                .to_vec();
            let filter = if modules.is_empty() {
                default_filter
            } else {
                modules
            };
            let filtered = filter_catalog(&services, &filter);
            if raw {
                serde_json::to_writer_pretty(io::stdout(), &filtered)?;
            } else {
                let export = ServiceCatalogExport {
                    source: cli.api_base.clone(),
                    services: filtered,
                    filtered_modules: filter,
                };
                serde_json::to_writer_pretty(io::stdout(), &export)?;
            }
            writeln!(io::stdout())?;
        }
        Command::Entity { kind } => match kind {
            EntityKind::Leek { id, profile_only } => {
                let raw = fetch_leek_public(&agent, &cli.api_base, id, &retry)?;
                let body = leek_sim_export_body(&raw, profile_only);
                serde_json::to_writer_pretty(io::stdout(), &body)?;
                writeln!(io::stdout())?;
            }
            EntityKind::Composition {
                id,
                summary_only,
                profile_only,
            } => {
                let bundle = fetch_composition_sim_bundle(
                    &agent,
                    &cli.api_base,
                    id,
                    &retry,
                    !summary_only,
                    profile_only,
                    gap,
                )?;
                serde_json::to_writer_pretty(io::stdout(), &bundle)?;
                writeln!(io::stdout())?;
            }
        },
        Command::Ranking { kind } => match kind {
            RankingKind::Leeks {
                order,
                leek_level,
                top,
                country,
                include_inactive,
            } => {
                let category = leek_level.map_or_else(|| "leek".into(), |l| format!("level-{l}"));
                let job = RankingRowsJob {
                    api_base: &cli.api_base,
                    active_only: !include_inactive,
                    category: category.as_str(),
                    order: &order,
                    top,
                    country: country.as_deref(),
                    retry: &retry,
                    gap,
                };
                let (rows, pages, total) = fetch_ranking_rows(&agent, &job)?;
                let leeks: Vec<LeekRankingRow> = rows
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
                let export = MetaExport {
                    source: cli.api_base.clone(),
                    category,
                    order: order.clone(),
                    active_only: !include_inactive,
                    country: country.clone(),
                    fetched_rows: leeks.len(),
                    pages,
                    total_entities: total,
                    leeks: Some(leeks),
                    teams: None,
                    compositions: None,
                };
                serde_json::to_writer_pretty(io::stdout(), &export)?;
                writeln!(io::stdout())?;
            }
            RankingKind::Teams {
                order,
                top,
                country,
                include_inactive,
            } => {
                let job = RankingRowsJob {
                    api_base: &cli.api_base,
                    active_only: !include_inactive,
                    category: "team",
                    order: &order,
                    top,
                    country: country.as_deref(),
                    retry: &retry,
                    gap,
                };
                let (rows, pages, total) = fetch_ranking_rows(&agent, &job)?;
                let teams: Vec<TeamRankingRow> = rows
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
                let export = MetaExport {
                    source: cli.api_base.clone(),
                    category: "team".into(),
                    order: order.clone(),
                    active_only: !include_inactive,
                    country: country.clone(),
                    fetched_rows: teams.len(),
                    pages,
                    total_entities: total,
                    leeks: None,
                    teams: Some(teams),
                    compositions: None,
                };
                serde_json::to_writer_pretty(io::stdout(), &export)?;
                writeln!(io::stdout())?;
            }
            RankingKind::Compositions {
                order,
                top,
                country,
                include_inactive,
            } => {
                let job = RankingRowsJob {
                    api_base: &cli.api_base,
                    active_only: !include_inactive,
                    category: "composition",
                    order: &order,
                    top,
                    country: country.as_deref(),
                    retry: &retry,
                    gap,
                };
                let (rows, pages, total) = fetch_ranking_rows(&agent, &job)?;
                let compositions: Vec<CompositionRankingRow> = rows
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
                let export = MetaExport {
                    source: cli.api_base.clone(),
                    category: "composition".into(),
                    order: order.clone(),
                    active_only: !include_inactive,
                    country: country.clone(),
                    fetched_rows: compositions.len(),
                    pages,
                    total_entities: total,
                    leeks: None,
                    teams: None,
                    compositions: Some(compositions),
                };
                serde_json::to_writer_pretty(io::stdout(), &export)?;
                writeln!(io::stdout())?;
            }
            RankingKind::All {
                order,
                leek_level,
                top,
                country,
                include_inactive,
            } => {
                let c = country.as_deref();
                let active_only = !include_inactive;
                let leek_cat = leek_level.map_or_else(|| "leek".into(), |l| format!("level-{l}"));
                let job_leeks = RankingRowsJob {
                    api_base: &cli.api_base,
                    active_only,
                    category: leek_cat.as_str(),
                    order: &order,
                    top,
                    country: c,
                    retry: &retry,
                    gap,
                };
                let (leek_v, pages_l, total_l) = fetch_ranking_rows(&agent, &job_leeks)?;
                let job_teams = RankingRowsJob {
                    api_base: &cli.api_base,
                    active_only,
                    category: "team",
                    order: &order,
                    top,
                    country: c,
                    retry: &retry,
                    gap,
                };
                let (team_v, _, total_t) = fetch_ranking_rows(&agent, &job_teams)?;
                let job_comp = RankingRowsJob {
                    api_base: &cli.api_base,
                    active_only,
                    category: "composition",
                    order: &order,
                    top,
                    country: c,
                    retry: &retry,
                    gap,
                };
                let (comp_v, _, total_c) = fetch_ranking_rows(&agent, &job_comp)?;
                let leeks: Vec<LeekRankingRow> = leek_v
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
                let teams: Vec<TeamRankingRow> = team_v
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
                let compositions: Vec<CompositionRankingRow> = comp_v
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
                let export = serde_json::json!({
                    "source": cli.api_base,
                    "order": order,
                    "leek_category": leek_cat,
                    "active_only": active_only,
                    "country": country,
                    "top_requested": top,
                    "leeks": {
                        "fetched_rows": leeks.len(),
                        "pages": pages_l,
                        "total_entities": total_l,
                        "ranking": leeks,
                    },
                    "teams": {
                        "fetched_rows": teams.len(),
                        "total_entities": total_t,
                        "ranking": teams,
                    },
                    "compositions": {
                        "fetched_rows": compositions.len(),
                        "total_entities": total_c,
                        "ranking": compositions,
                    },
                });
                serde_json::to_writer_pretty(io::stdout(), &export)?;
                writeln!(io::stdout())?;
            }
        },
    }

    Ok(())
}
