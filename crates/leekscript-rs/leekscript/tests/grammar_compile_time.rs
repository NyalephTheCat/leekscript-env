#![cfg(feature = "compile-time-grammar")]

//! Integration checks when building with `--features compile-time-grammar` (e.g. in CI).

#[test]
fn compile_time_grammar_feature_matches_build_script_cfg() {
    assert!(
        leekscript::COMPILE_TIME_GRAMMAR,
        "enable `compile-time-grammar` when running this test crate"
    );
}

#[test]
fn grammar_source_fingerprint_is_nonzero() {
    assert_ne!(leekscript::GRAMMAR_SOURCE_FINGERPRINT, 0);
}

#[test]
fn built_graph_materializes_with_bytecode() {
    let g = leekscript::grammar::built_graph();
    assert!(!g.insns.is_empty(), "grammar should produce VM insns");
}
