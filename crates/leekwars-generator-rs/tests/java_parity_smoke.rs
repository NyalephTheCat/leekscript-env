#![cfg(feature = "java-parity")]

use std::process::Command;

use serde_json::Value;

fn load_actions(path: &str) -> Vec<Value> {
    let src = std::fs::read_to_string(path).expect("read json");
    let v: Value = serde_json::from_str(&src).expect("parse json");
    v.get("fight")
        .and_then(|f| f.get("actions"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default()
}

fn opcode(v: &Value) -> Option<i64> {
    v.as_array().and_then(|a| a.first()).and_then(|x| x.as_i64())
}

/// Optional smoke test: run Java generator and ensure it produces output JSON.
/// Enable with `--features java-parity`.
#[test]
fn java_generator_runs_and_emits_actions() {
    // Feature-gated at compile time.

    let repo = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let ws_root = std::path::Path::new(&repo)
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    let java_dir = ws_root.join("leek-wars-generator");
    let scenario = java_dir.join("test/scenario/scenario1.json");
    let jar = java_dir.join("generator.jar");
    assert!(jar.is_file(), "missing generator.jar at {jar:?}");

    let out = ws_root.join("target/java_parity_outcome.json");
    let status = Command::new("java")
        .current_dir(&java_dir)
        .args(["-jar", jar.to_str().unwrap(), scenario.to_str().unwrap()])
        .stdout(std::fs::File::create(&out).unwrap())
        .status()
        .expect("run java");
    assert!(status.success(), "java generator failed");

    let actions = load_actions(out.to_str().unwrap());
    assert!(!actions.is_empty(), "java actions empty");
    // The very first action should be START_FIGHT (0) in Java.
    assert_eq!(opcode(&actions[0]), Some(0));
}

