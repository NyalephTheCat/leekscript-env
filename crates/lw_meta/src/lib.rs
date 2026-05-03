//! Leek Wars ranking API client (429-aware) and typed export shapes.
//! Used by the `lw-meta` binary and by `leekgen meta rankings`.

pub mod api;
pub mod fetch;
pub mod sim_export;
pub mod types;

pub use api::{
    composition_rich_tooltip_url, farmer_get_url, fetch_composition_rich_tooltip, fetch_farmer,
    fetch_farmer_public, fetch_leek_public, fetch_ranking_page, fetch_service_catalog,
    filter_catalog, leek_get_url, ranking_url, ApiError, RankingPageParams, RetryPolicy,
    DEFAULT_API_BASE,
};
pub use fetch::{fetch_ranking_rows, meta_agent, pages_needed, RankingRowsJob, ROWS_PER_PAGE};
pub use sim_export::{
    fetch_composition_sim_bundle, leek_sim_export_body, leek_sim_profile,
    scenario_entity_from_leek_get,
};
pub use types::*;
