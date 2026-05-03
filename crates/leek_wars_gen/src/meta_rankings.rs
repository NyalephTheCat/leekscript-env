//! `leekgen meta rankings` — uses the `lw_meta` crate (same stack as `lw-meta`).

use std::path::Path;
use std::time::Duration;

use lw_meta::{
    fetch_ranking_rows, fetch_service_catalog, filter_catalog, meta_agent, CompositionRankingRow,
    LeekRankingRow, MetaExport, RankingRowsJob, RetryPolicy, ServiceCatalogExport, TeamRankingRow,
};

use leek_wars_gen::experiment::meta::write_lw_meta_snapshot;
use leek_wars_gen::GenError;

pub struct LwHttpParams {
    pub api_base: String,
    pub token: Option<String>,
    pub max_attempts: u32,
    pub backoff_initial_ms: u64,
    pub backoff_max_ms: u64,
    pub request_gap_ms: u64,
}

fn retry_gap(http: &LwHttpParams) -> (RetryPolicy, Duration) {
    (
        RetryPolicy::from_ms(
            http.max_attempts,
            http.backoff_initial_ms,
            http.backoff_max_ms,
        ),
        Duration::from_millis(http.request_gap_ms),
    )
}

pub fn run_services(
    http: &LwHttpParams,
    output: &Path,
    modules: Vec<String>,
    raw: bool,
) -> Result<(), GenError> {
    let token = http.token.as_deref().ok_or_else(|| {
        GenError::Message(
            "service/get-all requires a bearer token (LEEKWARS_TOKEN or --token)".into(),
        )
    })?;
    let agent = meta_agent();
    let (retry, _) = retry_gap(http);
    let services = fetch_service_catalog(&agent, &http.api_base, token, &retry)?;
    let default_filter = ["ranking", "team", "team-composition", "leek"]
        .map(String::from)
        .to_vec();
    let filter = if modules.is_empty() {
        default_filter
    } else {
        modules
    };
    let filtered = filter_catalog(&services, &filter);
    let body = if raw {
        serde_json::to_value(&filtered).map_err(GenError::ScenarioJson)?
    } else {
        serde_json::to_value(&ServiceCatalogExport {
            source: http.api_base.clone(),
            services: filtered,
            filtered_modules: filter,
        })
        .map_err(GenError::ScenarioJson)?
    };
    write_lw_meta_snapshot(output, "lw-meta:service/get-all", &body)?;
    Ok(())
}

pub fn run_leeks(
    http: &LwHttpParams,
    output: &Path,
    order: String,
    leek_level: Option<u32>,
    top: usize,
    country: Option<String>,
    include_inactive: bool,
) -> Result<(), GenError> {
    let agent = meta_agent();
    let (retry, gap) = retry_gap(http);
    let category = leek_level.map_or_else(|| "leek".into(), |l| format!("level-{l}"));
    let job = RankingRowsJob {
        api_base: &http.api_base,
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
        .collect::<Result<_, _>>()
        .map_err(GenError::ScenarioJson)?;
    let export = MetaExport {
        source: http.api_base.clone(),
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
    let body = serde_json::to_value(&export).map_err(GenError::ScenarioJson)?;
    let label = format!(
        "lw-meta:rankings/leeks/{}/{}/top-{}",
        if include_inactive {
            "get"
        } else {
            "get-active"
        },
        order,
        top
    );
    write_lw_meta_snapshot(output, &label, &body)?;
    Ok(())
}

pub fn run_teams(
    http: &LwHttpParams,
    output: &Path,
    order: String,
    top: usize,
    country: Option<String>,
    include_inactive: bool,
) -> Result<(), GenError> {
    let agent = meta_agent();
    let (retry, gap) = retry_gap(http);
    let job = RankingRowsJob {
        api_base: &http.api_base,
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
        .collect::<Result<_, _>>()
        .map_err(GenError::ScenarioJson)?;
    let export = MetaExport {
        source: http.api_base.clone(),
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
    let body = serde_json::to_value(&export).map_err(GenError::ScenarioJson)?;
    let label = format!(
        "lw-meta:rankings/teams/{}/{}/top-{}",
        if include_inactive {
            "get"
        } else {
            "get-active"
        },
        order,
        top
    );
    write_lw_meta_snapshot(output, &label, &body)?;
    Ok(())
}

pub fn run_compositions(
    http: &LwHttpParams,
    output: &Path,
    order: String,
    top: usize,
    country: Option<String>,
    include_inactive: bool,
) -> Result<(), GenError> {
    let agent = meta_agent();
    let (retry, gap) = retry_gap(http);
    let job = RankingRowsJob {
        api_base: &http.api_base,
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
        .collect::<Result<_, _>>()
        .map_err(GenError::ScenarioJson)?;
    let export = MetaExport {
        source: http.api_base.clone(),
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
    let body = serde_json::to_value(&export).map_err(GenError::ScenarioJson)?;
    let label = format!(
        "lw-meta:rankings/compositions/{}/{}/top-{}",
        if include_inactive {
            "get"
        } else {
            "get-active"
        },
        order,
        top
    );
    write_lw_meta_snapshot(output, &label, &body)?;
    Ok(())
}

pub fn run_all(
    http: &LwHttpParams,
    output: &Path,
    order: String,
    leek_level: Option<u32>,
    top: usize,
    country: Option<String>,
    include_inactive: bool,
) -> Result<(), GenError> {
    let agent = meta_agent();
    let (retry, gap) = retry_gap(http);
    let c = country.as_deref();
    let active_only = !include_inactive;
    let leek_cat = leek_level.map_or_else(|| "leek".into(), |l| format!("level-{l}"));

    let job_leeks = RankingRowsJob {
        api_base: &http.api_base,
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
        api_base: &http.api_base,
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
        api_base: &http.api_base,
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
        .collect::<Result<_, _>>()
        .map_err(GenError::ScenarioJson)?;
    let teams: Vec<TeamRankingRow> = team_v
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(GenError::ScenarioJson)?;
    let compositions: Vec<CompositionRankingRow> = comp_v
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(GenError::ScenarioJson)?;

    let body = serde_json::json!({
        "source": http.api_base,
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
    let label = format!(
        "lw-meta:rankings/all/{}/{}/top-{}",
        if include_inactive {
            "get"
        } else {
            "get-active"
        },
        order,
        top
    );
    write_lw_meta_snapshot(output, &label, &body)?;
    Ok(())
}
