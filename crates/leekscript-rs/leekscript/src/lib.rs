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
//! ## Formatting
//!
//! [`format`] is a configurable pretty-printer with `// leekfmt:`, `//! leekfmt:` (file-wide options),
//! and `/* leekfmt: */` directive comments for options and verbatim regions.
//!
//! ## Cargo features
//!
//! - **`grammar-v4-only`**: Specializes lexer/parser bytecode for [`parse::Version::V4`] only by
//!   stripping compile-time flag checks. Do not parse older [`parse::Version`] values with this
//!   enabled.

pub mod ast;
pub mod document;
pub mod format;
pub mod grammar;
pub mod include;
pub mod parse;
pub mod syntax;
pub mod visit;

pub use document::{DocEditError, LeekDoc};
pub use include::{
    IncludeLimits, IncludeLoadError, LoadedProject, LoadedSourceFile, MergeIncludesError,
    ResolveError, load_project_with_includes, load_project_with_includes_limited,
    merge_included_sources_to_single_file, resolve_include_path,
};
pub use parse::{ParseError, Version, parse_doc, parse_syntax_root};
pub use sipha::types::{Pos, Span};

#[cfg(feature = "transform")]
pub use document::{TransformResult, Transformer, transform};
