pub mod analyze;
pub mod generator;
pub mod map;
pub mod outcome;
pub mod persistence;
pub mod report;
pub mod scenario;
pub mod util;
pub mod vm;

// Compatibility shims for legacy paths used by `vm/state.rs` include!.
mod registers;
mod world_map;

pub mod batch;
pub mod snapshot;

pub use analyze::{AnalyzeDiagnostic, analyze_ai_source, analyze_ai_source_with_path};
pub use batch::{
    BatchJob, BatchResult, BatchRunner, EntityCartesianBlock, SweepCartesian, format_batch_human,
};
pub use generator::Generator;
pub use outcome::Outcome;
pub use persistence::{
    DirRegisterManager, FileRegisterManager, InMemoryRegisterManager, RegisterManager,
    RegisterManagerRc, Registers,
};
pub use report::{
    GameNames, find_game_data_dir, format_outcome_human, format_outcome_human_for_path,
    format_outcome_human_with_game,
};
pub use scenario::Scenario;
pub use snapshot::{FightSnapshot, snapshot_at_action_index};
pub use vm::{LeekWarsContext, LeekWarsEntity, LeekWarsState};
