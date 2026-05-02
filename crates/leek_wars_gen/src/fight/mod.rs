//! Minimal Leek Wars fight simulation in Rust (AI via [`leekscript_run`] + [`super::fight::host::FightHost`]).
//!
//! This is **not** a full port of the official generator: map generation, pathfinding, weapons/chips data,
//! and action logs are simplified. By default, `run_scenario_path` replays official-generator `State.init`
//! (procedural map + `StartOrder`) in Rust so fight RNG `n`, entity cells, and obstacles match `DumpStateRng`
//! without `generator.jar`.

mod host;
mod java_bootstrap;
pub use java_bootstrap::compute_java_fight_bootstrap;
mod java_weight_open;
mod registry_ops;
#[doc(hidden)]
pub use java_weight_open::{compile_treeset_weight_probe_java, replay_treeset_weight_probe_polls};
mod chips;
mod effects;
pub mod map;
mod pathfinding;
mod rng;
mod run;
mod sig_globals;
pub(crate) use sig_globals::merge_signature_globals;
mod start_order;
mod summons;
pub mod trace;
mod weapons;
mod world;

pub use chips::{load_chips_json, ChipEffect, ChipStats};
pub use effects::{apply_effects_on_cells, apply_start_turn_effects, EffectContext};
pub use host::FightHost;
pub use pathfinding::{astar_path_probe_script, get_path_between};
pub use rng::{JavaCompatRng, TurnOrderRng};
pub use run::{
    run_scenario_path, run_scenario_path_with_ai_overlay, run_scenario_path_with_options,
    FightRunOptions, FightRunOutput,
};
pub use trace::{TraceConfig, TraceEvent};
pub use start_order::compute_turn_order;
pub use summons::{load_summons_json, SummonTemplate};
pub use weapons::{load_weapons_json, WeaponStats};
pub use world::{ActiveEffect, FightWorld, SimEntity};
