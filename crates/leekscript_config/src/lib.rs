//! `Leek.toml` discovery and validation — see repository `docs/design/leek-toml.md`.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml::Value;

const ALLOWED_TOP_LEVEL: &[&str] = &[
    "package",
    "language",
    "fmt",
    "lint",
    "experimental",
    "schema_version",
    "signatures",
    "generator",
];

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("invalid Leek.toml: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct LeekManifest {
    pub schema_version: Option<u32>,
    pub package: Option<toml::Table>,
    pub language: Option<Language>,
    pub fmt: Option<toml::Table>,
    pub lint: Option<Lint>,
    pub experimental: Option<Experimental>,
    /// Optional [`leekscript_signatures`] TOML path (relative to this manifest’s directory).
    pub signatures: Option<SignaturesConfig>,
    /// `leekgen` / Leek Wars generator configuration (parsed by `leek_wars_gen`).
    pub generator: Option<toml::Table>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignaturesConfig {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Language {
    pub version: Option<i64>,
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Lint {
    pub level: Option<String>,
    pub deny: Option<Vec<String>>,
    pub allow: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Experimental {
    pub features: Option<Vec<String>>,
}

impl LeekManifest {
    pub fn from_str(s: &str) -> Result<Self, ConfigError> {
        let v: Value = s.parse().map_err(ConfigError::TomlParse)?;
        validate_top_level_keys(&v)?;
        let m: LeekManifest = toml::from_str(s).map_err(ConfigError::TomlParse)?;
        m.validate()?;
        Ok(m)
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let s = std::fs::read_to_string(path.as_ref())?;
        Self::from_str(&s)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if let Some(v) = self.schema_version {
            if v != 1 {
                return Err(ConfigError::Invalid(format!(
                    "unsupported schema_version {v} (only 1 is supported)"
                )));
            }
        }
        if let Some(ref l) = self.language {
            if let Some(ver) = l.version {
                if ver < 1 || ver > 99 {
                    return Err(ConfigError::Invalid(format!(
                        "language.version {ver} is out of expected range (1–99)"
                    )));
                }
            }
        }
        if let Some(ref lint) = self.lint {
            if let Some(ref lvl) = lint.level {
                match lvl.as_str() {
                    "allow" | "warn" | "deny" => {}
                    _ => {
                        return Err(ConfigError::Invalid(format!(
                            "lint.level must be allow|warn|deny, got {lvl:?}"
                        )));
                    }
                }
            }
        }
        if let Some(ref fmt) = self.fmt {
            for (k, v) in fmt {
                validate_fmt_value(k, v)?;
            }
        }
        Ok(())
    }
}

fn validate_fmt_value(key: &str, v: &toml::Value) -> Result<(), ConfigError> {
    match key {
        "width" | "indent" | "tab_width" => match v {
            toml::Value::Integer(i) if *i > 0 => Ok(()),
            toml::Value::Integer(_) => {
                Err(ConfigError::Invalid(format!("fmt.{key} must be positive")))
            }
            _ => Err(ConfigError::Invalid(format!(
                "fmt.{key} must be an integer"
            ))),
        },
        "use_tabs" => match v {
            toml::Value::Boolean(_) => Ok(()),
            _ => Err(ConfigError::Invalid(
                "fmt.use_tabs must be a boolean".into(),
            )),
        },
        _ => Ok(()),
    }
}

fn validate_top_level_keys(v: &Value) -> Result<(), ConfigError> {
    let Some(table) = v.as_table() else {
        return Err(ConfigError::Invalid("root must be a TOML table".into()));
    };
    for key in table.keys() {
        if !ALLOWED_TOP_LEVEL.contains(&key.as_str()) {
            return Err(ConfigError::Invalid(format!(
                "unknown top-level key {key:?} (allowed: {ALLOWED_TOP_LEVEL:?})"
            )));
        }
    }
    Ok(())
}

/// Walk upward from `start` (file or directory) looking for `Leek.toml`.
pub fn find_manifest(mut dir: PathBuf) -> Option<PathBuf> {
    loop {
        let candidate = dir.join("Leek.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_valid() {
        let s = r#"
[language]
version = 4
strict = false
"#;
        LeekManifest::from_str(s).unwrap();
    }

    #[test]
    fn rejects_unknown_top_level() {
        let s = r#"
[language]
version = 4
[bad]
x = 1
"#;
        assert!(LeekManifest::from_str(s).is_err());
    }

    #[test]
    fn signatures_section_allowed() {
        let s = r#"
schema_version = 1
[signatures]
path = "lw-signatures.toml"
"#;
        let m = LeekManifest::from_str(s).unwrap();
        assert_eq!(
            m.signatures.as_ref().unwrap().path.as_deref(),
            Some("lw-signatures.toml")
        );
    }
}
