//! Compile and (eventually) execute LeekScript.
//!
//! Today this crate drives **directives → lexer → delimiter check → grammar parse → HIR lowering** and returns a
//! lossless rowan [`SyntaxNode`](rowan::SyntaxNode) plus [`HirFile`](leekscript_hir::HirFile). A future phase can emit bytecode or call a VM.

mod interp;
mod pipeline;
pub mod sig_workspace;

pub use interp::{
    interpret_hir, interpret_hir_with_host, interpret_hir_with_limits_and_stats,
    interpret_hir_with_strict, value_java_export, DebugLogHandled, DebugLogKind, DebugSourceContext,
    ExecAbort, InstanceData, InterpretError, InterpretSession, InterpretStats, InterpreterHost, Value,
    STDLIB_GLOBAL_IDENTIFIERS,
};
pub use leekscript_hir::{HirBinOp, HirExpr, HirFile, HirStmt, HirSwitchClause, HirUnaryOp};

/// Human-readable copy for lexer [`leekscript_lexer::LexError::reference`] values (`lek check`, `lek fmt`, `lek run` compile failures).
pub fn lexer_reference_display_message(reference: &str) -> &'static str {
    match reference {
        "INVALID_CHAR" => "invalid character",
        "STRING_NOT_CLOSED" => "string literal is not closed",
        _ => "lexer diagnostic",
    }
}

/// Stable CLI copy for interpreter [`InterpretError::reference`] values. [`None`] means use [`InterpretError::message`].
pub fn interpret_reference_display_message(reference: &str) -> Option<&'static str> {
    match reference {
        "NOT_ITERABLE" => Some("value is not iterable"),
        _ => None,
    }
}

/// Java `reference` ids emitted by [`interpret_hir`](interp::interpret_hir) — keep in sync with [`InterpretError`](interp::InterpretError).
pub const INTERP_EMITTED_REFERENCES: &[&str] = &[
    "BREAK_OUT_OF_LOOP",
    "CANNOT_ITERATE_UNBOUNDED_INTERVAL",
    "CONTINUE_OUT_OF_LOOP",
    "DIVISION_BY_ZERO",
    "FUNCTION_NOT_AVAILABLE",
    "REMOVED_FUNCTION_REPLACEMENT",
    "INTERNAL_ERROR",
    "INVALID_PARAMETER_COUNT",
    "NOT_CALLABLE",
    "NOT_ITERABLE",
    "THIS_NOT_ALLOWED_HERE",
    "UNCAUGHT_THROW",
    "VARIABLE_NOT_EXISTS",
    "WRONG_ARGUMENT_TYPE",
];
pub use leekscript_resolve::{resolve_hir, resolve_hir_with_extra_globals, ResolveDiagnostic};
pub use pipeline::{
    compile_signature_leek, compile_source, parse_sig_leek, resolve_include_file,
    CompileDiagnostic, CompileOptions, CompileOutcome, CompilePhase, CompiledUnit,
    ExpandedSourceUnit, ModuleExpansionCache, SigLeekUnit, PREAMBLE_MAX_LINES,
};
pub use sig_workspace::{
    extract_core_sig_line_based, merge_sig_units, merged_workspace_sig_bundle,
    strip_block_comments_for_sig_parse, CORE_SIG_LEEK, LEEKWARS_SIG_LEEK,
};
