//! LeekScript parser (sipha-backed).
//!
//! ## Editing and transforming the CST
//!
//! Use [`LeekDoc`] for a parse result whose tree and [`LeekDoc::source`] buffer stay aligned.
//! [`visit`] documents walking with sipha’s [`Visitor`](sipha::tree::walk::Visitor), resolving a
//! typed node at a byte offset via [`visit::typed_at_offset`], splicing text with
//! [`LeekDoc::replace_span`], or rewriting with [`LeekDoc::apply_transform`] / [`LeekDoc::set_syntax_root`].
//! Top-level [`include`] paths are resolved like the reference compiler’s `Folder.resolve`; use
//! [`load_project_with_includes`] to parse an entry file and all transitively included sources.
//! [`merge_included_sources_to_single_file`] expands those includes into one source string with
//! per-file metadata comments and skips duplicate includes (diamond graphs).
//! Inclusion depth is capped by [`IncludeLimits`] in [`include`] (default matches the reference implementation).
//!
//! ## Scope extents
//!
//! [`scope`] assigns stable [`scope::ScopeId`] values, maps byte offsets to the innermost
//! lexical scope, and records variables / functions / classes with source-level types and
//! signatures (requires the `walk` feature, on by default).
//!
//! ## Formatting
//!
//! [`format`] is a configurable pretty-printer with `// leekfmt:`, `//! leekfmt:` (file-wide options),
//! and `/* leekfmt: */` directive comments for options and verbatim regions.
//!
//! ## Per-file dialect (`leeklang:`)
//!
//! Leading comments can set the parse dialect and experimental flags before any other tokens:
//! `// leeklang: version=v4 experimental-let=true` (also `//! …`, block `/* … */`, and `dialect=`).
//! Default values on `function (…)` parameters are part of dialect v4; optional flags include
//! `experimental-templates` (or `experimental-generics`).
//! See [`parse::language_options_with_source_directives`]. Merged buffers only honor directives at
//! the very start of the combined source.
//!
//! ## Bytecode VM (experimental)
//!
//! [`vm`] is a table-driven stack VM (`opcode → handler` via [`vm::DISPATCH`]) for running a small
//! LeekScript subset; the Java reference compiler targets the JVM instead. See [`vm::compile_chunk_v4`].
//!
//! ## Grammar graph (`sipha`)
//!
//! The CST grammar lives in [`grammar`] and becomes a [`BuiltGraph`] via [`leekscript_build_grammar!`].
//! Sipha’s `optimize_graph` pass runs inside [`GrammarBuilder::finish`](sipha::prelude::GrammarBuilder::finish).
//! [`GRAMMAR_SOURCE_FINGERPRINT`] is fixed when this crate is **built** by Cargo (`build.rs` hashes sources).
//!
pub mod ast;
pub mod document;
pub mod format;
pub mod grammar;
pub mod include;
pub mod parse;
#[cfg(feature = "walk")]
pub mod scope;
pub mod syntax;
pub mod visit;
pub mod vm;

pub use document::{DocEditError, LeekDoc};
pub use grammar::{COMPILE_TIME_GRAMMAR, GRule, GRAMMAR_SOURCE_FINGERPRINT};
pub use include::{
    IncludeLimits, IncludeLoadError, LoadedProject, LoadedSourceFile, MergeIncludesError,
    MergedCheckPrepError, MergedCheckUnit, MergedSourceMapping, MergedSpanMap, PreludeBuildError,
    ResolveError, infer_include_project_root, load_project_with_includes,
    load_project_with_includes_limited, load_project_with_includes_limited_with_overlay,
    merge_included_sources_to_single_file, merge_included_sources_to_single_file_mapped,
    merge_included_sources_to_single_file_mapped_with_overlay, prepare_merged_check_unit,
    prepend_signatures_to_merged, resolve_include_path, try_resolve_include_file,
};
pub use parse::{
    ExperimentalFeatures, FLAG_PARSE_RECOVERY, FLAG_SIGNATURE_MODE, LanguageOptions, ParseError,
    ParseErrorInner, ParsedWithRecovery, Version, is_signature_stub_path,
    language_options_with_source_directives, parse_doc, parse_doc_or_recover,
    parse_doc_reusing_vec, parse_doc_reusing_vec_with_built, parse_doc_with_built,
    parse_doc_with_recovery, parse_doc_with_recovery_limited,
    parse_doc_with_recovery_limited_with_built, parse_doc_with_recovery_with_built,
    parse_signature_doc, parse_signature_doc_reusing_vec,
    parse_signature_doc_reusing_vec_with_built, parse_signature_doc_with_built,
    parse_signature_doc_with_recovery, parse_signature_doc_with_recovery_limited,
    parse_signature_doc_with_recovery_limited_with_built,
    parse_signature_doc_with_recovery_with_built, parse_syntax_root,
};
#[cfg(feature = "partial-reparse")]
pub use parse::{parse_rule_at_offset, parse_rule_at_offset_with_built};
pub use sipha::diagnostics::parsed_doc::ParsedDoc;
pub use sipha::parse::builder::{BuiltGraph, SharedGrammar};
pub use sipha::types::{Pos, Span};

#[cfg(feature = "walk")]
pub use scope::{
    AnalysisResult, ExprTypeKey, LeekTy, Reference, Scope, ScopeId, SemanticCode,
    SemanticDiagnostic, SemanticSeverity, Symbol, SymbolId, SymbolKind, run_semantic_analysis,
};

#[cfg(feature = "transform")]
pub use document::{TransformResult, Transformer, transform};
