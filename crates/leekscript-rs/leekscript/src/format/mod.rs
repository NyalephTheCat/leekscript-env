//! Opinionated LeekScript formatter with rich [`FormatOptions`] and `leekfmt:` directive comments.
//!
//! # Directive comments
//!
//! In line or block trivia, a marker `leekfmt:` (case-insensitive) starts a directive body.
//! Use **`//! leekfmt:`** (line) or **`/*! leekfmt: … */`** (block) for **file-wide** options: those
//! apply from byte offset **0**, so they affect the whole file, including lines above the comment.
//!
//! - **Options** use `kebab-case` or `snake_case` keys and `=` or `:` separators, e.g.  
//!   `// leekfmt: indent-width=2; space-around-binary-ops=false`  
//!   or `/* leekfmt: line-ending=crlf */` or `//! leekfmt: indent-width=2`.  
//!   **Semicolons:** `semicolons=preserve` (default), `always`, or `only-needed` (keep `return;` / `break;` / `continue;`, drop other optional `;`).
//! - **`off` / `on`**: everything after the `off` comment until the start of the `on` comment is
//!   copied verbatim from the source.
//! - **`ignore-next-line`** (aliases: `skip-next-line`, `ignore-next`, `skip-next`): the next line
//!   is preserved as-is.
//!
//! Ordinary `//` / `/*` directives apply from the comment’s **end byte offset** onward until
//! superseded. File-wide `!` form applies options from offset **0**; `off` / `on` / `ignore-next-line`
//! still use the comment end position.
//!
//! Trivia comments that are not `leekfmt:` directives are **not** written back; use `off`/`on` or
//! `ignore-next-line` to keep them.
//!
//! **Wrapping:** when [`FormatOptions::line_width`](crate::format::FormatOptions::line_width) is
//! non-zero, the printer may break after a comma before the next token; verbatim (`off`/`on`) regions
//! do not wrap inside the copied span (column tracking resumes from the pasted text).
//!
//! **Types:** [`FormatOptions::space_around_type_operators`](crate::format::FormatOptions::space_around_type_operators)
//! controls spaces around `|`, `<`, and `>` inside type syntax (default off → `A|B`, `A<B>`).
//!
//! **Blank lines:** [`FormatOptions::blank_lines_between_class_members`](crate::format::FormatOptions::blank_lines_between_class_members)
//! (default `1`) separates methods and separates field groups from methods; consecutive **fields** stay
//! tight (no extra blank). [`FormatOptions::blank_lines_between_block_statements`](crate::format::FormatOptions::blank_lines_between_block_statements)
//! (default `0`) adds blanks between statements in other blocks when set.
//! [`FormatOptions::blank_lines_after_class`](crate::format::FormatOptions::blank_lines_after_class)
//! (default `2`) adds extra separation after a top-level class before the next declaration.

mod directives;
mod options;
mod print;
mod spacing;

pub use directives::{
    DirectivePlan, preserve_region_overlapping, scan_directives, span_is_preserved,
    span_touches_preserve,
};
pub use options::{BraceStyle, FormatOptions, FormatPatch, LineEnding, SemicolonStyle};
pub use print::{format_document, format_leek_doc};
