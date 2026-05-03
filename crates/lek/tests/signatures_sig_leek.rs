//! `--signatures` with `.sig.leek` / `.sig.ls` via [`lek::signatures::collect_signature_globals`].

use lek::check::{check_one_file, default_registry_path, CheckOptions, CheckedFile};
use lek::signatures::collect_signature_globals;
use std::path::PathBuf;

fn temp_sig_leek(suffix: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "lek_sig_stub_{}_{}.sig.leek",
        std::process::id(),
        suffix
    ));
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn collect_sig_leek_merges_names() {
    let path = temp_sig_leek(
        "merge",
        "function lwOnlyFn() { }\nglobal LW_ONLY_GLOBAL = 0;\n",
    );
    let names = collect_signature_globals(None, &[path]).unwrap();
    assert!(names.iter().any(|n| n == "lwOnlyFn"));
    assert!(names.iter().any(|n| n == "LW_ONLY_GLOBAL"));
}

#[test]
fn check_uses_sig_leek_via_collect() {
    let reg = leekscript_diagnostics::Registry::load_path(default_registry_path()).unwrap();

    let sig_path = temp_sig_leek("partial", "function lwOnlyFn() { }\n");

    let src = concat!(
        "function turn() {\n",
        "  lwOnlyFn();\n",
        "  return LW_ONLY_GLOBAL;\n",
        "}\n",
    );
    let path = std::env::temp_dir().join(format!("lek_sig_leek_{}.leek", std::process::id()));
    std::fs::write(&path, src).unwrap();
    let src = std::fs::read_to_string(&path).unwrap();

    let globals = collect_signature_globals(None, std::slice::from_ref(&sig_path)).unwrap();
    let bad = check_one_file(
        &reg,
        &path,
        &src,
        &CheckOptions {
            signature_globals: globals,
            ..Default::default()
        },
    );
    assert!(
        matches!(bad, CheckedFile::Failed(_)),
        "LW_ONLY_GLOBAL still missing with only sig.leek fn"
    );

    std::fs::write(
        &sig_path,
        "function lwOnlyFn() { }\nglobal LW_ONLY_GLOBAL = 0;\n",
    )
    .unwrap();
    let globals = collect_signature_globals(None, &[sig_path]).unwrap();
    let ok = check_one_file(
        &reg,
        &path,
        &src,
        &CheckOptions {
            signature_globals: globals,
            ..Default::default()
        },
    );
    assert!(
        matches!(ok, CheckedFile::Ok(_)),
        "sig.leek with fn + global resolves: {ok:?}"
    );
}
