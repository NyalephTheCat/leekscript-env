use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn check_stdin_json_lists_file_as_dash() {
    let exe =
        option_env!("CARGO_BIN_EXE_lek").expect("lek binary must be built (cargo test -p lek)");
    let mut child = Command::new(exe)
        .args(["check", "--message-format", "json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lek");
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"var x = 0;\n").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema_version"], 4);
    assert_eq!(v["command"], "check");
    assert_eq!(v["files"][0]["file"], "-");
    assert_eq!(v["files"][0]["status"], "ok");
}

#[test]
fn check_stdin_stdin_path_labels_diagnostics() {
    let exe = option_env!("CARGO_BIN_EXE_lek").expect("lek binary");
    let mut child = Command::new(exe)
        .args([
            "check",
            "--message-format",
            "json",
            "--stdin-path",
            "/workspace/src/foo.leek",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn lek");
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"var x = 0;\n").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["files"][0]["file"], "/workspace/src/foo.leek");
}
