//! `leekscript check` — parse merged project, optional semantic diagnostics.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use leekscript::include::{
    MergedSourceMapping, infer_include_project_root, prepend_signatures_to_merged,
};
use leekscript::{
    LanguageOptions, SemanticCode, SemanticSeverity, language_options_with_source_directives,
    parse_doc, parse_signature_doc, prepare_merged_check_unit,
};

use crate::report;

#[inline]
fn semantic_diagnostic_fails_check(d: &leekscript::SemanticDiagnostic) -> bool {
    d.severity == SemanticSeverity::Error || d.code == SemanticCode::BareReturnRequiresSemicolon
}

pub(crate) fn cmd_check(
    lang: LanguageOptions,
    parse_only: bool,
    root_override: Option<&Path>,
    signature_files: &[PathBuf],
    files: &[PathBuf],
) -> ExitCode {
    let mut ok = true;

    if files.is_empty() {
        let mut src = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut src) {
            report::emit(report::stdin_io(e));
            return ExitCode::from(1);
        }
        let label = "<stdin>";

        if !signature_files.is_empty() {
            let (combined, map) = match prepend_signatures_to_merged(
                lang,
                signature_files,
                &src,
                MergedSourceMapping::default(),
            ) {
                Ok(x) => x,
                Err(e) => {
                    report::emit(report::prelude_signatures(e));
                    return ExitCode::from(1);
                }
            };
            let user_base = (combined.len() - src.len()) as u32;
            match parse_signature_doc(&combined, lang) {
                Ok(doc) => {
                    if !parse_only {
                        let resolved = language_options_with_source_directives(&combined, lang);
                        let analysis =
                            leekscript::run_semantic_analysis(doc.root(), resolved.version);
                        for d in &analysis.diagnostics {
                            if semantic_diagnostic_fails_check(d) {
                                ok = false;
                            }
                            report::emit(report::check_diagnostic(
                                &map,
                                None,
                                &combined,
                                label,
                                d.span,
                                &d.message,
                                d.related_span,
                                Some((&src, user_base)),
                                d.severity,
                            ));
                        }
                    }
                }
                Err(e) => {
                    ok = false;
                    report::emit(report::parse_diagnostic(label, &combined, &e));
                }
            }
            return if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            };
        }

        match parse_doc(&src, lang) {
            Ok(doc) => {
                if !parse_only {
                    let resolved = language_options_with_source_directives(&src, lang);
                    let analysis = leekscript::run_semantic_analysis(doc.root(), resolved.version);
                    for d in &analysis.diagnostics {
                        if semantic_diagnostic_fails_check(d) {
                            ok = false;
                        }
                        report::emit(report::check_diagnostic(
                            &MergedSourceMapping::default(),
                            None,
                            &src,
                            label,
                            d.span,
                            &d.message,
                            d.related_span,
                            None,
                            d.severity,
                        ));
                    }
                }
            }
            Err(e) => {
                ok = false;
                report::emit(report::parse_diagnostic(label, &src, &e));
            }
        }
        return if ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    for path in files {
        let label = path.display().to_string();
        let entry = match std::fs::canonicalize(path) {
            Ok(p) => p,
            Err(e) => {
                ok = false;
                report::emit(report::io_path(path, e));
                continue;
            }
        };
        let root = if let Some(r) = root_override {
            match std::fs::canonicalize(r) {
                Ok(p) => p,
                Err(e) => {
                    ok = false;
                    report::emit(report::io_path(r, e));
                    continue;
                }
            }
        } else {
            infer_include_project_root(&entry)
        };

        let prep = match prepare_merged_check_unit(&root, &entry, lang, signature_files, None) {
            Ok(p) => p,
            Err(e) => {
                ok = false;
                report::emit(report::merged_check_prep(
                    &root,
                    path,
                    e,
                    report::MergedCheckPrepContext::CheckOrFormat,
                ));
                continue;
            }
        };

        let parse_result = if prep.use_signature_grammar {
            parse_signature_doc(&prep.combined, prep.resolved)
        } else {
            parse_doc(&prep.combined, prep.resolved)
        };
        match parse_result {
            Ok(doc) => {
                if !parse_only {
                    let analysis =
                        leekscript::run_semantic_analysis(doc.root(), prep.resolved.version);
                    for d in &analysis.diagnostics {
                        if semantic_diagnostic_fails_check(d) {
                            ok = false;
                        }
                        report::emit(report::check_diagnostic(
                            &prep.mapping,
                            Some(&prep.project),
                            &prep.combined,
                            &label,
                            d.span,
                            &d.message,
                            d.related_span,
                            None,
                            d.severity,
                        ));
                    }
                }
            }
            Err(e) => {
                ok = false;
                report::emit(report::parse_diagnostic(
                    format!("{label} (merged)"),
                    &prep.combined,
                    &e,
                ));
            }
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
