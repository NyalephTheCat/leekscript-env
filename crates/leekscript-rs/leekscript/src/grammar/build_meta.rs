//! Build-time grammar metadata from `build.rs` (`OUT_DIR/leekscript_grammar_fingerprint.rs`).

include!(concat!(
    env!("OUT_DIR"),
    "/leekscript_grammar_fingerprint.rs"
));

/// `true` when crate was built with feature `compile-time-grammar` (build script sets
/// `leekscript_compile_time_grammar`).
pub const COMPILE_TIME_GRAMMAR: bool = cfg!(leekscript_compile_time_grammar);

#[cfg(all(feature = "compile-time-grammar", leekscript_compile_time_grammar))]
const _: () = assert!(GRAMMAR_SOURCE_FINGERPRINT != 0);
