use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeDiagnostic {
    pub message: String,
}

pub fn analyze_ai_source(src: &str) -> Result<Vec<AnalyzeDiagnostic>, leekscript::ParseError> {
    analyze_ai_source_with_path(src, None::<&Path>)
}

pub fn analyze_ai_source_with_path(
    src: &str,
    path: Option<impl AsRef<Path>>,
) -> Result<Vec<AnalyzeDiagnostic>, leekscript::ParseError> {
    let lang = match path {
        Some(p) if leekscript::is_signature_stub_path(p.as_ref()) => {
            leekscript::LanguageOptions::default()
        }
        _ => leekscript::LanguageOptions::default(),
    };

    let parsed = leekscript::parse_doc_or_recover(src, lang)?;
    Ok(parsed
        .errors
        .into_iter()
        .map(|e| AnalyzeDiagnostic {
            message: format!("{e:?}"),
        })
        .collect())
}
