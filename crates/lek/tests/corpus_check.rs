//! Frozen corpus checks for `lek check` (full compile pipeline + registry codes).

use leekscript_diagnostics::Registry;
use lek::check::{check_one_file, default_registry_path, CheckOptions, CheckedFile, CheckedOk};
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture(name: &str) -> PathBuf {
    workspace_root().join("tests/fixtures").join(name)
}

fn load_registry() -> Registry {
    Registry::load_path(default_registry_path()).expect("repo registry.yaml")
}

#[test]
fn smoke_leek_is_clean() {
    let path = fixture("smoke.leek");
    let src = std::fs::read_to_string(&path).unwrap();
    let reg = load_registry();
    let opts = CheckOptions::default();
    match check_one_file(&reg, &path, &src, &opts) {
        CheckedFile::Ok(ok) => {
            assert!(ok.token_count >= 1);
            assert_eq!(ok.language_version, 4);
            assert_eq!(ok.hir_stmt_count, 1);
        }
        CheckedFile::Failed(_) => panic!("expected clean smoke.leek"),
    }
}

#[test]
fn unclosed_string_diagnostic() {
    let path = fixture("unclosed.leek");
    let src = std::fs::read_to_string(&path).unwrap();
    let reg = load_registry();
    let opts = CheckOptions::default();
    let CheckedFile::Failed(records) = check_one_file(&reg, &path, &src, &opts) else {
        panic!("expected failure");
    };
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert_eq!(r.phase, "lexer");
    assert_eq!(r.reference, "STRING_NOT_CLOSED");
    assert_eq!(r.code, "E0104");
    assert_eq!(r.line, 1);
}

#[test]
fn unclosed_paren_delimiter_diagnostic() {
    let path = fixture("unclosed_paren.leek");
    let src = std::fs::read_to_string(&path).unwrap();
    let reg = load_registry();
    let opts = CheckOptions::default();
    let CheckedFile::Failed(records) = check_one_file(&reg, &path, &src, &opts) else {
        panic!("expected failure");
    };
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert_eq!(r.phase, "parser");
    assert_eq!(r.reference, "END_OF_SCRIPT_UNEXPECTED");
    assert_eq!(r.code, "E0205");
}

#[test]
fn checked_ok_json_shape() {
    let ok = CheckedOk {
        path_display: "smoke.leek".into(),
        language_version: 4,
        strict: None,
        token_count: 6,
        hir_stmt_count: 1,
        fmt: None,
        experimental: None,
    };
    let v = serde_json::to_value(&ok).unwrap();
    assert_eq!(v["file"], "smoke.leek");
    assert_eq!(v["language_version"], 4);
    assert!(v["strict"].is_null());
    assert_eq!(v["token_count"], 6);
    assert_eq!(v["hir_stmt_count"], 1);
}
