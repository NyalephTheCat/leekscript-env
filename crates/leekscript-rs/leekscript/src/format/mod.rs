//! Opinionated LeekScript formatter with rich [`FormatOptions`] and `// leekfmt:` directives.
//!
//! # Directive comments
//!
//! In line or block trivia, a marker `leekfmt:` (case-insensitive) starts a directive body.
//!
//! - **Options** use `kebab-case` or `snake_case` keys and `=` or `:` separators, e.g.  
//!   `// leekfmt: indent-width=2; space-around-binary-ops=false`  
//!   or `/* leekfmt: line-ending=crlf */`.
//! - **`off` / `on`**: everything after the `off` comment until the start of the `on` comment is
//!   copied verbatim from the source.
//! - **`ignore-next-line`** (aliases: `skip-next-line`, `ignore-next`, `skip-next`): the next line
//!   is preserved as-is.
//!
//! Directives apply from their **end byte offset** onward until superseded by another directive of
//! the same kind (options merge) or until a verbatim region ends (`on`).
//!
//! Trivia comments that are not `leekfmt:` directives are **not** written back; use `off`/`on` or
//! `ignore-next-line` to keep them.

mod directives;
mod options;
mod print;
mod spacing;

pub use directives::{DirectivePlan, scan_directives, span_is_preserved, span_touches_preserve};
pub use options::{BraceStyle, FormatOptions, FormatPatch, LineEnding};
pub use print::{format_document, format_leek_doc};
