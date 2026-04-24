//! Integration: two Java runs with the same scenario match after normalizing timing fields.
//!
//! Skips if `generator.jar` or Java is unavailable.

use leek_wars_gen::engine::{
    default_java_cwd, resolve_generator_jar, JavaEngine, JavaEngineConfig, RunRequest,
};
use leek_wars_gen::parity::outcomes_equal_ignore_timing;
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

#[test]
fn java_generator_is_stable_modulo_timing() {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let cwd = default_java_cwd(&jar);
    if !cwd.is_dir() {
        return;
    }
    let scenario = cwd.join("test/scenario/scenario1.json");
    if !scenario.is_file() {
        eprintln!("skip: missing {}", scenario.display());
        return;
    }

    let cfg = JavaEngineConfig {
        jar,
        cwd,
        java_bin: java_bin(),
    };
    let engine = JavaEngine::new(cfg);
    let req = RunRequest {
        file: PathBuf::from("test/scenario/scenario1.json"),
        ..Default::default()
    };
    let a = match engine.run(&req) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("skip: java run failed: {}", e);
            return;
        }
    };
    let b = engine.run(&req).expect("second run");
    assert!(
        outcomes_equal_ignore_timing(&a, &b).expect("json"),
        "normalized outcomes should match"
    );
}
