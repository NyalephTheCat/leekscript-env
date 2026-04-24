//! Full normalized Java vs Rust parity for every top-level `test/scenario/*.json` fixture.

use leek_wars_gen::engine::{
    default_java_cwd, resolve_generator_jar, JavaEngineConfig, RunRequest,
};
use leek_wars_gen::harness::{
    discover_scenario_json_files, run_scenario_harness, CompareMode, CompareResult,
    HarnessRunConfig, INCOMPLETE_SCENARIO_BASELINES,
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
fn harness_all_checkout_scenarios_full_normalized_matches_java() {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let cwd = default_java_cwd(&jar);
    let scenarios =
        match discover_scenario_json_files(&cwd, std::path::Path::new("test/scenario"))
    {
        Ok(s) => s,
        Err(_) => return,
    };
    if scenarios.is_empty() {
        return;
    }
    // Keep this test strict: it asserts *full normalized parity* for the canonical checkout fixtures.
    // Any “wider fuzz corpus” scenarios should live under a different directory so they can be used
    // by fuzzers without blocking this regression test.
    let scenarios: Vec<_> = scenarios
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| !INCOMPLETE_SCENARIO_BASELINES.contains(&n))
                .unwrap_or(true)
        })
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n != "parity_1v1_alt_ai.json" && n != "parity_2v2_chips.json")
                .unwrap_or(true)
        })
        .collect();
    if scenarios.is_empty() {
        return;
    }

    let cfg = HarnessRunConfig {
        java: JavaEngineConfig {
            jar,
            cwd: cwd.clone(),
            java_bin: java_bin(),
        },
        mode: CompareMode::FullNormalized,
        warmup: 0,
        iterations: 1,
        run_java: true,
        run_rust: true,
        runtime_cwd: None,
    };

    for file in scenarios {
        let req = RunRequest {
            file: file.clone(),
            ..Default::default()
        };
        let report = run_scenario_harness(&req, &cfg).expect("harness");
        if report.comparison_failed() {
            let diff = match &report.compare {
                CompareResult::FullMismatch {
                    normalized_diff: Some(d),
                    ..
                } => d.as_str(),
                _ => "<no normalized_diff>",
            };
            panic!(
                "full normalized parity failed for {}.\n\
 --- diff (reference: rust [-] vs java [+], timing stripped) ---\n{diff}",
                file.display()
            );
        }
    }
}
