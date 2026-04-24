//! Full normalized parity (Java `generator.jar` vs Rust) over a static corpus and a few generated scenarios.
//!
//! Requires `generator.jar`, `test/ai/basic.leek`, and data files under `leek-wars-generator/`.
//! `fight.ops` and top-level timings are stripped by [`leek_wars_gen::parity::normalize_outcome_json`].

use leek_wars_gen::engine::{
    default_java_cwd, resolve_generator_jar, JavaEngineConfig, RunRequest,
};
use leek_wars_gen::harness::{run_scenario_harness, CompareMode, HarnessRunConfig};
use std::fs;
use std::path::{Path, PathBuf};

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

fn generator_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator")
}

/// Checked-in scenarios relative to `leek-wars-generator/` (JVM cwd).
const STATIC_CORPUS: &[&str] = &[
    "test/scenario/scenario1.json",
    "test/scenario/parity_seed_424242.json",
    "test/scenario/parity_minimal_1v1.json",
    "test/scenario/parity_2v1.json",
];

fn assert_full_parity(rel: &str) {
    let jar = match resolve_generator_jar() {
        Ok(j) => j,
        Err(_) => return,
    };
    let cwd = default_java_cwd(&jar);
    if !cwd.join(rel).is_file() {
        return;
    }

    let req = RunRequest {
        file: PathBuf::from(rel),
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

    let report =
        run_scenario_harness(&req, &cfg).unwrap_or_else(|e| panic!("{rel}: harness error: {e}"));
    assert!(
        !report.comparison_failed(),
        "{}: compare {:?}",
        rel,
        report.compare
    );
}

#[test]
fn parity_static_corpus_matches_java() {
    for &rel in STATIC_CORPUS {
        assert_full_parity(rel);
    }
}

/// Deterministic “fuzz”: same1v1 layout as `parity_minimal_1v1.json` with varying `random_seed`.
#[test]
fn parity_generated_1v1_seeds_match_java() {
    let root = generator_repo_root();
    let gen_dir = root.join("test/scenario/generated");
    if resolve_generator_jar().is_err() {
        return;
    }
    if !root.join("test/ai/basic.leek").is_file() {
        return;
    }

    fs::create_dir_all(&gen_dir).expect("generated scenario dir");

    let seeds = [
        11_111, 222_222, 3_141_592, 9_000_001, 12_345_678, 98_765_431,
    ];
    for (i, &seed) in seeds.iter().enumerate() {
        let name = format!("autogen_1v1_{i}_{seed}.json");
        let path = gen_dir.join(&name);
        let json = serde_json::json!({
            "farmers": [
                {"id": 1, "name": "A", "country": "fr"},
                {"id": 2, "name": "B", "country": "fr"}
            ],
            "teams": [
                {"id": 1, "name": "TeamA"},
                {"id": 2, "name": "TeamB"}
            ],
            "entities": [
                [{
                    "id": 101,
                    "ai": "test/ai/basic.leek",
                    "name": "Alpha",
                    "type": 1,
                    "farmer": 1,
                    "team": 1,
                    "level": 200,
                    "life": 4000,
                    "strength": 300,
                    "cores": 10,
                    "tp": 15,
                    "mp": 7,
                    "cell": 123,
                    "weapons": [37, 47],
                    "chips": []
                }],
                [{
                    "id": 202,
                    "ai": "test/ai/basic.leek",
                    "name": "Beta",
                    "type": 1,
                    "farmer": 2,
                    "team": 2,
                    "level": 200,
                    "life": 4000,
                    "strength": 300,
                    "cores": 10,
                    "tp": 15,
                    "mp": 7,
                    "cell": 301,
                    "weapons": [47],
                    "chips": []
                }]
            ],
            "map": {"width": 17, "height": 17, "type": 3, "obstacles": []},
            "random_seed": seed,
            "max_turns": 40,
            "max_operations_per_entity": 20000000
        });
        fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).expect("write scenario");

        let rel = format!("test/scenario/generated/{name}");
        assert_full_parity(&rel);
        let _ = fs::remove_file(&path);
    }
}
