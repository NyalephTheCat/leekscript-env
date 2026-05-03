//! Example: time Java `--analyze` vs Rust `compile_ai_file` on the same `.leek`.
//!
//! Run from workspace root:
//! `cargo run -p leek_wars_gen --example compare_rust_java_compile --release -- \
//!     /path/to/leek-wars-generator/test/ai/basic.leek`
//!
//! Requires `JAVA_HOME` or `java` on `PATH` and a resolvable `generator.jar`.

use leek_wars_gen::engine::{
    default_java_cwd, resolve_generator_jar, JavaEngine, JavaEngineConfig, RunRequest, RustEngine,
};
use std::env;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("pass path to .leek");
    let jar = resolve_generator_jar().expect("jar");
    let cwd = default_java_cwd(&jar);
    let cfg = JavaEngineConfig {
        jar,
        cwd: cwd.clone(),
        java_bin: env::var_os("JAVA_HOME")
            .map(PathBuf::from)
            .map(|mut p| {
                p.push("bin/java");
                p
            })
            .filter(|p| p.is_file())
            .unwrap_or_else(|| PathBuf::from("java")),
    };
    let engine = JavaEngine::new(cfg);
    let rel = path
        .strip_prefix(&cwd)
        .unwrap_or(&path)
        .display()
        .to_string();
    let req = RunRequest {
        analyze: true,
        file: PathBuf::from(rel),
        ..Default::default()
    };

    let t0 = Instant::now();
    let _java_out = engine.run(&req).expect("java analyze");
    let java_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t1 = Instant::now();
    RustEngine
        .compile_ai_file(&path)
        .expect("rust compile_ai_file");
    let rust_ms = t1.elapsed().as_secs_f64() * 1000.0;

    eprintln!("java --analyze: {java_ms:8.2} ms");
    eprintln!("rust compile:     {rust_ms:8.2} ms");
}
