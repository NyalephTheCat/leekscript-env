//! Load signature bundles for the resolve pass: TOML ([`leekscript_signatures`]) or Leek-shaped
//! **`.sig.leek` / `.sig.ls`** files ([`leekscript_run::compile_signature_leek`]).

use leekscript_run::{compile_signature_leek, CompileDiagnostic, CompileOptions};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn is_sig_leek_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".sig.leek") || s.ends_with(".sig.ls")
}

fn is_toml_signature_path(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "toml")
}

fn format_sig_diags(path: &Path, diags: &[CompileDiagnostic]) -> String {
    match diags.first() {
        Some(d) => format!("{}: [{}] {}", path.display(), d.reference, d.message),
        None => format!("{}: could not load signature file", path.display()),
    }
}

fn load_one_signature_file(
    path: &Path,
    compile_opts: &CompileOptions,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
) -> Result<(), String> {
    if is_toml_signature_path(path) {
        let file = leekscript_signatures::SignatureFile::load_path(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        for n in file.resolve_names() {
            if seen.insert(n.clone()) {
                out.push(n);
            }
        }
        return Ok(());
    }
    if is_sig_leek_path(path) {
        let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let canon = path.canonicalize().ok();
        let mut opts = compile_opts.clone();
        opts.source_path = canon.clone();
        opts.snippet_origin = canon.clone();
        let names = compile_signature_leek(&src, &opts).map_err(|d| format_sig_diags(path, &d))?;
        for n in names {
            if seen.insert(n.clone()) {
                out.push(n);
            }
        }
        return Ok(());
    }
    Err(format!(
        "unsupported signature file `{}` (use `.toml`, `.sig.leek`, or `.sig.ls`)",
        path.display()
    ))
}

/// Merge globals/functions from `[signatures].path` (relative to the manifest directory) and from
/// each `--signatures` file. Order: manifest first, then CLI files; duplicates keep the first.
///
/// Manifest and CLI entries may be either TOML ([`leekscript_signatures::SignatureFile`]) or
/// `.sig.leek` / `.sig.ls` (LeekScript-shaped stubs: `function`, `global`, `var`, `class` at top level;
/// use empty `{ }` function bodies).
pub fn collect_signature_globals(
    manifest_file: Option<&PathBuf>,
    cli_paths: &[PathBuf],
) -> Result<Vec<String>, String> {
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::new();

    let compile_opts = CompileOptions {
        manifest: manifest_file.cloned(),
        ..Default::default()
    };

    if let Some(mf) = manifest_file {
        if let Ok(m) = leekscript_config::LeekManifest::load_path(mf) {
            if let Some(ref sc) = m.signatures {
                if let Some(ref rel) = sc.path {
                    let base = mf.parent().unwrap_or_else(|| Path::new("."));
                    let full = base.join(rel);
                    load_one_signature_file(&full, &compile_opts, &mut seen, &mut out)?;
                }
            }
        }
    }

    for p in cli_paths {
        load_one_signature_file(p, &compile_opts, &mut seen, &mut out)?;
    }

    Ok(out)
}
