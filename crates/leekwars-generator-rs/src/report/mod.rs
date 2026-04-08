pub mod fight_report;
pub mod enums;

pub use fight_report::{
    find_game_data_dir, format_outcome_human, format_outcome_human_for_path,
    format_outcome_human_with_game, GameNames,
};

