use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GenError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("scenario JSON error: {0}")]
    ScenarioJson(#[from] serde_json::Error),

    #[error("Java generator exited with status {:?}; stderr:\n{stderr}", status)]
    JavaFailed { status: Option<i32>, stderr: String },

    #[error("Java generator produced invalid UTF-8 stdout")]
    JavaInvalidUtf8,

    #[error("could not find generator.jar; set LEEK_GENERATOR_JAR or place leek-wars-generator/generator.jar next to the workspace")]
    JarNotFound,

    #[error("generator working directory does not exist: {0}")]
    CwdMissing(PathBuf),

    #[error("{0}")]
    Message(String),
}
