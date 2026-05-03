//! `lek run` executes HIR after a successful compile.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn run_reports_return_value() {
    let exe = env!("CARGO_BIN_EXE_lek");
    let path = workspace_root().join("tests/fixtures/return_sum.leek");
    let out = Command::new(exe)
        .args(["run", path.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("result:") && stderr.contains('2'),
        "expected result 2 in: {stderr}"
    );
}

#[test]
fn run_runtime_error_exits_nonzero() {
    let exe = env!("CARGO_BIN_EXE_lek");
    let dir = std::env::temp_dir().join(format!("lek-run-err-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("bad.leek");
    std::fs::write(&p, "return nope;\n").unwrap();
    let out = Command::new(exe)
        .args(["run", p.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("E1002") && stderr.contains("VARIABLE_NOT_EXISTS"),
        "expected registry-backed code and reference, got: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
