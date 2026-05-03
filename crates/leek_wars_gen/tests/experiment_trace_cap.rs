//! Trace respects `max_events` cap (Rust-only sidecar path).

use leek_wars_gen::fight::{run_scenario_path_with_options, FightRunOptions, TraceConfig};
use std::path::PathBuf;

#[test]
fn trace_respects_max_events() {
    let root = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../leek-wars-generator"
    ));
    let scenario = root.join("test/scenario/scenario1.json");
    if !scenario.is_file() {
        eprintln!("skip: scenario1.json not at {}", scenario.display());
        return;
    }
    let out = run_scenario_path_with_options(
        &scenario,
        &root,
        None,
        FightRunOptions {
            trace: Some(TraceConfig {
                enabled: true,
                max_events: 5,
            }),
            ..Default::default()
        },
    )
    .expect("fight");
    let ev = out.trace_events.expect("trace on");
    assert!(ev.len() <= 5, "got {} events", ev.len());
}
