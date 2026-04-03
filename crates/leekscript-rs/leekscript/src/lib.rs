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
//! ## Cargo features
//!
//! - **`grammar-v4-only`**: Specializes lexer/parser bytecode for [`parse::Version::V4`] /
//!   [`parse::Version::VNext`] by stripping older-dialect flag checks. Do not parse
//!   [`parse::Version::V1`]–[`parse::Version::V3`] with this enabled.

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

pub use document::{DocEditError, LeekDoc};
pub use include::{
    IncludeLimits, IncludeLoadError, LoadedProject, LoadedSourceFile, MergeIncludesError,
    MergedSourceMapping, MergedSpanMap, PreludeBuildError, ResolveError,
    load_project_with_includes, load_project_with_includes_limited,
    merge_included_sources_to_single_file, merge_included_sources_to_single_file_mapped,
    prepend_signatures_to_merged, resolve_include_path, try_resolve_include_file,
};
#[cfg(feature = "partial-reparse")]
pub use parse::parse_rule_at_offset;
pub use parse::{
    FLAG_PARSE_RECOVERY, FLAG_SIGNATURE_MODE, ParseError, ParsedWithRecovery, Version,
    is_signature_stub_path, parse_doc, parse_doc_or_recover, parse_doc_with_recovery,
    parse_doc_with_recovery_limited, parse_signature_doc, parse_syntax_root,
};
pub use sipha::types::{Pos, Span};

#[cfg(feature = "walk")]
pub use scope::{
    AnalysisResult, ExprTypeKey, LeekTy, Reference, Scope, ScopeId, SemanticCode,
    SemanticDiagnostic, SemanticSeverity, run_semantic_analysis,
};

#[cfg(feature = "transform")]
pub use document::{TransformResult, Transformer, transform};
