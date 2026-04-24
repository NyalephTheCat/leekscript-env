//! Smoke-test the Rust fight engine on the official `scenario1` fixture (requires repo layout).

use leek_wars_gen::engine::RunRequest;
use leek_wars_gen::engine::RustEngine;
use std::path::PathBuf;

#[test]
fn rust_engine_runs_scenario1() {
    let cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator");
    if !cwd.join("test/scenario/scenario1.json").is_file() {
        eprintln!("skip: leek-wars-generator fixture not present");
        return;
    }
    std::env::set_var("LEEK_GENERATOR_CWD", cwd.as_os_str());
    let req = RunRequest {
        file: PathBuf::from("test/scenario/scenario1.json"),
        ..Default::default()
    };
    let json = RustEngine
        .run_scenario(&req)
        .expect("rust engine should complete scenario1");
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(v.get("fight").is_some());
    assert!(v.get("winner").is_some());
    assert_eq!(v["duration"], 65);
}
