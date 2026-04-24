//! `lek fmt` integration tests.

use std::path::PathBuf;
use std::process::Command;

fn lek() -> Command {
    let exe = option_env!("CARGO_BIN_EXE_lek").expect("lek binary");
    let mut c = Command::new(exe);
    c.current_dir(env!("CARGO_MANIFEST_DIR"));
    c
}

#[test]
fn fmt_smoke_fixture_stdout() {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    let smoke = root.join("tests/fixtures/smoke.leek");
    let out = lek()
        .args(["fmt", smoke.to_str().unwrap()])
        .output()
        .expect("run lek fmt");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("var x = 1;"), "got: {s:?}");
}

#[test]
fn fmt_check_clean_exits_zero() {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    let smoke = root.join("tests/fixtures/smoke.leek");
    let out = lek()
        .args(["fmt", "--check", smoke.to_str().unwrap()])
        .output()
        .expect("run lek fmt --check");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn fmt_check_dirty_exits_nonzero() {
    let path = std::env::temp_dir().join(format!("lek_fmt_dirty_{}.leek", std::process::id()));
    std::fs::write(&path, "var  x=1;\n").unwrap();
    let out = lek()
        .args(["fmt", "--check", path.to_str().unwrap()])
        .output()
        .expect("run lek fmt --check");
    let _ = std::fs::remove_file(&path);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("would reformat") || !stderr.is_empty(),
        "stderr={stderr}"
    );
}
