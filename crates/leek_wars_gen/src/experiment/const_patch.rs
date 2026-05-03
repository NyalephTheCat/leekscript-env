//! Replace top-level `const Name = …;` initializers in `.leek` sources (single-line convention).

use crate::error::GenError;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

fn const_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<indent>\s*)const\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?P<init>.+?)\s*;\s*(?P<trail>//.*)?$")
            .expect("const line regex")
    })
}

fn value_to_leek_literal(v: &Value) -> Result<String, GenError> {
    match v {
        Value::Null => Err(GenError::Message(
            "null is not a valid Leek const initializer for experiment tunables".into(),
        )),
        Value::Bool(b) => Ok(if *b { "true".into() } else { "false".into() }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.to_string())
            } else if let Some(u) = n.as_u64() {
                Ok(u.to_string())
            } else if let Some(f) = n.as_f64() {
                Ok(format!("{f:?}"))
            } else {
                Err(GenError::Message(format!("unsupported number {n}")))
            }
        }
        Value::String(s) => {
            let escaped = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            Ok(format!("\"{escaped}\""))
        }
        Value::Array(_) | Value::Object(_) => Err(GenError::Message(
            "array/object tunable values are not supported; use a scalar".into(),
        )),
    }
}

/// Apply replacements to `const` declarations matching `name` (first match per name wins per line scan).
pub fn patch_leek_constants(
    source: &str,
    replacements: &HashMap<String, Value>,
) -> Result<String, GenError> {
    if replacements.is_empty() {
        return Ok(source.to_string());
    }
    let mut out_lines: Vec<String> = Vec::new();
    for line in source.lines() {
        if let Some(caps) = const_line_re().captures(line) {
            let name = caps.name("name").unwrap().as_str();
            if let Some(val) = replacements.get(name) {
                let indent = caps.name("indent").unwrap().as_str();
                let lit = value_to_leek_literal(val)?;
                let trail = caps.name("trail").map_or("", |m| m.as_str());
                let trail_part = if trail.is_empty() {
                    String::new()
                } else {
                    format!(" {trail}")
                };
                out_lines.push(format!("{indent}const {name} = {lit};{trail_part}"));
                continue;
            }
        }
        out_lines.push(line.to_string());
    }
    Ok(out_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn patches_numeric_and_bool() {
        let src = "const FOO = 1;\nconst BAR = false;\n";
        let mut m = HashMap::new();
        m.insert("FOO".into(), json!(42));
        m.insert("BAR".into(), json!(true));
        let out = patch_leek_constants(src, &m).unwrap();
        assert!(out.contains("const FOO = 42;"));
        assert!(out.contains("const BAR = true;"));
    }

    #[test]
    fn preserves_comment_suffix() {
        let src = "const X = 0; // tune\n";
        let mut m = HashMap::new();
        m.insert("X".into(), json!(7));
        let out = patch_leek_constants(src, &m).unwrap();
        assert!(out.contains("const X = 7; // tune"));
    }
}
