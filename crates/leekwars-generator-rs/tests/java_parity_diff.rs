#![cfg(feature = "java-parity")]

use std::process::Command;

use serde_json::Value;

fn load_actions(path: &std::path::Path) -> Vec<Value> {
    let src = std::fs::read_to_string(path).expect("read json");
    let v: Value = serde_json::from_str(&src).expect("parse json");
    v.get("fight")
        .and_then(|f| f.get("actions"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default()
}

fn opcodes(actions: &[Value]) -> Vec<Option<i64>> {
    actions
        .iter()
        .map(|v| v.as_array().and_then(|a| a.first()).and_then(|x| x.as_i64()))
        .collect()
}

fn normalize_actions(actions: Vec<Value>) -> Vec<Value> {
    actions
        .into_iter()
        .filter(|v| v.as_array().and_then(|a| a.first()).and_then(|x| x.as_i64()).is_some())
        // Known noise: SAY (203) is very scenario/stdlib dependent and not a core parity signal yet.
        .filter(|v| v.as_array().and_then(|a| a.first()).and_then(|x| x.as_i64()) != Some(203))
        // MOVE_TO (10) contains a full path array which is highly tie-break sensitive in A*.
        // For combat-rule parity we only care about the start/end cells.
        .map(|v| {
            let Some(arr) = v.as_array() else { return v };
            if arr.first().and_then(|x| x.as_i64()) != Some(10) {
                return v;
            }
            if arr.len() < 3 {
                return v;
            }
            Value::Array(vec![arr[0].clone(), arr[1].clone(), arr[2].clone()])
        })
        .collect()
}

/// On-demand diff helper.
///
/// Run with:
/// - `cargo test -p leekwars-generator-rs --features java-parity --test java_parity_diff -- --ignored`
#[test]
#[ignore]
fn diff_java_and_rust_opcode_streams() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ws_root = manifest_dir.parent().and_then(|p| p.parent()).expect("workspace root");

    let java_dir = ws_root.join("leek-wars-generator");
    let scenario = java_dir.join("test/scenario/scenario1.json");
    let jar = java_dir.join("generator.jar");
    assert!(jar.is_file(), "missing generator.jar at {jar:?}");

    let out_dir = ws_root.join("target/java_parity");
    std::fs::create_dir_all(&out_dir).expect("create out dir");
    let java_out = out_dir.join("java_outcome.json");
    let rust_out = out_dir.join("rust_outcome.json");

    // Java: must run with cwd set to leek-wars-generator so relative data/ paths resolve.
    let status = Command::new("java")
        .current_dir(&java_dir)
        .args(["-jar", jar.to_str().unwrap(), scenario.to_str().unwrap()])
        .stdout(std::fs::File::create(&java_out).unwrap())
        .status()
        .expect("run java");
    assert!(status.success(), "java generator failed");

    // Rust: run the compiled binary produced by cargo for this workspace.
    let rust_bin = std::env::var("CARGO_BIN_EXE_leekwars-generator").ok().map(std::path::PathBuf::from).unwrap_or_else(|| {
        // When running as an ignored test, Cargo doesn't always provide CARGO_BIN_EXE_* env vars.
        // Fall back to `target/debug/leekwars-generator`, building it if needed.
        let candidate = ws_root.join("target").join("debug").join("leekwars-generator");
        if candidate.is_file() {
            return candidate;
        }
        let status = Command::new("cargo")
            .current_dir(ws_root)
            .args(["build", "-q", "-p", "leekwars-generator-rs", "--bin", "leekwars-generator"])
            .status()
            .expect("build rust generator");
        assert!(status.success(), "cargo build for rust generator failed");
        candidate
    });
    let status = Command::new(rust_bin)
        .current_dir(ws_root)
        .arg(scenario.to_str().unwrap())
        // Provide reference world so Rust can isolate combat-rule diffs.
        .env("LW_REFERENCE_OUTCOME", java_out.to_str().unwrap())
        .stdout(std::fs::File::create(&rust_out).unwrap())
        .status()
        .expect("run rust");
    assert!(status.success(), "rust generator failed");

    let ja = normalize_actions(load_actions(&java_out));
    let ra = normalize_actions(load_actions(&rust_out));
    assert!(!ja.is_empty(), "java actions empty");
    assert!(!ra.is_empty(), "rust actions empty");

    let jo = opcodes(&ja);
    let ro = opcodes(&ra);

    let m = jo.len().min(ro.len());
    let mut mismatch = None;
    for i in 0..m {
        if jo[i] != ro[i] {
            mismatch = Some(i);
            break;
        }
    }

    if let Some(i) = mismatch {
        panic!(
            "opcode mismatch at index {i}\njava: {:?}\nrust: {:?}\n(java_len={}, rust_len={})\noutputs: java={}, rust={}",
            ja.get(i),
            ra.get(i),
            ja.len(),
            ra.len(),
            java_out.display(),
            rust_out.display(),
        );
    }

    if jo.len() != ro.len() {
        panic!(
            "opcode stream length mismatch at {m} (java_len={}, rust_len={})\noutputs: java={}, rust={}",
            jo.len(),
            ro.len(),
            java_out.display(),
            rust_out.display(),
        );
    }
}

