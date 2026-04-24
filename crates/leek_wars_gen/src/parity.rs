//! Compare fight outcomes while ignoring volatile top-level timing fields.
//!
//! Normalized equality includes `logs` and the full `fight` object (`actions`, `ops`, `map`, `dead`, …).

use serde_json::Value;
use std::collections::BTreeSet;

const TOP_LEVEL_TIME_KEYS: &[&str] = &["analyze_time", "compilation_time", "execution_time"];

/// Max lines emitted by [`diff_normalized_outcomes`]; further differences are omitted after this.
pub const MAX_NORMALIZED_DIFF_LINES: usize = 200;

/// Max characters for a single value preview on a diff line.
const MAX_VALUE_CHARS: usize = 180;

/// Parse outcome JSON and remove top-level timing keys (nanoseconds vary run-to-run).
pub fn normalize_outcome_json(s: &str) -> Result<Value, serde_json::Error> {
    let mut v: Value = serde_json::from_str(s)?;
    if let Value::Object(map) = &mut v {
        for k in TOP_LEVEL_TIME_KEYS {
            map.remove(*k);
        }
        if let Some(logs) = map.get_mut("logs") {
            normalize_logs_in_place(logs);
        }
    }
    Ok(v)
}

fn normalize_logs_in_place(v: &mut Value) {
    match v {
        Value::String(s) => {
            // The official generator’s logs sometimes degrade this marker depending on encoding/font.
            // Normalize to the generator-side placeholder so parity checks focus on semantics.
            if s.contains('▶') {
                *s = s.replace('▶', "?");
            }
        }
        Value::Array(a) => {
            for el in a {
                normalize_logs_in_place(el);
            }
        }
        Value::Object(o) => {
            for (_k, el) in o.iter_mut() {
                normalize_logs_in_place(el);
            }
        }
        _ => {}
    }
}

/// Equality of normalized outcomes (stable for the same scenario seed and jar).
pub fn outcomes_equal_ignore_timing(a: &str, b: &str) -> Result<bool, serde_json::Error> {
    let na = normalize_outcome_json(a)?;
    let nb = normalize_outcome_json(b)?;
    Ok(na == nb)
}

fn fmt_json_path(path: &[String]) -> String {
    if path.is_empty() {
        "<root>".to_string()
    } else {
        path.iter().fold(String::new(), |acc, seg| {
            if acc.is_empty() {
                format!("/{}", seg)
            } else {
                format!("{}/{}", acc, seg)
            }
        })
    }
}

fn short_value_preview(v: &Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_else(|_| "\"<invalid>\"".to_string());
    if s.len() <= MAX_VALUE_CHARS {
        s
    } else {
        let n = MAX_VALUE_CHARS.saturating_sub(20);
        format!("{}… ({} chars total)", &s[..n], s.len())
    }
}

fn push_diff_line(lines: &mut Vec<String>, budget: &mut usize, line: String) {
    if *budget == 0 {
        return;
    }
    lines.push(line);
    *budget = budget.saturating_sub(1);
}

/// Walk two normalized JSON values; `left_label` / `right_label` name the sides (e.g. `generator` / `rust`).
fn diff_values(
    left: &Value,
    right: &Value,
    path: &mut Vec<String>,
    lines: &mut Vec<String>,
    budget: &mut usize,
    left_label: &str,
    right_label: &str,
) {
    if *budget == 0 {
        return;
    }
    if left == right {
        return;
    }

    match (left, right) {
        (Value::Object(oa), Value::Object(ob)) => {
            let keys: BTreeSet<_> = oa.keys().chain(ob.keys()).cloned().collect();
            for k in keys {
                let pa = oa.get(&k);
                let pb = ob.get(&k);
                path.push(k);
                match (pa, pb) {
                    (Some(va), Some(vb)) => {
                        diff_values(va, vb, path, lines, budget, left_label, right_label);
                    }
                    (None, Some(vb)) => {
                        let p = fmt_json_path(path);
                        push_diff_line(
                            lines,
                            budget,
                            format!("- [{}] {}: <absent>", left_label, p),
                        );
                        push_diff_line(
                            lines,
                            budget,
                            format!("+ [{}] {}: {}", right_label, p, short_value_preview(vb)),
                        );
                    }
                    (Some(va), None) => {
                        let p = fmt_json_path(path);
                        push_diff_line(
                            lines,
                            budget,
                            format!("- [{}] {}: {}", left_label, p, short_value_preview(va)),
                        );
                        push_diff_line(
                            lines,
                            budget,
                            format!("+ [{}] {}: <absent>", right_label, p),
                        );
                    }
                    (None, None) => {}
                }
                path.pop();
            }
        }
        (Value::Array(aa), Value::Array(ab)) => {
            if aa.len() != ab.len() {
                let p = fmt_json_path(path);
                push_diff_line(
                    lines,
                    budget,
                    format!(
                        "! [{} vs {}] {}: array length {} vs {}",
                        left_label,
                        right_label,
                        p,
                        aa.len(),
                        ab.len()
                    ),
                );
            }
            let n = aa.len().min(ab.len());
            for i in 0..n {
                path.push(i.to_string());
                diff_values(&aa[i], &ab[i], path, lines, budget, left_label, right_label);
                path.pop();
            }
            for i in n..aa.len() {
                path.push(i.to_string());
                let p = fmt_json_path(path);
                push_diff_line(
                    lines,
                    budget,
                    format!("- [{}] {}: {}", left_label, p, short_value_preview(&aa[i])),
                );
                path.pop();
            }
            for i in n..ab.len() {
                path.push(i.to_string());
                let p = fmt_json_path(path);
                push_diff_line(
                    lines,
                    budget,
                    format!("+ [{}] {}: {}", right_label, p, short_value_preview(&ab[i])),
                );
                path.pop();
            }
        }
        _ => {
            let p = fmt_json_path(path);
            push_diff_line(
                lines,
                budget,
                format!("- [{}] {}: {}", left_label, p, short_value_preview(left)),
            );
            push_diff_line(
                lines,
                budget,
                format!("+ [{}] {}: {}", right_label, p, short_value_preview(right)),
            );
        }
    }
}

/// Human-readable structural diff of two outcomes after [`normalize_outcome_json`].
///
/// For [`diff_normalized_outcomes`], lines use `- [rust]` (reference) / `+ [generator]` at diverging paths.
/// For [`diff_normalized_outcomes_labeled`], `-` is always the first JSON argument, `+` the second.
/// Empty string if equal after normalization.
pub fn diff_normalized_outcomes_labeled(
    a: &str,
    b: &str,
    left_label: &str,
    right_label: &str,
) -> Result<String, serde_json::Error> {
    let na = normalize_outcome_json(a)?;
    let nb = normalize_outcome_json(b)?;
    if na == nb {
        return Ok(String::new());
    }
    let mut lines = Vec::new();
    let mut budget = MAX_NORMALIZED_DIFF_LINES;
    let mut path = Vec::new();
    diff_values(
        &na,
        &nb,
        &mut path,
        &mut lines,
        &mut budget,
        left_label,
        right_label,
    );
    if budget == 0 && !lines.is_empty() {
        lines.push(format!(
            "… diff truncated (increase leek_wars_gen::parity::MAX_NORMALIZED_DIFF_LINES; limit is {})",
            MAX_NORMALIZED_DIFF_LINES
        ));
    }
    Ok(lines.join("\n"))
}

/// Structural diff with **Rust as the reference** (baseline on `-` lines).
///
/// Arguments are `(official_generator_json, rust_json)` for call-site consistency with [`crate::harness::compare_outcomes`].
pub fn diff_normalized_outcomes(official_generator_json: &str, rust_json: &str) -> Result<String, serde_json::Error> {
    diff_normalized_outcomes_labeled(rust_json, official_generator_json, "rust", "generator")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_top_level_times() {
        let j = r#"{"fight":{},"logs":{},"winner":0,"duration":0,"analyze_time":1,"compilation_time":2,"execution_time":3}"#;
        let v = normalize_outcome_json(j).unwrap();
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("execution_time"));
        assert!(obj.contains_key("fight"));
        assert!(obj.contains_key("logs"));
    }

    #[test]
    fn diff_reports_leaf_mismatch() {
        let a = r#"{"winner":1,"fight":{}}"#;
        let b = r#"{"winner":2,"fight":{}}"#;
        let d = diff_normalized_outcomes(a, b).unwrap();
        assert!(d.contains("/winner"));
        assert!(d.contains("generator"));
        assert!(d.contains("rust"));
    }

    #[test]
    fn diff_empty_when_equal_modulo_timing() {
        let a = r#"{"fight":{},"winner":0,"analyze_time":1}"#;
        let b = r#"{"fight":{},"winner":0,"execution_time":9}"#;
        let d = diff_normalized_outcomes(a, b).unwrap();
        assert!(d.is_empty());
    }
}
