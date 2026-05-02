//! Declarative benchmarks: scenario sweeps, AI tunable grids, cache, aggregates.
//!
//! ## Spec file (TOML)
//!
//! Example TOML: `crates/leek_wars_gen/examples/experiment_sample.toml`.
//!
//! ## Optimizers
//!
//! Use [`batch::execute_run_task`] with a programmatically built [`planner::RunTask`] to evaluate
//! candidates; read back [`metrics::RunMetrics`] from the returned JSON.
//!
//! For **stat / loadout black-box search** (coordinate steps, hill-climb, win-rate summaries), see [`optimize`].

pub mod aggregate;
pub mod batch;
pub mod bench;
pub mod cache;
pub mod const_patch;
pub mod meta;
pub mod metrics;
pub mod optimize;
pub mod planner;
pub mod spec;
pub mod trace_summarize;

pub use batch::{execute_run_task, run_experiment, ExecuteRunOutput, Manifest, RunRecord};
pub use meta::load_meta_snapshot;
pub use metrics::RunMetrics;
pub use optimize::{
    add_entity_stat, best_to_record, coordinate_search_stats, fitness_team_win,
    fitness_team_win_fast, hill_climb_stats, set_entity_loadout, win_rate_summary,
    CoordinateSearchConfig, HillClimbConfig, STAT_FIELDS,
};
pub use planner::{plan_experiment, RunTask};
pub use spec::ExperimentSpec;
pub use trace_summarize::{summarize_trace_events, summarize_trace_file, TraceSummary};

pub use bench::{
    apply_all_ai_override, apply_team0_ai_override, apply_team1_ai_override, apply_team_ai_override,
    build_pvp_scenario_value, farmer_inner, fetch_side_row, list_farmer_leeks, list_team_compositions,
    print_pvp_summary, run_pvp_batch, BenchFightRecord, BenchSide, PvpBenchParams,
};
