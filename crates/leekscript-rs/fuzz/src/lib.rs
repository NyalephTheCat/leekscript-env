//! Shared helpers for LeekScript `cargo-fuzz` targets.

use leekscript::parse::{ExperimentalFeatures, LanguageOptions, Version, language_options_with_source_directives};
use std::hint::black_box;

/// Upper bound on UTF-8 input length passed into the library (keeps format / analysis predictable).
pub const MAX_INPUT_LEN: usize = 256 * 1024;

/// Turn arbitrary bytes into a `String` for parser entry points that take `&str`.
#[must_use]
pub fn bytes_to_string(data: &[u8]) -> String {
    let slice = if data.len() > MAX_INPUT_LEN {
        &data[..MAX_INPUT_LEN]
    } else {
        data
    };
    String::from_utf8_lossy(slice).into_owned()
}

/// Base dialect + experiments from a seed byte; merged with `//! leeklang:` / leading directives in `src`.
#[must_use]
pub fn language_options_for_fuzz(seed: u8, src: &str) -> LanguageOptions {
    let version = match seed % 4 {
        0 => Version::V1,
        1 => Version::V2,
        2 => Version::V3,
        _ => Version::V4,
    };
    let mut base = LanguageOptions::new(version, ExperimentalFeatures::NONE);
    if seed & 0b1000_0000 != 0 {
        base.experimental = ExperimentalFeatures::ALL;
    }
    language_options_with_source_directives(src, base)
}

/// Feed values to the fuzzer so statement boundaries are not dead-stripped when optimizing.
#[inline]
pub fn touch_u64(x: u64) {
    black_box(x);
}
