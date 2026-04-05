//! Declarative macros for wiring the sipha grammar (see [`crate::grammar::built_graph`]).
//!
//! Kept separate so `grammar/mod.rs` stays a thin init layer (`LazyLock`, fingerprint re-exports).

/// Expands to a [`BuiltGraph`](sipha::parse::builder::BuiltGraph) built with
/// `grammar::configure::configure_builder` and all LeekScript rule modules.
///
/// Sipha’s **optimize-graph** pass runs inside [`GrammarBuilder::finish`]. The parse VM then uses
/// staged opcode dispatch on that bytecode. The graph is still materialized when this expression
/// runs (e.g. on first [`super::built_graph`] access), not during `rustc` — see Cargo feature
/// `compile-time-grammar` for build-time fingerprinting of grammar sources.
#[macro_export]
macro_rules! leekscript_build_grammar {
    () => {{
        let mut g = sipha::prelude::GrammarBuilder::new();
        $crate::grammar::configure::configure_builder(&mut g);
        $crate::grammar::stmt::define(&mut g);
        $crate::grammar::tokens::define(&mut g);
        $crate::grammar::r#type::define(&mut g);
        $crate::grammar::expr::define(&mut g);
        g.finish().expect("leekscript grammar should build")
    }};
}

/// Runs [`GrammarBuilder::finish`](sipha::prelude::GrammarBuilder::finish). After
/// `grammar::configure::configure_builder`, Sipha’s bytecode optimizer runs inside `finish`
/// before the graph is frozen.
#[macro_export]
macro_rules! leekscript_finish_optimized_grammar {
    ($g:expr) => {
        $g.finish().expect("leekscript grammar should build")
    };
}
