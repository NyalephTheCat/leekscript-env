#![no_main]

use libfuzzer_sys::fuzz_target;

// This target is designed for *minimization* of existing repro bundles.
//
// Usage:
// - Create an artifact bundle with `leekgen-compare --fuzz --fuzz-artifacts-dir ...`
// - Set:
//     - `LEEKGEN_REPRO_DIR` to that artifact directory
//     - `LEEK_GENERATOR_CWD` so the generator checkout is available
//
// The fuzzer input bytes override the recorded `fuzz_input_hex`. A panic indicates the
// failure still reproduces (engine error or parity mismatch), so libFuzzer will minimize.
fuzz_target!(|data: &[u8]| {
    let repro_dir = match std::env::var_os("LEEKGEN_REPRO_DIR") {
        Some(v) => std::path::PathBuf::from(v),
        None => return,
    };
    let gen_root = match std::env::var_os("LEEK_GENERATOR_CWD") {
        Some(v) => std::path::PathBuf::from(v),
        None => return,
    };
    if !repro_dir.is_dir() || !gen_root.is_dir() {
        return;
    }

    let input = leek_wars_gen::fuzz_input::FuzzInput::from_bytes(data);
    let res = leek_wars_gen::fuzz::replay_fuzz_artifact_dir_with_input(
        &repro_dir,
        &gen_root,
        None,
        false,
        Some(&input),
    );
    if res.is_err() {
        panic!("repro persists");
    }
});

