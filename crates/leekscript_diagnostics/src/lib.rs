//! Stable `E####` diagnostic registry (see `data/diagnostics/registry.yaml`).

use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("IO error reading registry: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("duplicate diagnostic code: {0}")]
    DuplicateCode(String),
    #[error("registry schema_version {0} is not supported (expected 1)")]
    UnsupportedSchema(u32),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Registry {
    pub schema_version: u32,
    pub reference_source: String,
    pub entries: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryEntry {
    pub code: String,
    pub reference: Option<String>,
    pub band: Option<String>,
    pub id: Option<String>,
}

impl Registry {
    /// Load and validate the registry file (unique `code` values).
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::Io`] on read failure, [`RegistryError::Yaml`] if the file is not valid YAML,
    /// [`RegistryError::UnsupportedSchema`] if `schema_version` is not `1`, or [`RegistryError::DuplicateCode`]
    /// when two entries share the same `code`.
    pub fn load_path(path: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let bytes = std::fs::read(path.as_ref())?;
        Self::from_slice(&bytes)
    }

    /// Parse a registry from YAML bytes and validate it.
    ///
    /// # Errors
    ///
    /// Same as [`Registry::load_path`], except I/O errors are not produced.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, RegistryError> {
        let reg: Registry = serde_yaml::from_slice(bytes)?;
        if reg.schema_version != 1 {
            return Err(RegistryError::UnsupportedSchema(reg.schema_version));
        }
        let mut seen = HashSet::new();
        for e in &reg.entries {
            if !seen.insert(e.code.clone()) {
                return Err(RegistryError::DuplicateCode(e.code.clone()));
            }
        }
        Ok(reg)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Resolve stable `E####` for a Java `Error` name, if present in the registry.
    #[must_use]
    pub fn code_for_reference(&self, reference: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.reference.as_deref() == Some(reference))
            .map(|e| e.code.as_str())
    }

    /// Resolve stable `E####` for a toolchain-only `id` (e.g. `unknown_leek_directive`).
    #[must_use]
    pub fn code_for_id(&self, id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.id.as_deref() == Some(id))
            .map(|e| e.code.as_str())
    }

    /// References (Java `Error` names) missing from this registry — used to validate `lek` emit lists.
    #[must_use]
    pub fn missing_references<'a>(&self, refs: &[&'a str]) -> Vec<&'a str> {
        refs.iter()
            .copied()
            .filter(|r| self.code_for_reference(r).is_none())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sample_registry_roundtrip() {
        let yaml = r"
schema_version: 1
reference_source: test
entries:
  - code: E0001
    reference: FOO
    band: parse
";
        let reg = Registry::from_slice(yaml.as_bytes()).unwrap();
        assert_eq!(reg.entries.len(), 1);
        assert_eq!(reg.entries[0].code, "E0001");
    }

    #[test]
    fn loads_repo_registry_yaml() {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.pop();
        p.push("data/diagnostics/registry.yaml");
        let reg = Registry::load_path(&p).expect("repo registry.yaml should load");
        assert!(reg.len() >= 148);
        assert_eq!(reg.code_for_id("unknown_leek_directive"), Some("E7201"));
        assert_eq!(
            reg.code_for_id("leek_directive_invalid_value"),
            Some("E7202")
        );
        assert_eq!(reg.code_for_reference("VARIABLE_NOT_EXISTS"), Some("E1002"));
        assert_eq!(reg.code_for_reference("DIVISION_BY_ZERO"), Some("E5000"));
        assert_eq!(
            reg.code_for_reference("THIS_NOT_ALLOWED_HERE"),
            Some("E4600")
        );
        assert_eq!(reg.code_for_reference("NOT_ITERABLE"), Some("E3600"));
        assert_eq!(reg.code_for_reference("WRONG_ARGUMENT_TYPE"), Some("E2200"));
    }

    #[test]
    fn missing_references_empty_when_all_known() {
        let yaml = r"
schema_version: 1
reference_source: test
entries:
  - code: E0001
    reference: FOO
    band: parse
";
        let reg = Registry::from_slice(yaml.as_bytes()).unwrap();
        assert!(reg.missing_references(&["FOO"]).is_empty());
        assert_eq!(reg.missing_references(&["FOO", "BAR"]), vec!["BAR"]);
    }
}
