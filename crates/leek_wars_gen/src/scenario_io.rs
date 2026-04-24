use crate::error::GenError;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ScenarioFormat {
    Json,
    Toml,
}

pub fn infer_format(path: &Path) -> Option<ScenarioFormat> {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "json" => Some(ScenarioFormat::Json),
        "toml" => Some(ScenarioFormat::Toml),
        _ => None,
    }
}

pub fn load_value(path: &Path) -> Result<Value, GenError> {
    let raw = std::fs::read_to_string(path)?;
    match infer_format(path).ok_or_else(|| {
        GenError::Message(format!(
            "scenario format not recognized for {} (expected .json or .toml)",
            path.display()
        ))
    })? {
        ScenarioFormat::Json => serde_json::from_str(&raw).map_err(GenError::ScenarioJson),
        ScenarioFormat::Toml => {
            let tv: toml::Value =
                toml::from_str(&raw).map_err(|e| GenError::Message(e.to_string()))?;
            serde_json::to_value(tv).map_err(|e| GenError::Message(e.to_string()))
        }
    }
}

pub fn write_value(path: &Path, format: ScenarioFormat, v: &Value) -> Result<(), GenError> {
    match format {
        ScenarioFormat::Json => {
            let s = serde_json::to_string_pretty(v).map_err(GenError::ScenarioJson)?;
            std::fs::write(path, s)?;
        }
        ScenarioFormat::Toml => {
            let tv: toml::Value = json_to_toml(v)?;
            let s = toml::to_string_pretty(&tv).map_err(|e| GenError::Message(e.to_string()))?;
            std::fs::write(path, s)?;
        }
    }
    Ok(())
}

fn json_to_toml(v: &Value) -> Result<toml::Value, GenError> {
    Ok(match v {
        Value::Null => {
            return Err(GenError::Message(
                "cannot represent JSON null in TOML".into(),
            ))
        }
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                return Err(GenError::Message(format!(
                    "cannot represent JSON number {n} in TOML"
                )));
            }
        }
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for el in arr {
                out.push(json_to_toml(el)?);
            }
            toml::Value::Array(out)
        }
        Value::Object(obj) => {
            let mut t = toml::value::Table::new();
            for (k, vv) in obj {
                t.insert(k.clone(), json_to_toml(vv)?);
            }
            toml::Value::Table(t)
        }
    })
}

pub fn validate_value(v: &Value) -> Result<(), GenError> {
    let obj = v
        .as_object()
        .ok_or_else(|| GenError::Message("scenario must be a JSON object".into()))?;

    fn req<'a>(obj: &'a serde_json::Map<String, Value>, k: &str) -> Result<&'a Value, GenError> {
        obj.get(k)
            .ok_or_else(|| GenError::Message(format!("scenario missing required key {k:?}")))
    }

    req(obj, "farmers")?
        .as_array()
        .ok_or_else(|| GenError::Message("scenario.farmers must be an array".into()))?;
    req(obj, "teams")?
        .as_array()
        .ok_or_else(|| GenError::Message("scenario.teams must be an array".into()))?;
    req(obj, "entities")?
        .as_array()
        .ok_or_else(|| GenError::Message("scenario.entities must be an array".into()))?;

    Ok(())
}

/// Materialize a JSON scenario file path for the Rust engine.
///
/// If `scenario_path` is already `.json`, returns it as-is. If it is `.toml`,
/// converts it to JSON in a temp file and returns that path plus a cleanup guard.
pub fn materialize_json_path(
    scenario_path: &Path,
) -> Result<(PathBuf, Option<TempFileGuard>), GenError> {
    match infer_format(scenario_path).ok_or_else(|| {
        GenError::Message(format!(
            "scenario format not recognized for {} (expected .json or .toml)",
            scenario_path.display()
        ))
    })? {
        ScenarioFormat::Json => Ok((scenario_path.to_path_buf(), None)),
        ScenarioFormat::Toml => {
            let v = load_value(scenario_path)?;
            validate_value(&v)?;
            let tmp = std::env::temp_dir().join(format!(
                "leekgen_scenario_{}.json",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            let s = serde_json::to_string(&v).map_err(GenError::ScenarioJson)?;
            std::fs::write(&tmp, s)?;
            Ok((tmp.clone(), Some(TempFileGuard { path: tmp })))
        }
    }
}

pub struct TempFileGuard {
    path: PathBuf,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(ext: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "leekgen_scenario_io_test_{}.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            ext
        ))
    }

    #[test]
    fn json_to_toml_to_json_roundtrip_smoke() {
        let json_path = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../leek-wars-generator/test/scenario/scenario1.json"
        ));
        let v = load_value(json_path.as_path()).expect("load json");
        validate_value(&v).expect("validate");

        let toml_path = tmp_path("toml");
        write_value(&toml_path, ScenarioFormat::Toml, &v).expect("write toml");

        let v2 = load_value(&toml_path).expect("load toml");
        validate_value(&v2).expect("validate toml->json");
        let _ = std::fs::remove_file(&toml_path);
    }

    #[test]
    fn materialize_json_path_for_toml_creates_temp_json() {
        let v = serde_json::json!({
            "farmers": [{ "id": 1, "name": "A", "country": "fr" }],
            "teams": [{ "id": 1, "name": "T1" }],
            "entities": [[]]
        });
        let toml_path = tmp_path("toml");
        write_value(&toml_path, ScenarioFormat::Toml, &v).expect("write toml");

        let (json_path, guard) = materialize_json_path(&toml_path).expect("materialize");
        assert!(json_path.extension().and_then(|e| e.to_str()) == Some("json"));
        assert!(json_path.is_file());
        assert!(guard.is_some(), "toml should create a temp json guard");

        drop(guard);
        let _ = std::fs::remove_file(&toml_path);
        assert!(!json_path.is_file(), "guard should remove temp json");
    }
}

