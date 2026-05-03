//! `lek run` — compile ([`leekscript_run::compile_source`]) then interpret HIR ([`leekscript_run::interpret_hir`]).

use crate::check::{
    diagnostic_record_from_compile_with_cache, expand_check_targets, read_source, CheckOptions,
    CheckTarget,
};
use crate::reporter::{emit_diagnostic, emit_message};
use leekscript_diagnostics::Registry;
use leekscript_run::{
    compile_source, interpret_hir, interpret_reference_display_message, value_java_export,
    CompileOptions,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Serialize)]
pub struct JsonRunDiagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub code: String,
    pub reference: String,
    pub message: String,
    pub phase: &'static str,
}

#[derive(Serialize)]
pub struct JsonRunFileLine {
    pub file: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_ok: Option<bool>,
    /// Top-level statements after HIR lowering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hir_stmt_count: Option<usize>,
    /// Result of [`interpret_hir`](leekscript_run::interpret_hir) when compile succeeded (`None` on runtime error).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

impl JsonRunFileLine {
    fn ok(
        path: &str,
        language_version: u8,
        strict: Option<bool>,
        token_count: usize,
        hir_stmt_count: usize,
        result: Option<String>,
    ) -> Self {
        Self {
            file: path.to_string(),
            status: "ok",
            language_version: Some(language_version),
            strict,
            token_count: Some(token_count),
            parse_ok: Some(true),
            hir_stmt_count: Some(hir_stmt_count),
            result,
        }
    }

    fn error(path: &str) -> Self {
        Self {
            file: path.to_string(),
            status: "error",
            language_version: None,
            strict: None,
            token_count: None,
            parse_ok: None,
            hir_stmt_count: None,
            result: None,
        }
    }

    fn io_error(path: &str) -> Self {
        Self {
            file: path.to_string(),
            status: "io_error",
            language_version: None,
            strict: None,
            token_count: None,
            parse_ok: None,
            hir_stmt_count: None,
            result: None,
        }
    }
}

#[derive(Clone, Copy)]
pub enum RunMessageFormat {
    Human,
    Json,
}

fn registry_e_code(registry: &Registry, reference: &str) -> String {
    registry
        .code_for_reference(reference)
        .unwrap_or("E????")
        .to_string()
}

/// Run the compile pipeline on each target, then interpret HIR (tree-walking executor in `leekscript_run`).
pub fn run_compile(
    files: &[PathBuf],
    stdin_path: Option<PathBuf>,
    opts: CheckOptions,
    message_format: RunMessageFormat,
    registry: &Registry,
) -> std::process::ExitCode {
    if stdin_path.is_some() && !files.iter().any(|p| p.as_os_str() == "-") {
        emit_message("lek run: --stdin-path only applies when '-' is among the input paths");
        return std::process::ExitCode::from(2);
    }
    let targets = match expand_check_targets(files, stdin_path) {
        Ok(t) => t,
        Err(e) => {
            emit_message(format!("lek run: {e}"));
            return std::process::ExitCode::from(1);
        }
    };
    if targets.is_empty() {
        emit_message("lek run: no .leek files found under the given directories");
        return std::process::ExitCode::from(2);
    }

    let mut json_files: Vec<JsonRunFileLine> = Vec::new();
    let mut json_diags: Vec<JsonRunDiagnostic> = Vec::new();
    let mut exit: u8 = 0;

    for target in targets {
        let (src, path) = match &target {
            CheckTarget::Stdin { display } => match read_source(Path::new("-")) {
                Ok(s) => (s, display.as_path()),
                Err(e) => {
                    exit = 1;
                    let msg = format!("{e}");
                    match message_format {
                        RunMessageFormat::Human => {
                            emit_message(format!("{}: {e}", display.display()));
                        }
                        RunMessageFormat::Json => {
                            json_files
                                .push(JsonRunFileLine::io_error(&display.display().to_string()));
                            json_diags.push(JsonRunDiagnostic {
                                file: display.display().to_string(),
                                line: 0,
                                column: 0,
                                code: "E????".into(),
                                reference: "IO_ERROR".into(),
                                message: msg,
                                phase: "io",
                            });
                        }
                    }
                    continue;
                }
            },
            CheckTarget::File(p) => match read_source(p) {
                Ok(s) => (s, p.as_path()),
                Err(e) => {
                    exit = 1;
                    let msg = format!("{e}");
                    match message_format {
                        RunMessageFormat::Human => {
                            emit_message(format!("{}: {e}", p.display()));
                        }
                        RunMessageFormat::Json => {
                            json_files.push(JsonRunFileLine::io_error(&p.display().to_string()));
                            json_diags.push(JsonRunDiagnostic {
                                file: p.display().to_string(),
                                line: 0,
                                column: 0,
                                code: "E????".into(),
                                reference: "IO_ERROR".into(),
                                message: msg,
                                phase: "io",
                            });
                        }
                    }
                    continue;
                }
            },
        };

        let file_str = path.display().to_string();
        let compile_opts = CompileOptions {
            manifest: opts.manifest.clone(),
            cli_language_version: opts.cli_language_version,
            cli_strict: opts.cli_strict,
            source_path: match &target {
                CheckTarget::File(p) => std::fs::canonicalize(p).ok(),
                CheckTarget::Stdin { .. } => None,
            },
            snippet_origin: None,
            signature_globals: opts.signature_globals.clone(),
        };
        match compile_source(&file_str, &src, &compile_opts) {
            Ok(unit) => {
                let hir_n = unit.hir.stmts.len();
                match interpret_hir(&unit.hir, unit.language_version) {
                    Ok(outcome) => {
                        let result_str =
                            outcome.map(|v| value_java_export(&v, unit.language_version));
                        match message_format {
                            RunMessageFormat::Human => {
                                let strict_note = match unit.strict {
                                    Some(true) => ", strict on",
                                    Some(false) => ", strict off",
                                    None => "",
                                };
                                let run_note = match &result_str {
                                    Some(s) => format!("; result: {s}"),
                                    None => "; completed (no return value)".to_string(),
                                };
                                eprintln!(
                                    "{}: OK (HIR {} stmt(s), language {}{}, {} token(s){run_note})",
                                    unit.path_display,
                                    hir_n,
                                    unit.language_version,
                                    strict_note,
                                    unit.token_count
                                );
                            }
                            RunMessageFormat::Json => {
                                json_files.push(JsonRunFileLine::ok(
                                    &file_str,
                                    unit.language_version,
                                    unit.strict,
                                    unit.token_count,
                                    hir_n,
                                    result_str,
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        exit = 1;
                        let code = registry_e_code(registry, e.reference);
                        let msg = interpret_reference_display_message(e.reference)
                            .map(str::to_string)
                            .unwrap_or(e.message);
                        match message_format {
                            RunMessageFormat::Human => {
                                emit_message(format!(
                                    "{file_str}: run error: {code} ({}): {}",
                                    e.reference, msg
                                ));
                            }
                            RunMessageFormat::Json => {
                                json_files.push(JsonRunFileLine {
                                    file: file_str.clone(),
                                    status: "run_error",
                                    language_version: Some(unit.language_version),
                                    strict: unit.strict,
                                    token_count: Some(unit.token_count),
                                    parse_ok: Some(true),
                                    hir_stmt_count: Some(hir_n),
                                    result: None,
                                });
                                json_diags.push(JsonRunDiagnostic {
                                    file: file_str.clone(),
                                    line: 0,
                                    column: 0,
                                    code,
                                    reference: e.reference.to_string(),
                                    message: msg,
                                    phase: "run",
                                });
                            }
                        }
                    }
                }
            }
            Err(diags) => {
                exit = 1;
                if matches!(message_format, RunMessageFormat::Json) {
                    json_files.push(JsonRunFileLine::error(&file_str));
                }
                let mut snippet_cache: HashMap<PathBuf, Arc<str>> = HashMap::new();
                if let Some(ref p) = compile_opts.source_path {
                    snippet_cache.insert(p.clone(), Arc::from(src.clone().into_boxed_str()));
                }
                for d in &diags {
                    let rec = diagnostic_record_from_compile_with_cache(
                        registry,
                        &file_str,
                        &src,
                        d,
                        &mut snippet_cache,
                    );
                    match message_format {
                        RunMessageFormat::Human => emit_diagnostic(&src, &rec),
                        RunMessageFormat::Json => json_diags.push(JsonRunDiagnostic {
                            file: rec.file,
                            line: rec.line,
                            column: rec.column,
                            code: rec.code,
                            reference: rec.reference,
                            message: rec.message,
                            phase: rec.phase,
                        }),
                    }
                }
            }
        }
    }

    if matches!(message_format, RunMessageFormat::Json) {
        match serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 4,
            "command": "run",
            "files": json_files,
            "diagnostics": json_diags
        })) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                emit_message(format!("json error: {e}"));
                return std::process::ExitCode::from(1);
            }
        }
    }

    std::process::ExitCode::from(exit)
}
