//! Error types for API and transport failures.

use serde::Deserialize;

/// JSON body returned by the API on many error responses (matches the Vue client's `error` toast).
#[derive(Debug, Deserialize)]
pub struct ApiErrorBody {
    #[serde(default)]
    pub error: Option<String>,
}

impl ApiErrorBody {
    pub fn message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HTTP error {status}: {body}")]
    Http {
        status: u16,
        body: String,
        /// `Retry-After` header when the server sent a delay (seconds), e.g. for HTTP 429.
        retry_after_secs: Option<u64>,
    },
    #[error("API error: {0}")]
    Api(String),
    #[error("failed to deserialize response: {0}")]
    Deserialize(#[from] serde_json::Error),
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
