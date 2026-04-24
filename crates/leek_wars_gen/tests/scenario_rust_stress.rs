//! Rust fight engine smoke: all checkout scenarios, plus deterministic `random_seed` variants.

use leek_wars_gen::fight::run_scenario_path;
use leek_wars_gen::harness::{discover_scenario_json_files, INCOMPLETE_SCENARIO_BASELINES};
use std::path::{Path, PathBuf};

fn generator_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../leek-wars-generator")
}

#[test]
fn rust_engine_runs_all_checkout_scenarios() {
    let base = generator_root();
    if !base.is_dir() {
        return;
    }
    if !base.join("test/scenario").is_dir() {
        return;
    }
    let rels =
        discover_scenario_json_files(&base, Path::new("test/scenario")).expect("list scenarios");
    assert!(
        !rels.is_empty(),
        "expected test/scenario/*.json under {}",
        base.display()
    );
    for rel in rels {
        let skip = rel
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| INCOMPLETE_SCENARIO_BASELINES.contains(&n));
        if skip {
            continue;
        }
        run_scenario_path(&rel, &base).unwrap_or_else(|e| {
            panic!("Rust engine failed for {}: {e:?}", rel.display());
        });
    }
}

#[test]
fn rust_engine_scenario1_random_seeds_smoke() {
    let base = generator_root();
    let scenario1 = base.join("test/scenario/scenario1.json");
    if !scenario1.is_file() || !base.is_dir() {
        return;
    }
    let raw = std::fs::read_to_string(&scenario1).expect("read scenario1");
    let mut v: serde_json::Value = serde_json::from_str(&raw).expect("parse scenario1");
    let tmp = std::env::temp_dir();
    let pid = std::process::id();

    for i in 0u32..64 {
        let seed = (i.wrapping_mul(1_103_515_245)).wrapping_add(1_234_567) as i32;
        v["random_seed"] = serde_json::json!(seed);
        let path = tmp.join(format!("leek_wars_gen_seed_smoke_{pid}_{i}.json"));
        std::fs::write(&path, serde_json::to_string(&v).expect("serialize")).expect("write temp");
        run_scenario_path(&path, &base)
            .unwrap_or_else(|e| panic!("Rust engine failed for random_seed={seed}: {e:?}"));
        let _ = std::fs::remove_file(&path);
    }
}
