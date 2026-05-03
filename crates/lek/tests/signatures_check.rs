//! `signature_globals` on [`lek::check::CheckOptions`] feeds the resolve pass.

use leekscript_signatures::SignatureFile;
use lek::check::{check_one_file, default_registry_path, CheckOptions, CheckedFile};

#[test]
fn check_uses_signature_globals_for_resolve() {
    let reg = leekscript_diagnostics::Registry::load_path(default_registry_path()).unwrap();
    let sig: SignatureFile = r#"
schema_version = 1
functions = ["lwOnlyFn"]
globals = ["LW_ONLY_GLOBAL"]
"#
    .parse()
    .unwrap();
    let src = concat!(
        "function turn() {\n",
        "  lwOnlyFn();\n",
        "  return LW_ONLY_GLOBAL;\n",
        "}\n",
    );
    let path = std::env::temp_dir().join(format!("lek_sig_ai_{}.leek", std::process::id()));
    std::fs::write(&path, src).unwrap();
    let src = std::fs::read_to_string(&path).unwrap();
    let bad = check_one_file(
        &reg,
        &path,
        &src,
        &CheckOptions {
            signature_globals: vec![],
            ..Default::default()
        },
    );
    assert!(
        matches!(bad, CheckedFile::Failed(_)),
        "without signatures, lwOnlyFn / LW_ONLY_GLOBAL are unknown"
    );

    let ok = check_one_file(
        &reg,
        &path,
        &src,
        &CheckOptions {
            signature_globals: sig.resolve_names(),
            ..Default::default()
        },
    );
    assert!(
        matches!(ok, CheckedFile::Ok(_)),
        "with signatures, resolve succeeds: {ok:?}"
    );
}
