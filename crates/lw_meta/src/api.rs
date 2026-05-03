use std::thread;
use std::time::Duration;

use crate::types::{RankingResponse, ServiceDescriptor};
use serde::de::DeserializeOwned;
use thiserror::Error;
use ureq::{Agent, Error as UreqError, OrAnyStatus, Transport as UreqTransport};

pub const DEFAULT_API_BASE: &str = "https://leekwars.com/api/";

/// HTTP 429 / 503 handling: exponential backoff, optional `Retry-After` (seconds).
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Total tries per logical request (including the first).
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 12,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_mins(2),
        }
    }
}

impl RetryPolicy {
    #[must_use]
    pub fn from_ms(max_attempts: u32, initial_ms: u64, max_ms: u64) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            initial_backoff: Duration::from_millis(initial_ms.max(1)),
            max_backoff: Duration::from_millis(max_ms.max(1)),
        }
    }
}

fn parse_retry_after_header(value: Option<&str>) -> Option<Duration> {
    let raw = value?.trim();
    let secs: u64 = raw.parse().ok()?;
    Some(Duration::from_secs(secs.min(600)))
}

fn jitter() -> Duration {
    let us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_micros());
    Duration::from_millis(u64::from(us % 350))
}

fn backoff_after_failure(policy: &RetryPolicy, failures_so_far: u32) -> Duration {
    let pow = failures_so_far.min(24);
    let mult = 1u128 << pow;
    let base = policy.initial_backoff.as_millis().saturating_mul(mult);
    let capped = base.min(policy.max_backoff.as_millis());
    Duration::from_millis(u64::try_from(capped).unwrap_or(u64::MAX)) + jitter()
}

fn should_retry_status(status: u16) -> bool {
    status == 429 || status == 503
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("transport: {0}")]
    Transport(Box<ureq::Error>),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<ureq::Error> for ApiError {
    fn from(e: ureq::Error) -> Self {
        ApiError::Transport(Box::new(e))
    }
}

fn trim_slash(base: &str) -> &str {
    base.trim_end_matches('/')
}

/// One ranking listing request (`ranking/get` / `ranking/get-active`).
#[derive(Clone, Copy, Debug)]
pub struct RankingPageParams<'a> {
    pub base: &'a str,
    pub active_only: bool,
    pub category: &'a str,
    pub order: &'a str,
    pub page: u32,
    pub country: Option<&'a str>,
}

#[must_use]
pub fn ranking_url(
    base: &str,
    active_only: bool,
    category: &str,
    order: &str,
    page: u32,
    country: Option<&str>,
) -> String {
    let svc = if active_only { "get-active" } else { "get" };
    let country_seg = country.unwrap_or("null");
    format!(
        "{}/{}/{}/{}/{}/{}/{}",
        trim_slash(base),
        "ranking",
        svc,
        category,
        order,
        page,
        country_seg
    )
}

pub fn get_json<T: DeserializeOwned>(
    agent: &Agent,
    url: &str,
    token: Option<&str>,
    retry: &RetryPolicy,
) -> Result<T, ApiError> {
    let mut failures = 0u32;
    for attempt in 1..=retry.max_attempts {
        let mut req = agent.get(url);
        if let Some(t) = token {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        // ureq returns status >= 400 as `Err(Status(..))` by default; treat those as normal
        // `Response` values so 429/503 hit our retry path.
        let resp = req
            .call()
            .or_any_status()
            .map_err(|t: UreqTransport| ApiError::Transport(Box::new(UreqError::Transport(t))))?;
        let status = resp.status();
        let retry_after = resp
            .header("Retry-After")
            .or_else(|| resp.header("retry-after"))
            .map(str::to_string);

        if should_retry_status(status) && attempt < retry.max_attempts {
            let body = resp.into_string().unwrap_or_default();
            let server_wait = parse_retry_after_header(retry_after.as_deref());
            let wait = server_wait.map_or_else(
                || backoff_after_failure(retry, failures),
                |d| d.min(retry.max_backoff) + jitter(),
            );
            failures += 1;
            eprintln!(
                "lw-meta: HTTP {} (attempt {}/{}), backing off {:?} …",
                status, attempt, retry.max_attempts, wait
            );
            if !body.is_empty() && body.len() < 256 {
                eprintln!("lw-meta: response: {}", body.trim());
            }
            thread::sleep(wait);
            continue;
        }

        let body = resp.into_string().unwrap_or_default();
        if status >= 400 {
            return Err(ApiError::Http { status, body });
        }
        return Ok(serde_json::from_str(&body)?);
    }
    unreachable!("retry.max_attempts is always ≥ 1; loop always returns")
}

pub fn fetch_ranking_page(
    agent: &Agent,
    params: &RankingPageParams<'_>,
    retry: &RetryPolicy,
) -> Result<RankingResponse, ApiError> {
    let url = ranking_url(
        params.base,
        params.active_only,
        params.category,
        params.order,
        params.page,
        params.country,
    );
    get_json(agent, &url, None, retry)
}

pub fn fetch_service_catalog(
    agent: &Agent,
    base: &str,
    token: &str,
    retry: &RetryPolicy,
) -> Result<Vec<ServiceDescriptor>, ApiError> {
    let url = format!("{}/service/get-all", trim_slash(base));
    get_json(agent, &url, Some(token), retry)
}

/// Public leek sheet (`leek/get/{id}`) — stats, chips, weapons, components, AI metadata (no source).
#[must_use]
pub fn leek_get_url(base: &str, leek_id: u64) -> String {
    format!("{}/leek/get/{}", trim_slash(base), leek_id)
}

/// Composition summary + roster stats (`team/composition-rich-tooltip/{id}`). For full loadouts, also call [`fetch_leek_public`] per leek id.
#[must_use]
pub fn composition_rich_tooltip_url(base: &str, composition_id: u64) -> String {
    format!(
        "{}/team/composition-rich-tooltip/{}",
        trim_slash(base),
        composition_id
    )
}

/// Public farmer profile (`farmer/get/{id}`) — team, compositions, leeks metadata.
#[must_use]
pub fn farmer_get_url(base: &str, farmer_id: u64) -> String {
    format!("{}/farmer/get/{}", trim_slash(base), farmer_id)
}

/// Optional bearer token (same as the website session) may return extra fields (e.g. team compositions).
pub fn fetch_farmer(
    agent: &Agent,
    base: &str,
    farmer_id: u64,
    token: Option<&str>,
    retry: &RetryPolicy,
) -> Result<serde_json::Value, ApiError> {
    get_json(agent, &farmer_get_url(base, farmer_id), token, retry)
}

pub fn fetch_farmer_public(
    agent: &Agent,
    base: &str,
    farmer_id: u64,
    retry: &RetryPolicy,
) -> Result<serde_json::Value, ApiError> {
    fetch_farmer(agent, base, farmer_id, None, retry)
}

pub fn fetch_leek_public(
    agent: &Agent,
    base: &str,
    leek_id: u64,
    retry: &RetryPolicy,
) -> Result<serde_json::Value, ApiError> {
    get_json(agent, &leek_get_url(base, leek_id), None, retry)
}

pub fn fetch_composition_rich_tooltip(
    agent: &Agent,
    base: &str,
    composition_id: u64,
    retry: &RetryPolicy,
) -> Result<serde_json::Value, ApiError> {
    get_json(
        agent,
        &composition_rich_tooltip_url(base, composition_id),
        None,
        retry,
    )
}

#[must_use]
pub fn filter_catalog(
    services: &[ServiceDescriptor],
    modules: &[String],
) -> Vec<ServiceDescriptor> {
    if modules.is_empty() {
        return services.to_vec();
    }
    services
        .iter()
        .filter(|s| modules.iter().any(|m| m == &s.module))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_header_seconds() {
        assert_eq!(
            parse_retry_after_header(Some("42")),
            Some(Duration::from_secs(42))
        );
        assert_eq!(
            parse_retry_after_header(Some("  9  ")),
            Some(Duration::from_secs(9))
        );
        assert_eq!(parse_retry_after_header(Some("nope")), None);
    }

    #[test]
    fn backoff_caps_at_max() {
        let p = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_secs(10),
            max_backoff: Duration::from_secs(15),
        };
        let w = backoff_after_failure(&p, 10);
        assert!(w >= Duration::from_secs(15));
        assert!(w <= Duration::from_millis(15_000 + 400));
    }

    #[test]
    fn ranking_url_worldwide_active() {
        assert_eq!(
            ranking_url("https://leekwars.com/api", true, "leek", "talent", 3, None),
            "https://leekwars.com/api/ranking/get-active/leek/talent/3/null"
        );
    }

    #[test]
    fn ranking_url_country_inactive() {
        assert_eq!(
            ranking_url(
                "https://leekwars.com/api/",
                false,
                "team",
                "level",
                1,
                Some("fr")
            ),
            "https://leekwars.com/api/ranking/get/team/level/1/fr"
        );
    }

    #[test]
    fn leek_and_composition_urls() {
        assert_eq!(
            leek_get_url("https://leekwars.com/api/", 42),
            "https://leekwars.com/api/leek/get/42"
        );
        assert_eq!(
            composition_rich_tooltip_url("https://leekwars.com/api", 99),
            "https://leekwars.com/api/team/composition-rich-tooltip/99"
        );
        assert_eq!(
            farmer_get_url("https://leekwars.com/api/", 76142),
            "https://leekwars.com/api/farmer/get/76142"
        );
    }
}
