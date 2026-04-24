//! TOML **signature** files: declare names that exist at global scope for static resolution
//! (`lek check`) without implementing them in the tree interpreter.
//!
//! Typical use: Leek Wars AI globals and natives (`getLife`, `useChip`, …) so project `.leek` files
//! typecheck in-repo before running them in the Java generator or a Rust fight host.
//!
//! # Format (`*.toml`)
//!
//! ```toml
//! schema_version = 1
//!
//! # Values treated like `global x` for name resolution
//! globals = ["WEAPON_LASER", "MAP_WIDTH"]
//!
//! # Function names at file scope (same as `function foo() { … }` for resolve)
//! functions = ["getLife", "attack", "useChip"]
//! ```

use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use thiserror::Error;

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("unsupported schema_version {0} (only 1 is supported)")]
    UnsupportedSchema(u32),
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignatureFile {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub globals: Vec<String>,
    #[serde(default)]
    pub functions: Vec<String>,
}

impl SignatureFile {
    pub fn from_str(s: &str) -> Result<Self, SignatureError> {
        let f: SignatureFile = toml::from_str(s)?;
        f.validate()?;
        Ok(f)
    }

    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, SignatureError> {
        let s = fs::read_to_string(path.as_ref())?;
        Self::from_str(&s)
    }

    fn validate(&self) -> Result<(), SignatureError> {
        if self.schema_version != 1 {
            return Err(SignatureError::UnsupportedSchema(self.schema_version));
        }
        Ok(())
    }

    /// Names to pre-seed the resolve global scope (`globals` ∪ `functions`, stable order, first wins).
    pub fn resolve_names(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for list in [&self.globals, &self.functions] {
            for n in list.iter() {
                let n = n.trim();
                if n.is_empty() {
                    continue;
                }
                if seen.insert(n.to_string()) {
                    out.push(n.to_string());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_globals_and_functions_dedupes() {
        let f = SignatureFile::from_str(
            r#"
 schema_version = 1
            globals = ["a", "b", "a"]
            functions = ["b", "c"]
        "#,
        )
        .unwrap();
        assert_eq!(f.resolve_names(), vec!["a", "b", "c"]);
    }
}
