pub mod enums;
pub mod fight_report;

pub use fight_report::{
    GameNames, find_game_data_dir, format_outcome_human, format_outcome_human_for_path,
    format_outcome_human_with_game,
};
