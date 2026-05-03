//! Multi-page ranking fetches and shared HTTP agent defaults.

use std::thread;
use std::time::Duration;

use ureq::Agent;

use crate::api::{fetch_ranking_page, ApiError, RankingPageParams, RetryPolicy};

pub const ROWS_PER_PAGE: usize = 50;

#[must_use]
pub fn pages_needed(top: usize) -> u32 {
    top.div_ceil(ROWS_PER_PAGE) as u32
}

/// Default ureq agent (timeouts suitable for Leek Wars API).
#[must_use]
pub fn meta_agent() -> Agent {
    ureq::AgentBuilder::new()
        .timeout_read(Duration::from_mins(1))
        .timeout_write(Duration::from_mins(1))
        .build()
}

pub struct RankingRowsJob<'a> {
    pub api_base: &'a str,
    pub active_only: bool,
    pub category: &'a str,
    pub order: &'a str,
    pub top: usize,
    pub country: Option<&'a str>,
    pub retry: &'a RetryPolicy,
    pub gap: Duration,
}

pub fn fetch_ranking_rows(
    agent: &Agent,
    job: &RankingRowsJob<'_>,
) -> Result<(Vec<serde_json::Value>, u32, u32), ApiError> {
    let mut all = Vec::new();
    let mut pages_total = 0u32;
    let mut total_entities = 0u32;
    let max_page = pages_needed(job.top);
    for page in 1..=max_page {
        if page > 1 && !job.gap.is_zero() {
            thread::sleep(job.gap);
        }
        let params = RankingPageParams {
            base: job.api_base,
            active_only: job.active_only,
            category: job.category,
            order: job.order,
            page,
            country: job.country,
        };
        let resp = fetch_ranking_page(agent, &params, job.retry)?;
        if page == 1 {
            pages_total = resp.pages;
            total_entities = resp.total;
        }
        let page_len = resp.ranking.len();
        all.extend(resp.ranking);
        if all.len() >= job.top {
            all.truncate(job.top);
            break;
        }
        if page_len < ROWS_PER_PAGE {
            break;
        }
    }
    Ok((all, pages_total, total_entities))
}
