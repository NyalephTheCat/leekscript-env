//! Multi-file loading and include resolution aligned with the reference compiler’s `Folder.resolve`
//! and main-block include pass (path rules, duplicate includes, depth cap).
//!
//! Only top-level `include(...)` statements are followed — same scope as the reference main block.
//!
//! When the resolved path is not a file, the loader and merge pass try `.leek`, `.ls`, and
//! `.leekscript` on the final segment (see [`try_resolve_include_file`]). If the path already uses
//! one of those extensions but the file is missing, the other two extensions are tried on the same stem.

mod limits;
pub mod merge;

pub use limits::IncludeLimits;
pub use merge::{
    MergeIncludesError, MergedSourceMapping, MergedSpanMap, PreludeBuildError,
    merge_included_sources_to_single_file, merge_included_sources_to_single_file_mapped,
    merge_included_sources_to_single_file_mapped_with_overlay, prepend_signatures_to_merged,
};

use sipha::prelude::ParsedDoc;
use sipha::tree::ast::{AstNode, AstNodeExt};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ast::{Root, Stmt};
use crate::parse::{
    LanguageOptions, ParseError, is_signature_stub_path, language_options_with_source_directives,
    parse_doc, parse_signature_doc,
};

/// Resolved project: entry file and all transitively included sources, in **depth-first preorder**
/// (same order as the reference compiler’s first include pass).
#[derive(Debug)]
pub struct LoadedProject {
    pub entry: PathBuf,
    pub files: Vec<LoadedSourceFile>,
}

/// One source file on disk plus its parse tree.
#[derive(Debug)]
pub struct LoadedSourceFile {
    pub path: PathBuf,
    pub source: String,
    pub parsed: ParsedDoc,
}

/// Failure while resolving or loading includes.
#[derive(Debug)]
pub enum IncludeLoadError {
    /// [`std::fs`] error reading a file.
    Io(std::io::Error),
    /// Lex/parse error in a loaded file.
    Parse(PathBuf, ParseError),
    /// Include string could not be resolved to a path (invalid / empty).
    Resolve(PathBuf, ResolveError),
    /// Include target path does not exist or is not a file.
    NotFound {
        from_file: PathBuf,
        include_argument: String,
        resolved: PathBuf,
    },
    /// Distinct-file cap from [`IncludeLimits`] exceeded (reference compiler reports `UNKNOWN_ERROR` for its default).
    TooManyIncludes { max_distinct_files: usize },
    /// Entry path could not be canonicalized or read.
    InvalidEntry(PathBuf, std::io::Error),
}

impl fmt::Display for IncludeLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IncludeLoadError::Io(e) => write!(f, "I/O error: {e}"),
            IncludeLoadError::Parse(p, e) => {
                write!(f, "parse error in {}: {e:?}", p.display())
            }
            IncludeLoadError::Resolve(p, e) => {
                write!(f, "include path resolve error from {}: {e}", p.display())
            }
            IncludeLoadError::NotFound {
                from_file,
                include_argument,
                resolved,
            } => write!(
                f,
                "include {:?} from {} → {} not found",
                include_argument,
                from_file.display(),
                resolved.display()
            ),
            IncludeLoadError::TooManyIncludes { max_distinct_files } => write!(
                f,
                "more than {max_distinct_files} distinct included files (include cap)"
            ),
            IncludeLoadError::InvalidEntry(p, e) => {
                write!(f, "invalid entry {}: {e}", p.display())
            }
        }
    }
}

impl std::error::Error for IncludeLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IncludeLoadError::Io(e) => Some(e),
            IncludeLoadError::InvalidEntry(_, e) => Some(e),
            _ => None,
        }
    }
}

/// Invalid include path string (mirrors `Folder.resolve` preconditions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    EmptyPath,
    /// [`resolve_include_path`] succeeded, but no file exists at that path or with `.leek` / `.ls` / `.leekscript`.
    NoMatchingFile {
        logical: PathBuf,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::EmptyPath => write!(f, "empty include path after normalization"),
            ResolveError::NoMatchingFile { logical } => write!(
                f,
                "no file at {} (tried .leek, .ls, .leekscript)",
                logical.display()
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Resolve an include argument like the reference compiler’s `Folder.resolve` (paths relative to the
/// **directory** of the current file, with `/` from `root_dir`, `./`, `../`, unescaped `/`
/// splitting, and `\\/` → `/` in the final filename segment).
pub fn resolve_include_path(
    root_dir: &Path,
    current_file_dir: &Path,
    path_arg: &str,
) -> Result<PathBuf, ResolveError> {
    resolve_inner(root_dir, current_file_dir, path_arg.trim())
}

/// Default project root for a Leek entry file when the tool does not take an explicit root (LSP,
/// `leekscript check` without `--root`).
///
/// Walks upward from the entry’s parent for a directory containing `main.leek`. If none is found,
/// returns the entry file’s parent so behavior matches a single-folder “project”.
///
/// This matters because `..` in includes cannot escape above [`project_root`](load_project_with_includes)
/// (see [`parent_dir_for_include_resolve`]).
pub fn infer_include_project_root(entry_path: &Path) -> PathBuf {
    let Some(mut dir) = entry_path.parent().map(Path::to_path_buf) else {
        return Path::new("/").to_path_buf();
    };
    let fallback = dir.clone();
    loop {
        if dir.join("main.leek").is_file() {
            return dir;
        }
        let Some(parent) = dir.parent() else {
            return fallback;
        };
        if parent.as_os_str() == dir.as_os_str() {
            return fallback;
        }
        dir = parent.to_path_buf();
    }
}

/// Extensions tried after the path from [`resolve_include_path`] when that path is not a file.
const INCLUDE_FILE_EXTENSIONS: &[&str] = &["leek", "ls", "leekscript"];

fn include_path_candidates(base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.push(base.to_path_buf());

    let ext = base.extension().and_then(|e| e.to_str());
    let has_known_ext = ext.is_some_and(|e| INCLUDE_FILE_EXTENSIONS.contains(&e));

    if ext.is_none() {
        for e in INCLUDE_FILE_EXTENSIONS {
            out.push(base.with_extension(e));
        }
    } else if has_known_ext {
        let without = base.with_extension("");
        for e in INCLUDE_FILE_EXTENSIONS {
            if Some(*e) == ext {
                continue;
            }
            out.push(without.with_extension(e));
        }
    }
    out
}

/// Resolve an include to an existing file path, same rules as [`resolve_include_path`] plus
/// automatic `.leek`, `.ls`, and `.leekscript` suffixes when needed.
pub fn try_resolve_include_file(
    root_dir: &Path,
    current_file_dir: &Path,
    path_arg: &str,
) -> Result<PathBuf, ResolveError> {
    let base = resolve_include_path(root_dir, current_file_dir, path_arg)?;
    include_path_candidates(&base)
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| ResolveError::NoMatchingFile {
            logical: base.clone(),
        })
}

fn resolve_inner(root: &Path, dir: &Path, path: &str) -> Result<PathBuf, ResolveError> {
    if path.is_empty() {
        return Err(ResolveError::EmptyPath);
    }
    if let Some(rest) = path.strip_prefix('/') {
        return resolve_inner(root, root, rest);
    }
    if let Some(rest) = path.strip_prefix("./") {
        return resolve_inner(root, dir, rest);
    }
    if let Some(rest) = path.strip_prefix("../") {
        let parent = parent_dir_for_include_resolve(dir, root);
        return resolve_inner(root, &parent, rest);
    }
    if let Some((prefix, suffix)) = split_first_unescaped_slash(path) {
        let next_dir = dir.join(prefix);
        return resolve_inner(root, &next_dir, suffix);
    }
    let name = unescape_slash_slashes(path);
    Ok(dir.join(name))
}

/// Parent directory for `../` (reference `Folder`: project root’s parent is itself).
fn parent_dir_for_include_resolve(dir: &Path, root: &Path) -> PathBuf {
    if paths_equal(dir, root) {
        root.to_path_buf()
    } else {
        dir.parent()
            .map(Path::to_path_buf)
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| root.to_path_buf())
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let ac: Vec<_> = a.components().collect();
    let bc: Vec<_> = b.components().collect();
    ac == bc
}

fn split_first_unescaped_slash(path: &str) -> Option<(&str, &str)> {
    let b = path.as_bytes();
    let mut i = 1usize;
    while i < b.len() {
        if b[i] == b'/' && b[i - 1] != b'\\' {
            return Some((&path[..i], &path[i + 1..]));
        }
        i += 1;
    }
    None
}

/// Reference `path.replaceAll("\\\\/", "/")` on the final segment.
fn unescape_slash_slashes(s: &str) -> String {
    s.replace(r"\/", "/")
}

fn canonical_file_key(path: &Path) -> Result<PathBuf, std::io::Error> {
    std::fs::canonicalize(path)
}

/// Look up editor buffer text for a path (LSP / IDE): exact path, then canonicalized path.
fn overlay_get<'a>(overlay: &'a HashMap<PathBuf, String>, path: &Path) -> Option<&'a String> {
    overlay
        .get(path)
        .or_else(|| fs::canonicalize(path).ok().and_then(|c| overlay.get(&c)))
}

fn overlay_has(overlay: &HashMap<PathBuf, String>, path: &Path) -> bool {
    overlay_get(overlay, path).is_some()
}

/// Stable dedup key for [`load_file_recursive`]: canonical path when the file exists on disk,
/// otherwise an absolute path when the file is only present in `open_overlay`.
fn file_seen_key(
    path: &Path,
    open_overlay: Option<&HashMap<PathBuf, String>>,
) -> Result<PathBuf, std::io::Error> {
    match canonical_file_key(path) {
        Ok(k) => Ok(k),
        Err(e) => {
            if open_overlay.is_some_and(|o| overlay_has(o, path)) {
                std::path::absolute(path).map_err(|_| e)
            } else {
                Err(e)
            }
        }
    }
}

pub(crate) fn try_resolve_include_file_with_overlay(
    root_dir: &Path,
    current_file_dir: &Path,
    path_arg: &str,
    open_overlay: Option<&HashMap<PathBuf, String>>,
) -> Result<PathBuf, ResolveError> {
    match try_resolve_include_file(root_dir, current_file_dir, path_arg) {
        Ok(p) => Ok(p),
        Err(ResolveError::EmptyPath) => Err(ResolveError::EmptyPath),
        Err(ResolveError::NoMatchingFile { logical }) => {
            if let Some(ov) = open_overlay {
                for cand in include_path_candidates(&logical) {
                    if overlay_has(ov, &cand) {
                        return Ok(fs::canonicalize(&cand).unwrap_or(cand));
                    }
                }
            }
            Err(ResolveError::NoMatchingFile { logical })
        }
    }
}

/// Load `entry` (relative to `project_root` or absolute) and all files reached by top-level
/// `include("...")`, in depth-first preorder, using [`IncludeLimits::REFERENCE`].
///
/// For a custom cap, use [`load_project_with_includes_limited`].
pub fn load_project_with_includes(
    project_root: impl AsRef<Path>,
    entry: impl AsRef<Path>,
    lang: impl Into<LanguageOptions>,
) -> Result<LoadedProject, IncludeLoadError> {
    load_project_with_includes_limited(project_root, entry, lang, IncludeLimits::REFERENCE)
}

/// Like [`load_project_with_includes`], but with an explicit [`IncludeLimits`] (e.g.
/// [`IncludeLimits::UNLIMITED`]).
pub fn load_project_with_includes_limited(
    project_root: impl AsRef<Path>,
    entry: impl AsRef<Path>,
    lang: impl Into<LanguageOptions>,
    limits: IncludeLimits,
) -> Result<LoadedProject, IncludeLoadError> {
    load_project_with_includes_limited_with_overlay(project_root, entry, lang, limits, None)
}

/// Like [`load_project_with_includes_limited`], but uses `open_overlay` for any path present in the
/// map before reading the filesystem. Missing include targets can be satisfied from the overlay
/// alone (e.g. a new file open in the editor but not yet written to disk).
///
/// Keys should be normal file paths; values are full file text. The LSP typically keys by
/// `canonicalize(path)` when possible so lookups match resolved include paths.
pub fn load_project_with_includes_limited_with_overlay(
    project_root: impl AsRef<Path>,
    entry: impl AsRef<Path>,
    lang: impl Into<LanguageOptions>,
    limits: IncludeLimits,
    open_overlay: Option<&HashMap<PathBuf, String>>,
) -> Result<LoadedProject, IncludeLoadError> {
    let lang = lang.into();
    let project_root = project_root.as_ref();
    let root_canon = fs::canonicalize(project_root)
        .map_err(|e| IncludeLoadError::InvalidEntry(project_root.to_path_buf(), e))?;
    let entry_path = {
        let e = entry.as_ref();
        let p = if e.is_absolute() {
            e.to_path_buf()
        } else {
            root_canon.join(e)
        };
        fs::canonicalize(&p).map_err(|e| IncludeLoadError::InvalidEntry(p, e))?
    };

    let mut seen = HashSet::<PathBuf>::new();
    let mut files = Vec::new();
    load_file_recursive(
        &entry_path,
        &root_canon,
        lang,
        limits,
        &mut seen,
        &mut files,
        open_overlay,
    )?;

    Ok(LoadedProject {
        entry: entry_path,
        files,
    })
}

fn load_file_recursive(
    file_path: &Path,
    root_dir: &Path,
    lang: LanguageOptions,
    limits: IncludeLimits,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<LoadedSourceFile>,
    open_overlay: Option<&HashMap<PathBuf, String>>,
) -> Result<(), IncludeLoadError> {
    let key = file_seen_key(file_path, open_overlay).map_err(IncludeLoadError::Io)?;
    if seen.contains(&key) {
        return Ok(());
    }
    if seen.len() > limits.max_distinct_files {
        return Err(IncludeLoadError::TooManyIncludes {
            max_distinct_files: limits.max_distinct_files,
        });
    }
    seen.insert(key);

    let source = if let Some(ov) = open_overlay {
        if let Some(s) = overlay_get(ov, file_path) {
            s.clone()
        } else {
            fs::read_to_string(file_path).map_err(IncludeLoadError::Io)?
        }
    } else {
        fs::read_to_string(file_path).map_err(IncludeLoadError::Io)?
    };
    let parsed = if is_signature_stub_path(file_path) {
        parse_signature_doc(&source, lang)
    } else {
        parse_doc(&source, lang)
    }
    .map_err(|e| IncludeLoadError::Parse(file_path.to_path_buf(), e))?;

    let root_for_walk = parsed.root().clone();
    out.push(LoadedSourceFile {
        path: file_path.to_path_buf(),
        source,
        parsed,
    });

    let current_dir = file_path.parent().unwrap_or(root_dir).to_path_buf();

    let Some(root_node) = Root::cast(root_for_walk) else {
        return Ok(());
    };

    for stmt in AstNodeExt::children::<Stmt>(root_node.syntax()) {
        let Stmt::Include(inc) = stmt else {
            continue;
        };
        let Some(lit) = inc.path() else {
            continue;
        };
        let arg = lit.value();
        let resolved =
            match try_resolve_include_file_with_overlay(root_dir, &current_dir, &arg, open_overlay)
            {
                Ok(p) => p,
                Err(ResolveError::EmptyPath) => {
                    return Err(IncludeLoadError::Resolve(
                        file_path.to_path_buf(),
                        ResolveError::EmptyPath,
                    ));
                }
                Err(ResolveError::NoMatchingFile { logical }) => {
                    return Err(IncludeLoadError::NotFound {
                        from_file: file_path.to_path_buf(),
                        include_argument: arg,
                        resolved: logical,
                    });
                }
            };
        load_file_recursive(&resolved, root_dir, lang, limits, seen, out, open_overlay)?;
    }

    Ok(())
}

/// One merged buffer ready for `parse_*` / semantic analysis: includes expanded, signatures
/// prepended, and [`LanguageOptions`] resolved from leading `leeklang:` directives in `combined`.
#[derive(Debug)]
pub struct MergedCheckUnit {
    pub combined: String,
    pub mapping: MergedSourceMapping,
    pub project: LoadedProject,
    pub resolved: LanguageOptions,
    pub use_signature_grammar: bool,
}

/// Aggregate error for [`prepare_merged_check_unit`].
#[derive(Debug)]
pub enum MergedCheckPrepError {
    Load(IncludeLoadError),
    Merge(MergeIncludesError),
    Prelude(PreludeBuildError),
}

impl fmt::Display for MergedCheckPrepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergedCheckPrepError::Load(e) => e.fmt(f),
            MergedCheckPrepError::Merge(e) => e.fmt(f),
            MergedCheckPrepError::Prelude(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for MergedCheckPrepError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MergedCheckPrepError::Load(e) => Some(e),
            MergedCheckPrepError::Merge(e) => Some(e),
            MergedCheckPrepError::Prelude(e) => Some(e),
        }
    }
}

/// Load entry + transitive includes, merge to one string, prepend signature files, then apply
/// source directives to obtain parse options. Used by `leekscript check`, `merge`, `format
/// --merge-includes`, and the language server.
///
/// Pass `overlay: None` for disk-only loading (CLI). Pass open-document buffers for LSP-style
/// overlay (same map as [`load_project_with_includes_limited_with_overlay`]). Merge uses
/// [`merge_included_sources_to_single_file_mapped_with_overlay`] with the same map so includes that
/// exist only in open buffers (not on disk) still expand correctly.
#[must_use]
pub fn prepare_merged_check_unit(
    project_root: &Path,
    entry_path: &Path,
    lang: LanguageOptions,
    signature_files: &[PathBuf],
    overlay: Option<&HashMap<PathBuf, String>>,
) -> Result<MergedCheckUnit, MergedCheckPrepError> {
    let project = if let Some(o) = overlay {
        load_project_with_includes_limited_with_overlay(
            project_root,
            entry_path,
            lang,
            IncludeLimits::REFERENCE,
            Some(o),
        )
        .map_err(MergedCheckPrepError::Load)?
    } else {
        load_project_with_includes(project_root, entry_path, lang)
            .map_err(MergedCheckPrepError::Load)?
    };

    let (merged, map) =
        merge_included_sources_to_single_file_mapped_with_overlay(project_root, &project, overlay)
            .map_err(MergedCheckPrepError::Merge)?;

    let (combined, mapping) = prepend_signatures_to_merged(lang, signature_files, &merged, map)
        .map_err(MergedCheckPrepError::Prelude)?;

    let resolved = language_options_with_source_directives(&combined, lang);
    let use_signature_grammar = !signature_files.is_empty() || is_signature_stub_path(entry_path);

    Ok(MergedCheckUnit {
        combined,
        mapping,
        project,
        resolved,
        use_signature_grammar,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Version;

    #[test]
    fn resolve_absolute_from_root() {
        let root = Path::new("/project");
        let dir = Path::new("/project/ai/sub");
        let p = resolve_include_path(root, dir, "/ai/bonjour.leek").unwrap();
        assert_eq!(p, Path::new("/project/ai/bonjour.leek"));
    }

    #[test]
    fn resolve_dot_dot_at_root_stays_at_root() {
        let root = Path::new("/res");
        let dir = Path::new("/res");
        let p = resolve_include_path(root, dir, "../bonjour.leek").unwrap();
        assert_eq!(p, Path::new("/res/bonjour.leek"));
    }

    #[test]
    fn resolve_unescaped_slash_splits_subfolder() {
        let root = Path::new("/res");
        let dir = Path::new("/res/ai");
        let p = resolve_include_path(root, dir, "subfolder/sub.leek").unwrap();
        assert_eq!(p, Path::new("/res/ai/subfolder/sub.leek"));
    }

    #[test]
    fn resolve_final_segment_unescapes_slash() {
        let root = Path::new("/res");
        let dir = Path::new("/res/ai");
        let p = resolve_include_path(root, dir, r"foo\/bar.leek").unwrap();
        assert_eq!(p, Path::new("/res/ai/foo/bar.leek"));
    }

    #[test]
    fn try_resolve_adds_leek_suffix() {
        let root = std::env::temp_dir().join(format!("leek_try_leek_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("m.leek"), "1;\n").unwrap();
        let got = try_resolve_include_file(&root, &root, "m").unwrap();
        assert_eq!(got.file_name().unwrap(), "m.leek");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn try_resolve_prefers_exact_path_then_leek() {
        let root = std::env::temp_dir().join(format!("leek_try_order_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let f = root.join("m");
        std::fs::write(&f, "a;\n").unwrap();
        std::fs::write(root.join("m.leek"), "b;\n").unwrap();
        let got = try_resolve_include_file(&root, &root, "m").unwrap();
        assert_eq!(got, f);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn try_resolve_uses_ls_when_no_leek() {
        let root = std::env::temp_dir().join(format!("leek_try_ls_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("m.ls"), "1;\n").unwrap();
        let got = try_resolve_include_file(&root, &root, "m").unwrap();
        assert_eq!(got.file_name().unwrap(), "m.ls");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn try_resolve_leekscript_suffix() {
        let root = std::env::temp_dir().join(format!("leek_try_lss_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("x.leekscript"), "1;\n").unwrap();
        let got = try_resolve_include_file(&root, &root, "x").unwrap();
        assert_eq!(got.file_name().unwrap(), "x.leekscript");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn infer_root_finds_main_leek_ancestor() {
        let base = std::env::temp_dir().join(format!("leek_infer_root_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("registries")).unwrap();
        std::fs::write(base.join("main.leek"), "1;\n").unwrap();
        let entry = base.join("registries").join("X.leek");
        std::fs::write(&entry, "1;\n").unwrap();
        assert_eq!(infer_include_project_root(&entry), base);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn infer_root_falls_back_to_parent() {
        let base = std::env::temp_dir().join(format!("leek_infer_fallback_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("solo")).unwrap();
        let entry = base.join("solo").join("a.leek");
        std::fs::write(&entry, "1;\n").unwrap();
        assert_eq!(infer_include_project_root(&entry), base.join("solo"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn overlay_resolves_include_not_on_disk() {
        let base = std::env::temp_dir().join(format!("leek_overlay_inc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let main = base.join("main.leek");
        std::fs::write(&main, "include(\"helper\");\n").unwrap();
        let helper_path = base.join("helper.leek");
        let mut overlay = HashMap::new();
        overlay.insert(helper_path.clone(), "2;\n".to_string());
        let p = load_project_with_includes_limited_with_overlay(
            &base,
            &main,
            Version::V4,
            IncludeLimits::REFERENCE,
            Some(&overlay),
        )
        .unwrap();
        assert_eq!(p.files.len(), 2);
        assert_eq!(p.files[1].path, helper_path);
        assert_eq!(p.files[1].source, "2;\n");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn prepare_merged_check_unit_single_entry_no_overlay() {
        let base = std::env::temp_dir().join(format!("leek_prep_unit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let main = base.join("main.leek");
        std::fs::write(&main, "var x = 1;\n").unwrap();
        let lang = crate::parse::LanguageOptions::v4_experimental_all();
        let prep = prepare_merged_check_unit(&base, &main, lang, &[], None).unwrap();
        assert!(prep.combined.contains("var x"));
        assert!(!prep.use_signature_grammar);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn prepare_merged_check_unit_overlay_include_not_on_disk() {
        let base = std::env::temp_dir().join(format!("leek_prep_virt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let main = base.join("main.leek");
        std::fs::write(&main, "include(\"helper\");\n").unwrap();
        let helper_path = base.join("helper.leek");
        let mut overlay = HashMap::new();
        overlay.insert(helper_path.clone(), "var z = 3;\n".to_string());
        let lang = crate::parse::LanguageOptions::v4_experimental_all();
        let prep = prepare_merged_check_unit(&base, &main, lang, &[], Some(&overlay)).unwrap();
        assert_eq!(prep.project.files.len(), 2);
        assert!(prep.combined.contains("var z"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn prepare_merged_check_unit_overlay_overrides_entry_buffer() {
        let base = std::env::temp_dir().join(format!("leek_prep_ov_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let main = base.join("main.leek");
        std::fs::write(&main, "// stale on disk\n").unwrap();
        let helper = base.join("helper.leek");
        std::fs::write(&helper, "var y = 2;\n").unwrap();
        let mut overlay = HashMap::new();
        overlay.insert(
            std::fs::canonicalize(&main).unwrap(),
            "include(\"helper.leek\");\n".to_string(),
        );
        let lang = crate::parse::LanguageOptions::v4_experimental_all();
        let prep = prepare_merged_check_unit(&base, &main, lang, &[], Some(&overlay)).unwrap();
        assert_eq!(prep.project.files.len(), 2);
        assert!(prep.combined.contains("var y"));
        assert!(!prep.combined.contains("stale"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
