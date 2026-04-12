//! Shared HTTP client and credential resolution for authenticated commands.

use anyhow::anyhow;
use leekwars_api::LeekWarsClient;

use crate::cli::Cli;
use crate::config;

pub fn auth(cli: &Cli) -> anyhow::Result<(String, String)> {
    config::resolve_credentials(config::AuthInput {
        login: &cli.login,
        password: &cli.password,
        profile: &cli.profile,
        config: cli.config.as_deref(),
    })
}

pub fn client() -> anyhow::Result<LeekWarsClient> {
    LeekWarsClient::new().map_err(|e| anyhow!("{e}"))
}
