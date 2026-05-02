//! `leekgen meta entity` — `leek/get` and composition bundles via `lw_meta`.

use std::path::Path;
use std::time::Duration;

use leek_wars_gen::experiment::meta::write_lw_meta_snapshot;
use leek_wars_gen::GenError;
use lw_meta::{
    fetch_composition_sim_bundle, fetch_leek_public, leek_sim_export_body, meta_agent, RetryPolicy,
};

pub struct EntityHttpParams {
    pub api_base: String,
    pub max_attempts: u32,
    pub backoff_initial_ms: u64,
    pub backoff_max_ms: u64,
    pub request_gap_ms: u64,
}

fn retry_gap(http: &EntityHttpParams) -> (RetryPolicy, Duration) {
    (
        RetryPolicy::from_ms(
            http.max_attempts,
            http.backoff_initial_ms,
            http.backoff_max_ms,
        ),
        Duration::from_millis(http.request_gap_ms),
    )
}

pub fn run_leek(
    http: &EntityHttpParams,
    output: &Path,
    id: u64,
    profile_only: bool,
) -> Result<(), GenError> {
    let agent = meta_agent();
    let (retry, _) = retry_gap(http);
    let raw = fetch_leek_public(&agent, &http.api_base, id, &retry)?;
    let body = leek_sim_export_body(&raw, profile_only);
    let label = format!("lw-meta:leek/get/{id}");
    write_lw_meta_snapshot(output, &label, &body)?;
    Ok(())
}

pub fn run_composition(
    http: &EntityHttpParams,
    output: &Path,
    id: u64,
    summary_only: bool,
    profile_only: bool,
) -> Result<(), GenError> {
    let agent = meta_agent();
    let (retry, gap) = retry_gap(http);
    let bundle = fetch_composition_sim_bundle(
        &agent,
        &http.api_base,
        id,
        &retry,
        !summary_only,
        profile_only,
        gap,
    )?;
    let label = format!(
        "lw-meta:composition/{id}{}",
        if summary_only { "/summary" } else { "/full" }
    );
    write_lw_meta_snapshot(output, &label, &bundle)?;
    Ok(())
}
