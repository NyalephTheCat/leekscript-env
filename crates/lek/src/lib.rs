//! `LeekScript` CLI library (`lek`). The binary in `main.rs` wraps this for `registry`, `config`, and `check`.

pub mod check;
pub mod fmt;
pub mod reporter;
pub mod run;
pub mod signatures;
pub mod toolchain_refs;

pub use check::{
    check_one_file, collect_leek_files, default_registry_path, expand_check_targets,
    manifest_language_settings, read_source, CheckOptions, CheckTarget, CheckedFile, CheckedOk,
    DiagnosticRecord,
};
pub use fmt::{format_one_file, FmtOptions};
pub use leekscript_directives::FmtPreamble;
pub use leekscript_fmt::{FmtConfig, FormatError};
pub use leekscript_parser::{parse_file_green, ParsedFile};
pub use leekscript_syntax::{
    build_source_file_tree, gaps_from_source_file, parse_source_file_tree, AstNode, FileSegments,
    LeekLanguage, LeekSyntaxKind, SourceFile, SyntaxElementPtr, SyntaxNodePtr, SyntaxTokenPtr,
    TextRange, TriviaPiece,
};
