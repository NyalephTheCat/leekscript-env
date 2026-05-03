use std::process::Command;

#[test]
fn init_writes_valid_manifest_and_example() {
    let exe = env!("CARGO_BIN_EXE_lek");
    let dir = std::env::temp_dir().join(format!("lek-init-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let status = Command::new(exe)
        .args(["init"])
        .current_dir(&dir)
        .status()
        .expect("spawn lek init");
    assert!(status.success());

    let manifest = dir.join("Leek.toml");
    let s = std::fs::read_to_string(&manifest).unwrap();
    s.parse::<leekscript_config::LeekManifest>()
        .expect("valid Leek.toml");

    let example = dir.join("example.leek");
    assert!(example.is_file());
    let ex = std::fs::read_to_string(&example).unwrap();
    assert!(ex.contains("var x"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_no_example_skips_leek_file() {
    let exe = env!("CARGO_BIN_EXE_lek");
    let dir = std::env::temp_dir().join(format!("lek-init-noex-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let status = Command::new(exe)
        .args(["init", "--no-example"])
        .current_dir(&dir)
        .status()
        .expect("spawn");
    assert!(status.success());
    assert!(!dir.join("example.leek").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_refuses_existing_without_force() {
    let exe = env!("CARGO_BIN_EXE_lek");
    let dir = std::env::temp_dir().join(format!("lek-init-exists-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Leek.toml"), b"schema_version = 1\n").unwrap();

    let status = Command::new(exe)
        .args(["init"])
        .current_dir(&dir)
        .status()
        .expect("spawn");
    assert!(!status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
