//! `leekwars` — CLI for [Leek Wars](https://leekwars.com) via [`leekwars_api`].
//!
//! Unofficial client: follow site rules and avoid hammering the API.

mod batch;
mod build;
mod cli;
mod commands;
mod config;
mod output;
mod session;

use clap::Parser;

use crate::cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    commands::run(cli).await
}
