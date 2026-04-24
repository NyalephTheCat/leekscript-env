//! `lek check` — same compile pipeline as `lek run` ([`leekscript_run::compile_source`]): directives, lexer, parse, HIR.

use leekscript_directives::FmtPreamble;
use leekscript_run::{compile_source, CompileDiagnostic, CompileOptions, CompilePhase};
use leekscript_span::{line_col_at, Span};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Max leading lines scanned for `// leek-*` (kept in sync with [`leekscript_run::PREAMBLE_MAX_LINES`]).
pub const PREAMBLE_MAX_LINES: usize = leekscript_run::PREAMBLE_MAX_LINES;

/// Read UTF-8 source for a check path. [`Path`] `"-"` reads all of standard input (for `lek check -`).
pub fn read_source(path: &Path) -> std::io::Result<String> {
    if path.as_os_str() == "-" {
        let mut buf = String::new();
        std::io::stdin().lock().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path)
    }
}

/// One unit of work for `lek check`: a file path or stdin with a display path for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckTarget {
    /// Read bytes from stdin; `display` is what appears in diagnostics (from `--stdin-path` or `-`).
    Stdin {
        display: PathBuf,
    },
    File(PathBuf),
}

/// Recursively collect `*.leek` under `dir` (skips hidden directories). Sorted for stable output.
pub fn collect_leek_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if dir
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|s| s.starts_with('.'))
    {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            collect_leek_files(&p, out)?;
        } else if ft.is_file() && p.extension().is_some_and(|e| e == "leek") {
            out.push(p);
        }
    }
    Ok(())
}

/// Turn CLI file arguments into [`CheckTarget`]s: `-` → stdin, directories → all `*.leek` under them.
pub fn expand_check_targets(
    args: &[PathBuf],
    stdin_path: Option<PathBuf>,
) -> std::io::Result<Vec<CheckTarget>> {
    let mut targets = Vec::new();
    for arg in args {
        if arg.as_os_str() == "-" {
            targets.push(CheckTarget::Stdin {
                display: stdin_path.clone().unwrap_or_else(|| PathBuf::from("-")),
            });
            continue;
        }
        match std::fs::metadata(arg) {
            Ok(m) if m.is_dir() => {
                let mut found = Vec::new();
                collect_leek_files(arg, &mut found)?;
                found.sort();
                for p in found {
                    targets.push(CheckTarget::File(p));
                }
            }
            _ => targets.push(CheckTarget::File(arg.clone())),
        }
    }
    Ok(targets)
}

#[derive(Clone, Debug)]
pub struct CheckOptions {
    pub manifest: Option<PathBuf>,
    pub cli_language_version: Option<u8>,
    pub cli_strict: Option<bool>,
    /// Names from signature TOML (`[signatures]` / `--signatures`) for the resolve pass.
    pub signature_globals: Vec<String>,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            manifest: None,
            cli_language_version: None,
            cli_strict: None,
            signature_globals: Vec::new(),
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticRecord {
    pub file: String,
    pub line: u32,
    pub column: u32,
    /// Byte range in UTF-8 source (for rich rendering; not part of JSON output).
    #[serde(skip_serializing)]
    pub span: Span,
    /// When set, human diagnostics use this text for the snippet (spans are into it), not `root_src`.
    /// Shared across diagnostics from the same file via [`Arc`] to avoid O(N×file) copies.
    #[serde(skip_serializing)]
    pub snippet_source: Option<Arc<str>>,
    pub code: String,
    pub reference: String,
    pub message: String,
    pub phase: &'static str,
}

/// Successful file summary (`lek check --message-format json` and LSP-style consumers).
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct CheckedOk {
    /// Path as passed to the checker (display string).
    #[serde(rename = "file")]
    pub path_display: String,
    pub language_version: u8,
    /// Effective strictness after CLI, preamble, and manifest (reserved for lints).
    pub strict: Option<bool>,
    pub token_count: usize,
    /// Top-level statements after HIR lowering (same as `lek run`).
    pub hir_stmt_count: usize,
    /// From file preamble `// leek-fmt:` (for `lek fmt`; does not affect lexing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fmt: Option<FmtPreamble>,
    /// From `// leek-experimental:` (tooling metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub enum CheckedFile {
    Ok(CheckedOk),
    Failed(Vec<DiagnosticRecord>),
}

pub fn default_registry_path() -> PathBuf {
    if let Ok(p) = std::env::var("LEEK_REGISTRY") {
        return PathBuf::from(p);
    }
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("data/diagnostics/registry.yaml");
    p
}

/// `Leek.toml` language fields when present.
pub fn manifest_language_settings(manifest: Option<&PathBuf>) -> (Option<u8>, Option<bool>) {
    let path = manifest.cloned().or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(leekscript_config::find_manifest)
    });
    let Some(p) = path else {
        return (None, None);
    };
    let Ok(m) = leekscript_config::LeekManifest::load_path(&p) else {
        return (None, None);
    };
    let Some(l) = m.language else {
        return (None, None);
    };
    let v = l.version.map(|x| (x.clamp(1, 99)) as u8);
    let s = l.strict;
    (v, s)
}

fn registry_code_for_compile(
    reg: &leekscript_diagnostics::Registry,
    d: &CompileDiagnostic,
) -> String {
    match d.phase {
        CompilePhase::Directives => reg
            .code_for_id(&d.reference)
            .or_else(|| reg.code_for_reference(&d.reference))
            .unwrap_or("E????")
            .to_string(),
        CompilePhase::Lexer
        | CompilePhase::Parser
        | CompilePhase::Hir
        | CompilePhase::Resolve
        | CompilePhase::Types => {
            reg.code_for_reference(&d.reference)
                .unwrap_or("E????")
                .to_string()
        }
    }
}

/// Map a compile-phase diagnostic to a registry-backed record (shared with `lek run`).
///
/// For a single diagnostic this is fine; for many diagnostics prefer
/// [`diagnostic_record_from_compile_with_cache`] so each snippet file is read at most once.
#[allow(dead_code)]
pub(crate) fn diagnostic_record_from_compile(
    registry: &leekscript_diagnostics::Registry,
    file_display: &str,
    src: &str,
    d: &CompileDiagnostic,
) -> DiagnosticRecord {
    let mut cache: HashMap<PathBuf, Arc<str>> = HashMap::new();
    diagnostic_record_from_compile_with_cache(registry, file_display, src, d, &mut cache)
}

/// Like [`diagnostic_record_from_compile`], but reuses `snippet_cache` so repeated
/// `snippet_origin` paths are not re-read from disk for every diagnostic (critical for large
/// `lek check` runs with thousands of resolve errors).
pub(crate) fn diagnostic_record_from_compile_with_cache(
    registry: &leekscript_diagnostics::Registry,
    file_display: &str,
    root_src: &str,
    d: &CompileDiagnostic,
    snippet_cache: &mut HashMap<PathBuf, Arc<str>>,
) -> DiagnosticRecord {
    let code = registry_code_for_compile(registry, d);
    let (file, snippet_source, line, col) = if let Some(ref po) = d.snippet_origin {
        let arc = snippet_cache
            .entry(po.clone())
            .or_insert_with(|| {
                Arc::from(std::fs::read_to_string(po).unwrap_or_default().into_boxed_str())
            })
            .clone();
        let (l, c) = line_col_at(arc.as_ref(), d.span.start as usize);
        (po.display().to_string(), Some(arc), l, c)
    } else {
        let (l, c) = line_col_at(root_src, d.span.start as usize);
        (file_display.to_string(), None, l, c)
    };
    DiagnosticRecord {
        file,
        line,
        column: col,
        span: d.span,
        snippet_source,
        code,
        reference: d.reference.clone(),
        message: d.message.clone(),
        phase: d.phase.as_str(),
    }
}

/// Full compile pipeline on one source (same as [`leekscript_run::compile_source`]); resolves version/strict from `opts` + preamble + manifest.
pub fn check_one_file(
    registry: &leekscript_diagnostics::Registry,
    path: &Path,
    src: &str,
    opts: &CheckOptions,
) -> CheckedFile {
    let file_str = path.display().to_string();
    let canon = std::fs::canonicalize(path).ok();
    let compile_opts = CompileOptions {
        manifest: opts.manifest.clone(),
        cli_language_version: opts.cli_language_version,
        cli_strict: opts.cli_strict,
        source_path: canon.clone(),
        snippet_origin: None,
        signature_globals: opts.signature_globals.clone(),
    };

    match compile_source(&file_str, src, &compile_opts) {
        Ok(unit) => CheckedFile::Ok(CheckedOk {
            path_display: file_str,
            language_version: unit.language_version,
            strict: unit.strict,
            token_count: unit.token_count,
            hir_stmt_count: unit.hir.stmts.len(),
            fmt: unit.fmt,
            experimental: unit.experimental,
        }),
        Err(diags) => {
            let mut snippet_cache: HashMap<PathBuf, Arc<str>> = HashMap::new();
            if let Some(ref p) = canon {
                snippet_cache.insert(p.clone(), Arc::from(src.to_string().into_boxed_str()));
            }
            let records = diags
                .iter()
                .map(|d| {
                    diagnostic_record_from_compile_with_cache(
                        registry,
                        &file_str,
                        src,
                        d,
                        &mut snippet_cache,
                    )
                })
                .collect();
            CheckedFile::Failed(records)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_directory_finds_repo_fixtures() {
        let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        root.pop();
        root.pop();
        let fixtures = root.join("tests/fixtures");
        let t = expand_check_targets(&[fixtures], None).unwrap();
        let names: Vec<_> = t
            .iter()
            .filter_map(|x| match x {
                CheckTarget::File(p) => Some(p.file_name().unwrap().to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        assert!(names.iter().any(|n| n == "smoke.leek"));
        assert!(names.iter().any(|n| n == "unclosed.leek"));
        assert!(t.len() >= 3);
    }

    #[test]
    fn expand_stdin_label() {
        let t = expand_check_targets(
            &[PathBuf::from("-")],
            Some(PathBuf::from("/virtual/file.leek")),
        )
        .unwrap();
        assert_eq!(
            t,
            vec![CheckTarget::Stdin {
                display: PathBuf::from("/virtual/file.leek")
            }]
        );
    }
}
