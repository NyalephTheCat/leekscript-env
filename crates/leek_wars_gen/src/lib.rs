//! Leek Wars fight generation from Rust.
//!
//! - **Scenario**: JSON format accepted by the official [`generator.jar`](https://github.com/leek-wars/leek-wars-generator).
//! - **Engines**: [`RustEngine`](engine::RustEngine) runs the in-tree fight loop on [`leekscript_run`] (default `leekgen` engine).
//!   [`JavaEngine`](engine::JavaEngine) shells out to the JVM (`java -jar …`) for parity with `com.leekwars.Main` when requested.
//! - **Parity**: [`parity::normalize_outcome_json`] strips volatile timing fields so two runs can be compared.
//! - **Fight sim**: [`fight`] runs scenarios with [`leekscript_run`] + a Leek Wars [`fight::FightHost`] (simplified vs the official generator).
//! - **Experiments**: [`experiment`] — TOML sweeps, AI `const` grids, cache, NDJSON manifest (`leekgen experiment run`), plus [`experiment::optimize`] for black-box stat/loadout search (coordinate / hill-climb over fight outcomes). **`leekgen pvp`** runs multi-seed batches from live `composition:` / `leek:` API ids (progress bar + win-rate table).
//! - **Meta snapshots**: [`experiment::meta`] for URL snapshots; `leekgen meta rankings …` and `leekgen meta entity …` use [`lw_meta`] (`lw-meta` CLI).
pub mod compare_fuzz_cli;
pub mod config;
pub mod engine;
pub mod error;
pub mod experiment;
pub mod fight;
pub mod fuzz;
pub mod fuzz_input;
pub mod harness;
pub mod output;
pub mod parity;
pub mod scenario;
pub mod scenario_io;

/// Leek Wars ranking / `service/get-all` HTTP client (429 backoff). See `leekgen meta rankings`.
pub use lw_meta;

pub use engine::{JavaEngine, JavaEngineConfig, RunRequest, RustEngine};
pub use error::GenError;
pub use scenario::Scenario;

// Stable hooks for benchmarks / optimizers (see `experiment` module).
pub use experiment::{
    add_entity_stat, coordinate_search_stats, execute_run_task, fitness_team_win, hill_climb_stats,
    plan_experiment, run_experiment, set_entity_loadout, CoordinateSearchConfig, ExecuteRunOutput,
    ExperimentSpec, HillClimbConfig, Manifest, RunMetrics, RunRecord, RunTask,
};
pub use fight::{FightRunOptions, FightRunOutput, TraceConfig, TraceEvent};
