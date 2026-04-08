#![cfg(feature = "toml")]

use std::path::PathBuf;

use leekwars_generator_rs::BatchRunner;

fn batch_configs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("batch-configs")
}

#[test]
fn sample_batch_toml_files_parse() {
    let dir = batch_configs_dir();
    for name in [
        "scenarios",
        "sweep-variants",
        "sweep-loadout-and-talents",
        "sweep-cartesian-mini",
        "round-robin-ais",
        "versus-enemies",
    ] {
        let p = dir.join(format!("{name}.toml"));
        BatchRunner::load_job_from_file(&p)
            .unwrap_or_else(|e| panic!("failed to load {}: {e}", p.display()));
    }
}
