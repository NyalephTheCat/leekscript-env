use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistersError {
    #[error("register key too long (max {max}, got {got})")]
    KeyTooLong { max: usize, got: usize },
    #[error("register value too long (max {max}, got {got})")]
    ValueTooLong { max: usize, got: usize },
    #[error("too many register entries (max {max})")]
    TooManyEntries { max: usize },
}

/// Leek Wars persistent registers (saved between fights).
///
/// Mirrors Java generator semantics:
/// - bounded size limits
/// - `is_new` is true when no prior registers existed
/// - `modified` flips true on an effective change
#[derive(Debug, Clone, Default)]
pub struct Registers {
    values: BTreeMap<String, String>,
    modified: bool,
    is_new: bool,
}

impl Registers {
    pub const MAX_ENTRIES: usize = 100;
    pub const MAX_KEY_LENGTH: usize = 100;
    pub const MAX_DATA_LENGTH: usize = 5000;

    #[must_use]
    pub fn new(is_new: bool) -> Self {
        Self {
            values: BTreeMap::new(),
            modified: false,
            is_new,
        }
    }

    #[must_use]
    pub fn is_new(&self) -> bool {
        self.is_new
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    pub fn set(&mut self, key: String, value: String) -> Result<bool, RegistersError> {
        if self.values.len() > Self::MAX_ENTRIES {
            return Err(RegistersError::TooManyEntries {
                max: Self::MAX_ENTRIES,
            });
        }
        if key.len() > Self::MAX_KEY_LENGTH {
            return Err(RegistersError::KeyTooLong {
                max: Self::MAX_KEY_LENGTH,
                got: key.len(),
            });
        }
        if value.len() > Self::MAX_DATA_LENGTH {
            return Err(RegistersError::ValueTooLong {
                max: Self::MAX_DATA_LENGTH,
                got: value.len(),
            });
        }

        if let Some(old) = self.values.get(&key) {
            if old == &value {
                return Ok(true);
            }
        }
        self.modified = true;
        self.values.insert(key, value);
        Ok(true)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let existed = self.values.remove(key).is_some();
        if existed {
            self.modified = true;
        }
        existed
    }

    #[must_use]
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(&self.values).unwrap_or_else(|_| "{}".into())
    }

    #[must_use]
    pub fn from_json_string(value: &str, is_new: bool) -> Self {
        let mut out = Self::new(is_new);
        // Java ignores parse failures (treat as empty).
        let Ok(v) = serde_json::from_str::<serde_json::Value>(value) else {
            return out;
        };
        let Some(obj) = v.as_object() else {
            return out;
        };
        for (k, vv) in obj {
            if let Some(s) = vv.as_str() {
                // Respect limits best-effort; invalid entries are skipped.
                if k.len() <= Self::MAX_KEY_LENGTH && s.len() <= Self::MAX_DATA_LENGTH {
                    if out.values.len() >= Self::MAX_ENTRIES {
                        break;
                    }
                    out.values.insert(k.clone(), s.to_string());
                }
            }
        }
        out
    }
}

/// Persistence boundary for registers, modeled after Java generator's `RegisterManager`.
pub trait RegisterManager: std::fmt::Debug {
    fn get_registers(&self, entity_id: i64) -> Option<String>;
    fn save_registers(&self, entity_id: i64, registers_json: &str, is_new: bool);
}

/// Simple in-memory register store (useful for library embedding/tests).
#[derive(Debug, Default)]
pub struct InMemoryRegisterManager {
    map: std::cell::RefCell<HashMap<i64, String>>,
}

impl InMemoryRegisterManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl RegisterManager for InMemoryRegisterManager {
    fn get_registers(&self, entity_id: i64) -> Option<String> {
        self.map.borrow().get(&entity_id).cloned()
    }

    fn save_registers(&self, entity_id: i64, registers_json: &str, _is_new: bool) {
        self.map
            .borrow_mut()
            .insert(entity_id, registers_json.to_string());
    }
}

/// File-backed register store. Stores all registers in one JSON file:
/// `{ "123": {"k":"v"}, "456": {"k":"v"} }`
#[derive(Debug, Clone)]
pub struct FileRegisterManager {
    path: PathBuf,
}

impl FileRegisterManager {
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    fn load_all(&self) -> HashMap<String, serde_json::Value> {
        let Ok(src) = std::fs::read_to_string(&self.path) else {
            return HashMap::new();
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&src) else {
            return HashMap::new();
        };
        v.as_object()
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    fn save_all(&self, all: &HashMap<String, serde_json::Value>) {
        let v = serde_json::Value::Object(
            all.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        if let Ok(out) = serde_json::to_string_pretty(&v) {
            let _ = std::fs::create_dir_all(
                self.path
                    .parent()
                    .unwrap_or_else(|| Path::new(".")),
            );
            let _ = std::fs::write(&self.path, out.as_bytes());
        }
    }
}

impl RegisterManager for FileRegisterManager {
    fn get_registers(&self, entity_id: i64) -> Option<String> {
        let all = self.load_all();
        let key = entity_id.to_string();
        let v = all.get(&key)?;
        // Stored as object; return as JSON string compatible with Java.
        serde_json::to_string(v).ok()
    }

    fn save_registers(&self, entity_id: i64, registers_json: &str, _is_new: bool) {
        let mut all = self.load_all();
        let key = entity_id.to_string();
        let v = serde_json::from_str::<serde_json::Value>(registers_json).unwrap_or_else(|_| {
            // Best-effort; keep a valid object.
            serde_json::json!({})
        });
        all.insert(key, v);
        self.save_all(&all);
    }
}

/// Directory-backed register store. Stores one file per entity: `<dir>/<entity_id>.json`
#[derive(Debug, Clone)]
pub struct DirRegisterManager {
    dir: PathBuf,
}

impl DirRegisterManager {
    #[must_use]
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_path_buf(),
        }
    }

    fn entity_path(&self, entity_id: i64) -> PathBuf {
        self.dir.join(format!("{entity_id}.json"))
    }

    pub fn reset(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl RegisterManager for DirRegisterManager {
    fn get_registers(&self, entity_id: i64) -> Option<String> {
        let path = self.entity_path(entity_id);
        let src = std::fs::read_to_string(path).ok()?;
        // Stored as object; return canonical JSON string compatible with Java.
        let v = serde_json::from_str::<serde_json::Value>(&src).ok()?;
        serde_json::to_string(&v).ok()
    }

    fn save_registers(&self, entity_id: i64, registers_json: &str, _is_new: bool) {
        let v = serde_json::from_str::<serde_json::Value>(registers_json)
            .unwrap_or_else(|_| serde_json::json!({}));
        let Ok(out) = serde_json::to_string_pretty(&v) else { return };
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.entity_path(entity_id), out.as_bytes());
    }
}

pub type RegisterManagerRc = Rc<dyn RegisterManager>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_json_roundtrip_and_modified_flag() {
        let mut r = Registers::new(true);
        assert!(r.is_new());
        assert!(!r.is_modified());
        r.set("k".into(), "v".into()).unwrap();
        assert!(r.is_modified());
        let s = r.to_json_string();
        let r2 = Registers::from_json_string(&s, false);
        assert_eq!(r2.get("k"), Some("v"));
        assert!(!r2.is_modified(), "loading should not mark modified");
    }

    #[test]
    fn registers_limits_enforced() {
        let mut r = Registers::new(true);
        let long_key = "a".repeat(Registers::MAX_KEY_LENGTH + 1);
        assert!(matches!(
            r.set(long_key, "v".into()),
            Err(RegistersError::KeyTooLong { .. })
        ));
        let long_val = "b".repeat(Registers::MAX_DATA_LENGTH + 1);
        assert!(matches!(
            r.set("k".into(), long_val),
            Err(RegistersError::ValueTooLong { .. })
        ));
    }
}

