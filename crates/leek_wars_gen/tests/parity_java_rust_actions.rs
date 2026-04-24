//! Compare action code sequences between the Java generator and the Rust engine for `scenario1`.
//!
//! **Rust is the reference:** we require the filtered Java code stream to be a subsequence of the
//! Rust stream (same order). We only compare a **filtered subset** of codes (turn markers +
//! movement/weapon/death/error) while the Rust sim is still missing most effects/logs.

use leek_wars_gen::engine::{
    default_java_cwd, resolve_generator_jar, JavaEngine, JavaEngineConfig, RunRequest, RustEngine,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn java_bin() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(PathBuf::from)
        .map(|mut p| {
            p.push("bin/java");
            p
        })
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("java"))
}

fn extract_action_codes(outcome: &Value) -> Vec<i64> {
    let mut out = Vec::new();
    let actions = outcome
        .get("fight")
        .and_then(|f| f.get("actions"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    for a in actions {
        let code = a
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_i64());
        if let Some(c) = code {
            out.push(c);
        }
    }
    out
}

fn is_subsequence(needle: &[i64], haystack: &[i64]) -> bool {
    let mut i = 0usize;
    for &h in haystack {
        if i < needle.len() && needle[i] == h {
            i += 1;
        }
        if i == needle.len() {
            return true;
        }
    }
    i == needle.len()
}

#[test]
fn scenario1_java_and_rust_match_filtered_action_codes() {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let cwd = default_java_cwd(&jar);
    if !cwd.join("test/scenario/scenario1.json").is_file() {
        return;
    }

    // Rust is the reference; Java's filtered codes must appear in the same relative order in Rust.
    let keep: BTreeSet<i64> = [0, 5, 6, 7, 8, 10, 12, 13, 16, 101].into_iter().collect();

    let req = RunRequest {
        file: PathBuf::from("test/scenario/scenario1.json"),
        ..Default::default()
    };

    let java_out = {
        let cfg = JavaEngineConfig {
            jar: jar.clone(),
            cwd: cwd.clone(),
            java_bin: java_bin(),
        };
        JavaEngine::new(cfg).run(&req).expect("java scenario1")
    };
    let rust_out = RustEngine.run_scenario(&req).expect("rust scenario1");

    let j: Value = serde_json::from_str(&java_out).unwrap();
    let r: Value = serde_json::from_str(&rust_out).unwrap();

    let j_codes: Vec<i64> = extract_action_codes(&j)
        .into_iter()
        .filter(|c| keep.contains(c))
        .collect();
    let r_codes: Vec<i64> = extract_action_codes(&r)
        .into_iter()
        .filter(|c| keep.contains(c))
        .collect();

    assert!(
        is_subsequence(&j_codes, &r_codes),
        "java action codes (filtered) should be a subsequence of rust (reference).\njava={j_codes:?}\nrust={r_codes:?}"
    );
}
