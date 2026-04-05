//! LeekScript CST grammar for sipha ([`GrammarBuilder`](sipha::prelude::GrammarBuilder)).
//!
//! Rule registration order in [`leekscript_build_grammar!`] is intentional: `stmt` → `tokens` →
//! `type` → `expr` (forward references resolved at [`GrammarBuilder::finish`](sipha::prelude::GrammarBuilder::finish)).
//!
//! [`configure::configure_builder`] sets trivia, recursion, and bytecode optimization.
//! [`built_graph`] freezes the result once per process.

mod build_meta;
mod cfg_flags;
mod configure;
mod expr;
mod lexer_keyword_batch;
mod lexer_rules;
mod macros;
mod rules;
mod stmt;
#[macro_use]
mod token_macros;
mod tokens;
mod r#type;

pub use build_meta::{COMPILE_TIME_GRAMMAR, GRAMMAR_SOURCE_FINGERPRINT};
pub use rules::GRule;

use sipha::prelude::*;
use std::sync::LazyLock;

static BUILT: LazyLock<BuiltGraph> = LazyLock::new(|| crate::leekscript_build_grammar!());

/// Lazily-built grammar graph for this crate.
///
/// The graph is built **once per process** on first access. [`GRAMMAR_SOURCE_FINGERPRINT`] reflects
/// grammar sources at **Cargo build** time. With feature `compile-time-grammar`, enable
/// `cfg(leekscript_compile_time_grammar)` and run `cargo test -p leekscript --features compile-time-grammar`
/// in CI for extra checks.
///
/// Pass the returned reference to [`crate::parse::parse_doc_with_built`] when parsing many
/// sources so you avoid repeating the static lookup.
pub fn built_graph() -> &'static BuiltGraph {
    &BUILT
}
