//! Compare `winner` (and `duration`) between the Java jar and the Rust engine on `scenario1`.
//!
//! This is a **narrow** parity signal: the Rust simulator does not yet reproduce map, weapons, or logs.

use leek_wars_gen::engine::{
    default_java_cwd, resolve_generator_jar, JavaEngine, JavaEngineConfig, RunRequest, RustEngine,
};
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
fn scenario1_java_and_rust_match_winner_and_duration() {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let cwd = default_java_cwd(&jar);
    if !cwd.join("test/scenario/scenario1.json").is_file() {
        return;
    }

    std::env::set_var("LEEK_GENERATOR_CWD", cwd.as_os_str());

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

    let j: serde_json::Value = serde_json::from_str(&java_out).unwrap();
    let r: serde_json::Value = serde_json::from_str(&rust_out).unwrap();
    assert_eq!(j["winner"], r["winner"], "winner");
    assert_eq!(j["duration"], r["duration"], "duration");
}
