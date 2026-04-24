#![no_main]

use libfuzzer_sys::fuzz_target;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use serde_json::Value;

use leek_wars_gen::fight::{run_scenario_path, run_scenario_path_with_ai_overlay};
use leek_wars_gen::fuzz::{
    apply_fuzz_draw_check_life, apply_fuzz_entity_cells, apply_fuzz_entity_loadouts,
    apply_fuzz_entity_stats, apply_fuzz_map_obstacles, apply_fuzz_max_operations_per_entity,
    apply_fuzz_max_turns, apply_fuzz_variants, discover_ai_rel_paths, merge_parity_corpus_scenarios,
};
use leek_wars_gen::fuzz_input::FuzzInput;

fn default_generator_root() -> std::path::PathBuf {
    // In-repo default: `<workspace>/leek-wars-generator`
    // (when running via `cargo fuzz`, cwd is `fuzz/`, so go one level up).
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("leek-wars-generator")
}

fn generator_root() -> std::path::PathBuf {
    std::env::var_os("LEEK_GENERATOR_CWD")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_generator_root)
}

fn read_scenario_json(root: &std::path::Path, rel: &std::path::Path) -> Option<Value> {
    let raw = std::fs::read_to_string(root.join(rel)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_temp_scenario(doc: &Value, seed: u64) -> Option<std::path::PathBuf> {
    let tmp = std::env::temp_dir().join(format!("leek_wars_gen_fuzz_{seed:016x}.json"));
    let s = serde_json::to_string(doc).ok()?;
    std::fs::write(&tmp, s).ok()?;
    Some(tmp)
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&p, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&p, &to)?;
        }
    }
    Ok(())
}

fuzz_target!(|data: &[u8]| {
    // Keep the harness bounded: inputs are a fixed header + small knobs.
    if data.len() > 1024 {
        return;
    }

    let root = generator_root();
    if !root.is_dir() {
        return;
    }

    let input = FuzzInput::from_bytes(data);
    let mut rng = StdRng::seed_from_u64(input.seed);

    // Pick a scenario template from a small, stable corpus (only those that exist).
    let mut scenario_rels: Vec<std::path::PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    merge_parity_corpus_scenarios(&root, &mut scenario_rels, &mut seen);
    if scenario_rels.is_empty() {
        return;
    }
    let template = &scenario_rels[(rng.next_u64() as usize) % scenario_rels.len()];

    let mut doc = match read_scenario_json(&root, template) {
        Some(d) => d,
        None => return,
    };

    // AI pool: allow external AI paths only when explicitly rolled in the input,
    // otherwise keep the fuzz stable to the scenario-provided AIs.
    let ai_rels = discover_ai_rel_paths(&root, std::path::Path::new("test/ai")).unwrap_or_default();
    let ai_pool = leek_wars_gen::fuzz::ai_pool_from_document(&doc, &ai_rels);

    apply_fuzz_variants(&mut doc, &mut rng, &ai_pool, &input);
    if input.roll(&mut rng, input.p_jitter_entity_stats) {
        apply_fuzz_entity_stats(&mut doc, &mut rng, input.mag_entity_stats);
    }
    if input.roll(&mut rng, input.p_jitter_max_turns) {
        apply_fuzz_max_turns(&mut doc, &mut rng, input.mag_max_turns);
    }
    if input.roll(&mut rng, input.p_randomize_draw_rule) {
        apply_fuzz_draw_check_life(&mut doc, &mut rng);
    }
    if input.roll(&mut rng, input.p_jitter_map_obstacles) {
        apply_fuzz_map_obstacles(&mut doc, &mut rng);
    }
    if input.roll(&mut rng, input.p_jitter_entity_cells) {
        apply_fuzz_entity_cells(&mut doc, &mut rng);
    }
    if input.roll(&mut rng, input.p_jitter_max_operations) {
        apply_fuzz_max_operations_per_entity(&mut doc, &mut rng);
    }
    if input.roll(&mut rng, input.p_jitter_entity_loadouts) {
        apply_fuzz_entity_loadouts(&mut doc, &mut rng);
    }

    let tmp = match write_temp_scenario(&doc, input.seed) {
        Some(p) => p,
        None => return,
    };

    // Exercise both code paths: with and without AI overlay mutation.
    // (Overlay is Rust-only; full Java parity fuzzing is handled by the CLI and the repro minimizer target.)
    if input.mutate_ai_level == 0 {
        let _ = run_scenario_path(&tmp, &root);
    } else {
        let overlay_dir = std::env::temp_dir().join(format!(
            "leek_wars_gen_fuzz_ai_{:016x}",
            input.seed ^ 0x9e3779b97f4a7c15
        ));
        let _ = std::fs::remove_dir_all(&overlay_dir);
        // Best-effort mirror; if it fails, just run without overlay.
        if copy_dir_recursive(&root.join("test/ai"), &overlay_dir.join("test/ai")).is_ok() {
            let _ = run_scenario_path_with_ai_overlay(&tmp, &root, Some(&overlay_dir));
        } else {
            let _ = run_scenario_path(&tmp, &root);
        }
        let _ = std::fs::remove_dir_all(&overlay_dir);
    }

    let _ = std::fs::remove_file(&tmp);
});

