//! Unofficial HTTP client for the [Leek Wars](https://leekwars.com) JSON API.
//!
//! Behaviour matches the official Vue app ([`leek-wars` frontend](https://github.com/leek-wars/leek-wars)):
//! JSON bodies, session cookies after [`LeekWarsClient::farmer_login`], optional `Authorization: Bearer`.
//!
//! **Unofficial**: respect site terms of use and rate limits.
//!
//! # Example
//!
//! ```no_run
//! use leekwars_api::LeekWarsClient;
//!
//! # async fn demo() -> Result<(), leekwars_api::Error> {
//! let client = LeekWarsClient::new()?;
//! let v = client.data_version().await?;
//! println!("master_version = {}", v.master_version);
//! # Ok(())
//! # }
//! ```

mod ai;
mod ai_export;
mod auth;
mod client;
mod data;
mod encyclopedia;
mod error;
mod farmer;
mod fight;
mod garden;
mod history;
mod leek;
mod market;
mod message;
mod meta;
mod notification;
mod ranking;
mod team;
mod tournament;

pub use ai::{AiSaveRequest, AiSyncEntry, AiSyncRequest};
pub use ai_export::{AiExportOptions, AiExportReport, ai_leek_relative_path};
pub use client::{LeekWarsClient, LeekWarsClientBuilder};
pub use data::{DataVersion, GameDataAll};
pub use error::{ApiErrorBody, Error, Result};
pub use farmer::{FarmerLoginComebackRequest, FarmerSetLanguageRequest};
pub use leek::{
    LeekCreateRequest, LeekIdBody, LeekRenameBody, LeekSetAiBody, LeekSetInGardenBody,
    LeekSetXpBlockedBody, LeekSpendCapitalBody,
};
pub use notification::NotificationReadBody;
pub use ranking::RankingSearchRequest;
