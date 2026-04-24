//! Leek Wars fight generation from Rust.
//!
//! - **Scenario**: JSON format accepted by the official [`generator.jar`](https://github.com/leek-wars/leek-wars-generator).
//! - **Engines**: [`RustEngine`](engine::RustEngine) runs the in-tree fight loop on [`leekscript_run`] (default `leekgen` engine).
//!   [`JavaEngine`](engine::JavaEngine) shells out to the JVM (`java -jar …`) for parity with `com.leekwars.Main` when requested.
//! - **Parity**: [`parity::normalize_outcome_json`] strips volatile timing fields so two runs can be compared.
//! - **Fight sim**: [`fight`] runs scenarios with [`leekscript_run`] + a Leek Wars [`fight::FightHost`] (simplified vs the official generator).
pub mod compare_fuzz_cli;
pub mod config;
pub mod engine;
pub mod error;
pub mod fight;
pub mod fuzz;
pub mod fuzz_input;
pub mod harness;
pub mod output;
pub mod parity;
pub mod scenario;
pub mod scenario_io;

pub use engine::{JavaEngine, JavaEngineConfig, RunRequest, RustEngine};
pub use error::GenError;
pub use scenario::Scenario;
