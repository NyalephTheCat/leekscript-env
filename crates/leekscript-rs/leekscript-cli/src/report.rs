//! miette-backed diagnostics for the `leekscript` CLI.

#![allow(unused_assignments)] // struct fields read only by `miette::Diagnostic` derive

use std::io::{self, Write};
use std::path::Path;

use leekscript::ParseError;
use leekscript::include::{
    IncludeLoadError, LoadedProject, MergeIncludesError, MergedSourceMapping, PreludeBuildError,
};
use miette::{Diagnostic, NamedSource, Report, SourceSpan};
use sipha::types::Span;
use thiserror::Error;

/// Prefer Unicode box-drawing and terminal links when supported. Ignored if a hook is already set.
pub fn install_hook() {
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(3)
                .build(),
        )
    }));
}

/// Render a report to stderr (graphical when `fancy` is enabled).
pub fn emit(report: Report) {
    let _ = writeln!(io::stderr(), "{report:?}");
}

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(leekscript::cli::message))]
struct CliMessage {
    message: String,
}

pub fn message(text: impl Into<String>) -> Report {
    Report::new(CliMessage {
        message: text.into(),
    })
}

#[derive(Debug, Error, Diagnostic)]
#[error("Parse error in {label}")]
#[diagnostic(code(leekscript::parse))]
struct ParseFailure {
    label: String,
    #[help]
    detail: String,
}

pub fn parse_diagnostic(label: impl Into<String>, source: &str, err: &ParseError) -> Report {
    Report::new(ParseFailure {
        label: label.into(),
        detail: err.format_with_source(source),
    })
}

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
#[diagnostic(code(leekscript::semantic))]
struct SemanticSnippet {
    message: String,
    #[source_code]
    src: NamedSource<String>,
    #[label("here")]
    span: SourceSpan,
    #[label("type annotation")]
    related: Option<SourceSpan>,
}

fn snippet(
    label: impl Into<String>,
    source: String,
    start: u32,
    len: u32,
    message: impl Into<String>,
    related: Option<(u32, u32)>,
) -> Report {
    let start_u = (start as usize).min(source.len());
    let len_u = (len as usize).min(source.len().saturating_sub(start_u));
    let span: SourceSpan = (start_u, len_u).into();
    let related = related.and_then(|(rs, rlen)| {
        let rs = (rs as usize).min(source.len());
        let rlen = (rlen as usize).min(source.len().saturating_sub(rs));
        if rlen == 0 {
            None
        } else {
            Some(SourceSpan::from((rs, rlen)))
        }
    });
    Report::new(SemanticSnippet {
        message: message.into(),
        src: NamedSource::new(label.into(), source),
        span,
        related,
    })
}

/// Map a semantic diagnostic in a merged / signature-prefixed check buffer back to a source file.
pub fn check_diagnostic(
    mapping: &MergedSourceMapping,
    project: Option<&LoadedProject>,
    combined_src: &str,
    fallback_label: &str,
    span: Span,
    message: &str,
    related_span: Option<Span>,
    stdin_user: Option<(&str, u32)>,
) -> Report {
    if let Some((stdin_body, user_base)) = stdin_user {
        if span.start >= user_base {
            let s = Span::new(
                span.start.saturating_sub(user_base),
                span.end.saturating_sub(user_base),
            );
            let rel = related_span
                .filter(|r| r.start >= user_base)
                .map(|r| {
                    let a = Span::new(
                        r.start.saturating_sub(user_base),
                        r.end.saturating_sub(user_base),
                    );
                    (a.start, a.end.saturating_sub(a.start))
                });
            return snippet(
                "<stdin>",
                stdin_body.to_string(),
                s.start,
                s.end.saturating_sub(s.start),
                message,
                rel,
            );
        }
    }

    if let Some(sm) = mapping.span_at_merged_offset(span.start) {
        let rel = span.start.saturating_sub(sm.merged_start);
        let file_start = sm.file_offset.saturating_add(rel);
        let len = span.end.saturating_sub(span.start);
        let related_in_file = related_span.and_then(|r| {
            let rsm = mapping.span_at_merged_offset(r.start)?;
            if rsm.path != sm.path {
                return None;
            }
            let rrel = r.start.saturating_sub(rsm.merged_start);
            let r_file_start = rsm.file_offset.saturating_add(rrel);
            let rlen = r.end.saturating_sub(r.start);
            Some((r_file_start, rlen))
        });
        if let Some(proj) = project {
            if let Some(f) = proj.files.iter().find(|f| f.path == sm.path) {
                return snippet(
                    f.path.display().to_string(),
                    f.source.clone(),
                    file_start,
                    len,
                    message,
                    related_in_file,
                );
            }
        }
        if let Ok(src) = std::fs::read_to_string(&sm.path) {
            return snippet(
                sm.path.display().to_string(),
                src,
                file_start,
                len,
                message,
                related_in_file,
            );
        }
    }

    let rel_combined = related_span.map(|r| {
        (
            r.start,
            r.end.saturating_sub(r.start),
        )
    });
    snippet(
        fallback_label,
        combined_src.to_string(),
        span.start,
        span.end.saturating_sub(span.start),
        message,
        rel_combined,
    )
}

pub fn stdin_io(err: io::Error) -> Report {
    message(format!("Failed to read standard input: {err}"))
}

pub fn io_path(path: impl AsRef<Path>, err: io::Error) -> Report {
    let path = path.as_ref();
    message(format!("I/O error at `{}`: {err}", path.display()))
}

pub fn include_load(root: &Path, entry: &Path, err: &IncludeLoadError) -> Report {
    match err {
        IncludeLoadError::Parse(path, e) => {
            if let Ok(src) = std::fs::read_to_string(path) {
                parse_diagnostic(path.display().to_string(), &src, e)
            } else {
                message(format!(
                    "Parse error in {}: {e:?}",
                    path.display()
                ))
            }
        }
        _ => message(format!(
            "Failed to load {} (root {}): {err}",
            entry.display(),
            root.display(),
        )),
    }
}

pub fn merge_includes(path: &Path, err: MergeIncludesError) -> Report {
    message(format!(
        "Include merge from `{}`: {err}",
        path.display()
    ))
}

pub fn merge_command(err: MergeIncludesError) -> Report {
    message(format!("merge: {err}"))
}

pub fn prelude_signatures(err: PreludeBuildError) -> Report {
    match err {
        PreludeBuildError::Io(p, e) => message(format!(
            "`--signatures` file `{}`: {e}",
            p.display()
        )),
    }
}

pub fn signatures_for_entry(entry: &Path, sig: &Path, err: &std::io::Error) -> Report {
    message(format!(
        "{}: `--signatures` `{}`: {err}",
        entry.display(),
        sig.display()
    ))
}

pub fn format_config(path: &Path, err: impl std::fmt::Display) -> Report {
    message(format!("Format config `{}`: {err}", path.display()))
}

pub fn format_usage(text: impl Into<String>) -> Report {
    message(text)
}
