//! Fancy terminal diagnostics via [miette](https://docs.rs/miette) (`fancy` feature on the `lek` crate).

use crate::check::DiagnosticRecord;
use leekscript_fmt::FormatError;
use leekscript_lexer::LexError;
use leekscript_parser::ParseDiagnostic;
use leekscript_run::lexer_reference_display_message;
use leekscript_span::Span;
use miette::{LabeledSpan, NamedSource, Report};
use std::sync::OnceLock;

static HOOK: OnceLock<()> = OnceLock::new();

/// Install the graphical miette handler (terminal links, unicode, snippet context). Safe to call once.
pub fn install_hook() {
    HOOK.get_or_init(|| {
        let _ = miette::set_hook(Box::new(|_| {
            Box::new(
                miette::MietteHandlerOpts::new()
                    .terminal_links(true)
                    .unicode(true)
                    .context_lines(3)
                    .tab_width(4)
                    .break_words(true)
                    .build(),
            )
        }));
    });
}

fn clamp_span(span: Span, src_len: usize) -> std::ops::Range<usize> {
    let mut s = span.start as usize;
    let mut e = span.end as usize;
    if s > src_len {
        s = src_len;
    }
    if e > src_len {
        e = src_len;
    }
    if e <= s {
        if s < src_len {
            e = (s + 1).min(src_len);
        } else if s > 0 {
            s -= 1;
            e = s + 1;
        } else {
            return 0..0;
        }
    }
    s..e
}

/// Emit a single registry-backed diagnostic with source snippet.
pub fn emit_diagnostic(root_src: &str, record: &DiagnosticRecord) {
    let src = record.snippet_source.as_deref().unwrap_or(root_src);
    let range = clamp_span(record.span, src.len());
    let code = format!("{}::{}", record.code, record.reference);
    let help = format!(
        "Phase `{}` · `{}` · use `lek check --message-format json` for machine-readable output.",
        record.phase, record.code
    );
    let named = NamedSource::new(record.file.clone(), src.to_string());
    let report: Report = if range.is_empty() {
        miette::miette!(code = code, help = help, "{}", record.message)
    } else {
        miette::miette!(
            code = code,
            labels = vec![LabeledSpan::at(range, record.message.as_str())],
            help = help,
            "{}",
            record.message
        )
        .with_source_code(named)
    };
    eprintln!("{report:?}");
}

/// Format / lexer / parse failures from `lek fmt`.
pub fn emit_fmt_error(path_display: &str, src: &str, err: &FormatError) {
    match err {
        FormatError::Lex(errs) => {
            for e in errs {
                emit_lex_error(path_display, src, e);
            }
        }
        FormatError::Parse(errs) => {
            for e in errs {
                emit_parse_diag(path_display, src, e);
            }
        }
    }
}

fn emit_lex_error(path_display: &str, src: &str, err: &LexError) {
    let range = clamp_span(err.span, src.len());
    let msg = lexer_reference_display_message(err.reference);
    let code = format!("lexer::{}", err.reference);
    let named = NamedSource::new(path_display, src.to_string());
    let report: Report = if range.is_empty() {
        miette::miette!(
            code = code,
            help = "Lexing failed; fix the highlighted region and retry.",
            "{}",
            msg
        )
    } else {
        miette::miette!(
            code = code,
            labels = vec![LabeledSpan::at(range, msg)],
            help = "Lexing failed; fix the highlighted region and retry.",
            "{}",
            msg
        )
        .with_source_code(named)
    };
    eprintln!("{report:?}");
}

fn emit_parse_diag(path_display: &str, src: &str, err: &ParseDiagnostic) {
    let range = clamp_span(err.span, src.len());
    let code = format!("parser::{}", err.reference);
    let named = NamedSource::new(path_display, src.to_string());
    let report: Report = if range.is_empty() {
        miette::miette!(
            code = code,
            help = "Delimiter or parse issue; see the language spec for valid syntax.",
            "{}",
            err.message
        )
    } else {
        miette::miette!(
            code = code,
            labels = vec![LabeledSpan::at(range, err.message)],
            help = "Delimiter or parse issue; see the language spec for valid syntax.",
            "{}",
            err.message
        )
        .with_source_code(named)
    };
    eprintln!("{report:?}");
}

/// Simple message when there is no source buffer (I/O, config, …).
pub fn emit_message(msg: impl std::fmt::Display) {
    let report: Report = miette::miette!("{}", msg);
    eprintln!("{report:?}");
}
