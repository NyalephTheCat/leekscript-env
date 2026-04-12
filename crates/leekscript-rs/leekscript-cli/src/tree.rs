//! `leekscript tree` — syntax tree dump.

use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use leekscript::syntax::kinds::K;
use leekscript::{LanguageOptions, is_signature_stub_path, parse_doc, parse_signature_doc};
use sipha::tree::tree_display::{TreeDisplayOptions, format_syntax_tree};
use sipha::types::{FromSyntaxKind, SyntaxKind};

use crate::report;

fn kind_to_name(k: SyntaxKind) -> Option<&'static str> {
    K::from_syntax_kind(k).map(K::as_str)
}

pub(crate) fn cmd_tree(lang: LanguageOptions, trivia: bool, files: &[PathBuf]) -> ExitCode {
    let opts = TreeDisplayOptions {
        show_trivia: trivia,
        ..Default::default()
    };

    if files.is_empty() {
        let mut src = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut src) {
            report::emit(report::stdin_io(e));
            return ExitCode::from(1);
        }
        let doc = match parse_doc(&src, lang) {
            Ok(d) => d,
            Err(e) => {
                report::emit(report::parse_diagnostic("<stdin>", &src, &e));
                return ExitCode::from(1);
            }
        };
        let out = format_syntax_tree(doc.root(), &opts, |k| {
            kind_to_name(k).unwrap_or("<?>").to_string()
        });
        print!("{out}");
        return ExitCode::SUCCESS;
    }

    let mut ok = true;
    for path in files {
        let label = path.display().to_string();
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                ok = false;
                report::emit(report::io_path(path, e));
                continue;
            }
        };
        let parse_result = if is_signature_stub_path(path) {
            parse_signature_doc(&src, lang)
        } else {
            parse_doc(&src, lang)
        };
        let doc = match parse_result {
            Ok(d) => d,
            Err(e) => {
                ok = false;
                report::emit(report::parse_diagnostic(&label, &src, &e));
                continue;
            }
        };
        if files.len() > 1 {
            println!("== {} ==", path.display());
        }
        let out = format_syntax_tree(doc.root(), &opts, |k| {
            kind_to_name(k).unwrap_or("<?>").to_string()
        });
        print!("{out}");
        if files.len() > 1 {
            println!();
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
