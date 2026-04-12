//! Optional TOML config (`leekwars.toml`) and credential resolution (.env, env vars, profiles).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow};
use serde::Deserialize;

/// Inputs for [`resolve_credentials`] (maps to global CLI flags).
pub struct AuthInput<'a> {
    pub login: &'a Option<String>,
    pub password: &'a Option<String>,
    pub profile: &'a Option<String>,
    pub config: Option<&'a Path>,
}

/// Parsed `leekwars.toml` (or fragment).
#[derive(Debug, Deserialize, Default)]
pub struct ConfigFile {
    /// When `--profile` is omitted, use this account name.
    #[serde(default)]
    pub default_profile: Option<String>,
    /// Single-account shorthand (no `[accounts.*]`).
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub accounts: HashMap<String, AccountEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AccountEntry {
    pub login: String,
    pub password: String,
}

impl ConfigFile {
    /// Normalize: top-level `login`/`password` become `accounts["default"]` if no accounts.
    fn normalized(mut self) -> Self {
        if self.accounts.is_empty() {
            if let (Some(login), Some(password)) = (self.login.take(), self.password.take()) {
                self.accounts
                    .insert("default".to_string(), AccountEntry { login, password });
            }
        }
        self
    }
}

/// Resolve config path: explicit `--config`, then env, then search.
pub fn resolve_config_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    if let Ok(p) = std::env::var("LEEKWARS_CONFIG") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let cwd = PathBuf::from("leekwars.toml");
    if cwd.is_file() {
        return Some(cwd);
    }
    dirs::config_dir().map(|d| d.join("leekwars").join("config.toml"))
}

pub fn load_config(path: &Path) -> anyhow::Result<ConfigFile> {
    let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let cfg: ConfigFile = toml::from_str(&text).with_context(|| path.display().to_string())?;
    Ok(cfg.normalized())
}

/// Pick login/password: explicit `--login`/`--password` (or `LEEKWARS_*` via clap) win; else TOML profile.
pub fn resolve_credentials(a: AuthInput<'_>) -> anyhow::Result<(String, String)> {
    let login = a.login.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    let password = a
        .password
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    match (login, password) {
        (Some(l), Some(p)) => return Ok((l.to_string(), p.to_string())),
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!(
                "provide both login and password (flags or LEEKWARS_LOGIN / LEEKWARS_PASSWORD), or configure accounts in a TOML file"
            );
        }
        (None, None) => {}
    }

    let path = match a.config {
        Some(p) => {
            if !p.is_file() {
                anyhow::bail!("config file not found: {}", p.display());
            }
            p.to_path_buf()
        }
        None => resolve_config_path(None).filter(|p| p.is_file()).ok_or_else(|| {
            anyhow!(
                "no credentials: set LEEKWARS_LOGIN and LEEKWARS_PASSWORD, use --login/--password, or add leekwars.toml (./leekwars.toml, $LEEKWARS_CONFIG, or ~/.config/leekwars/config.toml)"
            )
        })?,
    };

    let cfg = load_config(&path)?;
    if cfg.accounts.is_empty() {
        anyhow::bail!(
            "no [accounts.*] in {} — add at least one account",
            path.display()
        );
    }
    let entry = pick_account(&cfg, a.profile)?;
    Ok((entry.login.clone(), entry.password.clone()))
}

fn pick_account<'a>(
    cfg: &'a ConfigFile,
    profile_cli: &Option<String>,
) -> anyhow::Result<&'a AccountEntry> {
    if let Some(name) = profile_cli
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return cfg
            .accounts
            .get(name)
            .with_context(|| format!("unknown profile `{name}` in config"));
    }
    if let Some(ref name) = cfg.default_profile {
        return cfg
            .accounts
            .get(name)
            .with_context(|| format!("default_profile `{name}` not found in config"));
    }
    if cfg.accounts.len() == 1 {
        return Ok(cfg.accounts.iter().next().expect("len checked").1);
    }
    cfg.accounts.get("default").with_context(|| {
        format!(
            "set --profile, or default_profile in TOML, or a single [accounts.*] block — got: {}",
            cfg.accounts.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })
}

/// Load config from the same path rules as credential resolution (for `leekwars profiles`).
pub fn load_resolved_config(config: Option<&Path>) -> anyhow::Result<(PathBuf, ConfigFile)> {
    let path = match config {
        Some(p) => {
            if !p.is_file() {
                anyhow::bail!("config file not found: {}", p.display());
            }
            p.to_path_buf()
        }
        None => resolve_config_path(None).filter(|p| p.is_file()).ok_or_else(|| {
            anyhow!("no config file found (./leekwars.toml, $LEEKWARS_CONFIG, ~/.config/leekwars/config.toml)")
        })?,
    };
    let cfg = load_config(&path)?;
    if cfg.accounts.is_empty() {
        anyhow::bail!("no [accounts.*] in {}", path.display());
    }
    Ok((path, cfg))
}
