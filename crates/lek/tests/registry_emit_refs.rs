//! `lek registry --verify-emit-refs` must succeed against the repo registry.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn registry_verify_emit_refs_ok() {
    let exe = env!("CARGO_BIN_EXE_lek");
    let mut reg = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    reg.pop();
    reg.pop();
    reg.push("data/diagnostics/registry.yaml");
    let out = Command::new(exe)
        .args([
            "registry",
            "--verify-emit-refs",
            "--path",
            reg.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
