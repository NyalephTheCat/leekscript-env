//! Library harness on `scenario1` (skipped when `generator.jar` / layout missing).

use leek_wars_gen::engine::{
    default_java_cwd, resolve_generator_jar, JavaEngineConfig, RunRequest,
};
use leek_wars_gen::harness::{run_scenario_harness, CompareMode, CompareResult, HarnessRunConfig};
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
fn harness_scenario1_winner_mode_matches() {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let cwd = default_java_cwd(&jar);
    if !cwd.join("test/scenario/scenario1.json").is_file() {
        return;
    }

    let req = RunRequest {
        file: PathBuf::from("test/scenario/scenario1.json"),
        ..Default::default()
    };

    let cfg = HarnessRunConfig {
        java: JavaEngineConfig {
            jar,
            cwd,
            java_bin: java_bin(),
        },
        mode: CompareMode::WinnerDuration,
        warmup: 0,
        iterations: 1,
        run_java: true,
        run_rust: true,
        runtime_cwd: None,
    };

    let report = run_scenario_harness(&req, &cfg).expect("harness");
    assert!(
        !report.comparison_failed(),
        "harness compare: {:?}",
        report.compare
    );
}

/// Full outcome JSON (timing stripped) must match Java (`generator.jar` with `DumpStateRng.outcomeObstacles`).
#[test]
fn harness_scenario1_full_normalized_matches_java() {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let cwd = default_java_cwd(&jar);
    if !cwd.join("test/scenario/scenario1.json").is_file() {
        return;
    }

    let req = RunRequest {
        file: PathBuf::from("test/scenario/scenario1.json"),
        ..Default::default()
    };

    let cfg = HarnessRunConfig {
        java: JavaEngineConfig {
            jar,
            cwd,
            java_bin: java_bin(),
        },
        mode: CompareMode::FullNormalized,
        warmup: 0,
        iterations: 1,
        run_java: true,
        run_rust: true,
        runtime_cwd: None,
    };

    let report = run_scenario_harness(&req, &cfg).expect("harness");
    if report.comparison_failed() {
        let engine = report.engine_errors_display().unwrap_or_default();
        let diff = match &report.compare {
            CompareResult::FullMismatch {
                normalized_diff: Some(d),
                ..
            } => d.as_str(),
            _ => "<no normalized_diff>",
        };
        panic!(
            "full normalized parity failed for scenario1.\n\
 --- engine errors (Java vs Rust) ---\n{engine}\n\
 --- diff (reference: rust [-] vs java [+], timing stripped) ---\n{diff}\n\
 --- compare summary ---\n{cmp:#?}",
            cmp = report.compare
        );
    }
}
