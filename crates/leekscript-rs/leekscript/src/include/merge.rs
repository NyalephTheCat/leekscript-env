//! Merge a loaded include graph into one source string: expand top-level `include(...)` like the
//! loader, emit a metadata line before each file’s body the first time it is inlined, and emit a
//! short comment at duplicate include sites so diamond graphs do not duplicate definitions.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sipha::tree::ast::{AstNode, AstNodeExt};
use sipha::tree::red::SyntaxNode;
use sipha::types::{IntoSyntaxKind, Span};

use crate::ast::{ReturnStmt, Root, Stmt};
use crate::parse::LanguageOptions;
use crate::syntax::kinds::{Lex, Node};

use super::{
    IncludeLoadError, LoadedProject, LoadedSourceFile, ResolveError, load_project_with_includes,
};

/// Maps byte offsets in merged include output back to a concrete source file and offset.
#[derive(Debug, Clone, Default)]
pub struct MergedSourceMapping {
    /// Non-overlapping spans in merge order (each piece of copied source).
    pub spans: Vec<MergedSpanMap>,
}

/// One contiguous slice of merged text copied from [`MergedSpanMap::path`].
#[derive(Debug, Clone)]
pub struct MergedSpanMap {
    pub merged_start: u32,
    pub merged_end: u32,
    pub path: PathBuf,
    /// Byte offset in that file’s UTF-8 source where this slice starts.
    pub file_offset: u32,
}

impl MergedSourceMapping {
    /// Returns the span map entry containing `merged_offset`, if any.
    #[must_use]
    pub fn span_at_merged_offset(&self, merged_offset: u32) -> Option<&MergedSpanMap> {
        let i = self
            .spans
            .partition_point(|s| s.merged_start <= merged_offset);
        let idx = i.checked_sub(1)?;
        let s = self.spans.get(idx)?;
        if merged_offset < s.merged_end {
            Some(s)
        } else {
            None
        }
    }

    /// Maps a UTF-8 byte offset in `file_path`’s source to the corresponding offset in the merged
    /// buffer (after signature prelude and include expansion). Used by the LSP to relate editor
    /// cursors to merged parse trees.
    ///
    /// Paths are matched using [`fs::canonicalize`] when possible; falls back to [`Path`] equality.
    #[must_use]
    pub fn merged_offset_for_file_byte(&self, file_path: &Path, file_byte: u32) -> Option<u32> {
        fn path_key(p: &Path) -> PathBuf {
            fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
        }
        let want = path_key(file_path);
        for s in &self.spans {
            let span_path = path_key(&s.path);
            if span_path != want {
                continue;
            }
            let chunk = s.merged_end.saturating_sub(s.merged_start);
            let end_file = s.file_offset.saturating_add(chunk);
            if file_byte >= s.file_offset && file_byte < end_file {
                return Some(s.merged_start + (file_byte - s.file_offset));
            }
        }
        None
    }
}

/// Failure while merging includes into one buffer.
#[derive(Debug)]
pub enum MergeIncludesError {
    /// I/O when building stable path keys ([`fs::canonicalize`], or [`std::path::absolute`] for
    /// overlay-only files that are not on disk).
    Io(PathBuf, std::io::Error),
    /// Include argument could not be resolved (invalid / empty path).
    Resolve(PathBuf, ResolveError),
    /// Resolved path is not present in `project.files` (inconsistent project).
    MissingLoadedFile(PathBuf),
}

impl fmt::Display for MergeIncludesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeIncludesError::Io(p, e) => {
                write!(f, "could not build merge path key for {}: {e}", p.display())
            }
            MergeIncludesError::Resolve(p, e) => {
                write!(f, "include path resolve error from {}: {e}", p.display())
            }
            MergeIncludesError::MissingLoadedFile(p) => {
                write!(
                    f,
                    "internal error: {} not found in loaded project files",
                    p.display()
                )
            }
        }
    }
}

impl std::error::Error for MergeIncludesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MergeIncludesError::Io(_, e) => Some(e),
            _ => None,
        }
    }
}

/// Failure while building the `--signatures` prelude (I/O, include load, or merge).
#[derive(Debug)]
pub enum PreludeBuildError {
    Io(PathBuf, std::io::Error),
    /// Loading the signature entry or a transitively included file failed.
    IncludeLoad {
        signature_entry: PathBuf,
        source: IncludeLoadError,
    },
    /// Expanding top-level `include` statements in the loaded signature bundle failed.
    Merge {
        signature_entry: PathBuf,
        source: MergeIncludesError,
    },
}

impl fmt::Display for PreludeBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreludeBuildError::Io(p, e) => write!(f, "{}: {e}", p.display()),
            PreludeBuildError::IncludeLoad {
                signature_entry,
                source,
            } => write!(f, "`--signatures` {}: {source}", signature_entry.display()),
            PreludeBuildError::Merge {
                signature_entry,
                source,
            } => write!(
                f,
                "`--signatures` {} (merge): {source}",
                signature_entry.display()
            ),
        }
    }
}

impl std::error::Error for PreludeBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PreludeBuildError::Io(_, e) => Some(e),
            PreludeBuildError::IncludeLoad { source, .. } => Some(source),
            PreludeBuildError::Merge { source, .. } => Some(source),
        }
    }
}

/// Prepend one or more “header” sources (stdlib / API signatures) before merged check input.
///
/// Each path is treated as a **bundle entry**: the file is loaded with
/// [`super::load_project_with_includes`] using **its parent directory** as the project root, then
/// top-level `include("…")` statements are expanded like [`merge_included_sources_to_single_file_mapped`].
/// Bundles are concatenated in order, separated by a single `\n`. If the prelude is non-empty, one
/// more `\n` is inserted before `merged` so the prelude and check input stay separate top-level
/// regions. [`MergedSourceMapping`] spans from each bundle are shifted; the user body mapping is
/// shifted by the total prelude length.
///
/// With an empty `signature_paths` slice, returns `(merged.to_string(), merged_mapping)` without
/// copying the mapping.
pub fn prepend_signatures_to_merged(
    lang: impl Into<LanguageOptions>,
    signature_paths: &[PathBuf],
    merged: &str,
    merged_mapping: MergedSourceMapping,
) -> Result<(String, MergedSourceMapping), PreludeBuildError> {
    let lang = lang.into();
    if signature_paths.is_empty() {
        return Ok((merged.to_string(), merged_mapping));
    }

    let mut prelude = String::new();
    let mut prelude_spans: Vec<MergedSpanMap> = Vec::new();

    for (i, path) in signature_paths.iter().enumerate() {
        if i > 0 {
            prelude.push('\n');
        }
        let base = prelude.len() as u32;

        let canon = fs::canonicalize(path).map_err(|e| PreludeBuildError::Io(path.clone(), e))?;
        let sig_root = canon.parent().unwrap_or_else(|| Path::new("/"));
        let sig_root = fs::canonicalize(sig_root)
            .map_err(|e| PreludeBuildError::Io(sig_root.to_path_buf(), e))?;

        let project = load_project_with_includes(&sig_root, &canon, lang).map_err(|e| {
            PreludeBuildError::IncludeLoad {
                signature_entry: canon.clone(),
                source: e,
            }
        })?;

        let (text, mut chunk_map) =
            merge_included_sources_to_single_file_mapped(&sig_root, &project).map_err(|e| {
                PreludeBuildError::Merge {
                    signature_entry: canon,
                    source: e,
                }
            })?;

        prelude.push_str(&text);
        for s in &mut chunk_map.spans {
            s.merged_start = s.merged_start.saturating_add(base);
            s.merged_end = s.merged_end.saturating_add(base);
        }
        prelude_spans.extend(chunk_map.spans);
    }

    let shift = if prelude.is_empty() {
        0u32
    } else {
        prelude.push('\n');
        prelude.len() as u32
    };

    let mut full = prelude;
    full.push_str(merged);

    let mut spans = prelude_spans;
    for mut s in merged_mapping.spans {
        s.merged_start = s.merged_start.saturating_add(shift);
        s.merged_end = s.merged_end.saturating_add(shift);
        spans.push(s);
    }

    Ok((full, MergedSourceMapping { spans }))
}

/// Stable key for deduplicating [`LoadedProject`] files during merge (must match loader resolution).
fn merge_path_key(path: &Path) -> Result<PathBuf, MergeIncludesError> {
    match fs::canonicalize(path) {
        Ok(p) => Ok(p),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            std::path::absolute(path).map_err(|e2| MergeIncludesError::Io(path.to_path_buf(), e2))
        }
        Err(e) => Err(MergeIncludesError::Io(path.to_path_buf(), e)),
    }
}

fn display_path_relative(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn build_file_index(
    project: &LoadedProject,
) -> Result<HashMap<PathBuf, usize>, MergeIncludesError> {
    let mut map = HashMap::new();
    for (i, file) in project.files.iter().enumerate() {
        let key = merge_path_key(&file.path)?;
        map.insert(key, i);
    }
    Ok(map)
}

/// Byte offsets in the source file where a bare `return` (no value, no `;`) ends — insert `;` here.
fn collect_bare_return_insert_offsets(root: &SyntaxNode) -> Vec<u32> {
    let mut inserts = Vec::new();
    for n in root.find_all_nodes(Node::ReturnStmt.into_syntax_kind()) {
        let Some(r) = ReturnStmt::cast(n.clone()) else {
            continue;
        };
        if r.expr().is_some() {
            continue;
        }
        if n.non_trivia_tokens()
            .any(|t| t.kind_as::<Lex>() == Some(Lex::Semi))
        {
            continue;
        }
        inserts.push(n.text_range().end);
    }
    inserts.sort_unstable();
    inserts.dedup();
    inserts
}

fn push_mapped_segment(
    out: &mut String,
    mapping: &mut MergedSourceMapping,
    path: &Path,
    src: &[u8],
    file_seg_start: u32,
    file_seg_end: u32,
) {
    let start = file_seg_start as usize;
    let end = (file_seg_end as usize).min(src.len());
    if start >= end {
        return;
    }
    let merged_start = out.len() as u32;
    out.push_str(std::str::from_utf8(&src[start..end]).unwrap_or(""));
    let merged_end = out.len() as u32;
    if merged_end > merged_start {
        mapping.spans.push(MergedSpanMap {
            merged_start,
            merged_end,
            path: path.to_path_buf(),
            file_offset: file_seg_start,
        });
    }
}

fn push_mapped_semicolon(
    out: &mut String,
    mapping: &mut MergedSourceMapping,
    path: &Path,
    file_offset_at_insert: u32,
) {
    let merged_start = out.len() as u32;
    out.push(';');
    let merged_end = out.len() as u32;
    mapping.spans.push(MergedSpanMap {
        merged_start,
        merged_end,
        path: path.to_path_buf(),
        file_offset: file_offset_at_insert,
    });
}

/// Copy `src[range]` into `out`, inserting `;` after each bare `return` in `subtree` (see
/// [`collect_bare_return_insert_offsets`]).
fn emit_source_range_with_bare_return_fixes(
    out: &mut String,
    path: &Path,
    src: &[u8],
    range: Span,
    subtree: &SyntaxNode,
    mapping: &mut MergedSourceMapping,
) {
    let range_end = range.end.min(src.len() as u32);
    if range.start > range_end {
        return;
    }

    let mut inserts = collect_bare_return_insert_offsets(subtree);
    inserts.retain(|&p| p >= range.start && p <= range.end && p <= range_end);
    inserts.sort_unstable();
    inserts.dedup();

    let start = range.start as usize;
    let end = range_end as usize;

    if inserts.is_empty() {
        if start <= src.len() && start < end {
            push_mapped_segment(out, mapping, path, src, range.start, range_end);
        } else {
            let merged_start = out.len() as u32;
            out.push_str(&subtree.collect_text());
            let merged_end = out.len() as u32;
            if merged_end > merged_start {
                mapping.spans.push(MergedSpanMap {
                    merged_start,
                    merged_end,
                    path: path.to_path_buf(),
                    file_offset: range.start,
                });
            }
        }
        return;
    }

    let mut cursor = range.start;
    for &ins in &inserts {
        if ins < cursor || ins > range_end {
            continue;
        }
        push_mapped_segment(out, mapping, path, src, cursor, ins);
        push_mapped_semicolon(out, mapping, path, ins);
        cursor = ins;
    }
    push_mapped_segment(out, mapping, path, src, cursor, range_end);
}

fn emit_stmt_text(
    out: &mut String,
    file: &LoadedSourceFile,
    stmt: &Stmt,
    mapping: &mut MergedSourceMapping,
) {
    let src = file.parsed.source();
    let range = stmt.syntax().text_range();
    emit_source_range_with_bare_return_fixes(
        out,
        &file.path,
        src,
        range,
        stmt.syntax(),
        mapping,
    );
}

/// Expand top-level includes and concatenate into one UTF-8 string.
///
/// - The **entry** file is copied as-is except each top-level `include("…")` is either replaced by
///   the included file’s expansion (first time that file appears) or by a one-line metadata comment
///   if that file was already merged earlier in the graph.
/// - Each time a file’s body is inlined for the first time, a `// leekscript-include: begin …`
///   line is written immediately before its top-level statements (nested `include` statements are
///   handled the same way).
/// - **Bare `return`** (no value and no `;`) is rewritten to `return;` everywhere in emitted
///   source, including inside function bodies, so merged output matches the `return;` style rule.
///
/// `project_root` must be the same directory passed to [`super::load_project_with_includes`] (it is
/// canonicalized the same way for resolution).
pub fn merge_included_sources_to_single_file(
    project_root: impl AsRef<Path>,
    project: &LoadedProject,
) -> Result<String, MergeIncludesError> {
    merge_included_sources_to_single_file_mapped(project_root, project).map(|(s, _)| s)
}

/// Like [`merge_included_sources_to_single_file`], plus a mapping from merged byte offsets to
/// original file paths and offsets (for diagnostics).
pub fn merge_included_sources_to_single_file_mapped(
    project_root: impl AsRef<Path>,
    project: &LoadedProject,
) -> Result<(String, MergedSourceMapping), MergeIncludesError> {
    merge_included_sources_to_single_file_mapped_with_overlay(project_root, project, None)
}

/// Like [`merge_included_sources_to_single_file_mapped`], but uses the same `open_overlay` as
/// [`super::load_project_with_includes_limited_with_overlay`] so includes that exist only in memory
/// resolve identically during merge expansion.
pub fn merge_included_sources_to_single_file_mapped_with_overlay(
    project_root: impl AsRef<Path>,
    project: &LoadedProject,
    open_overlay: Option<&HashMap<PathBuf, String>>,
) -> Result<(String, MergedSourceMapping), MergeIncludesError> {
    let project_root = project_root.as_ref();
    let root_dir = fs::canonicalize(project_root)
        .map_err(|e| MergeIncludesError::Io(project_root.to_path_buf(), e))?;

    let index = build_file_index(project)?;
    let entry_key = merge_path_key(&project.entry)?;
    let Some(&entry_idx) = index.get(&entry_key) else {
        return Err(MergeIncludesError::MissingLoadedFile(project.entry.clone()));
    };
    let entry_file = &project.files[entry_idx];

    let mut emitted = HashSet::<PathBuf>::new();
    let mut out = String::new();
    let mut mapping = MergedSourceMapping::default();
    emit_top_level(
        entry_file,
        &root_dir,
        project,
        &index,
        &mut emitted,
        &mut out,
        &mut mapping,
        open_overlay,
    )?;
    Ok((out, mapping))
}

fn emit_top_level(
    file: &LoadedSourceFile,
    root_dir: &Path,
    project: &LoadedProject,
    index: &HashMap<PathBuf, usize>,
    emitted: &mut HashSet<PathBuf>,
    out: &mut String,
    mapping: &mut MergedSourceMapping,
    open_overlay: Option<&HashMap<PathBuf, String>>,
) -> Result<(), MergeIncludesError> {
    let Some(root_node) = Root::cast(file.parsed.root().clone()) else {
        let src = file.parsed.source();
        let n = src.len() as u32;
        emit_source_range_with_bare_return_fixes(
            out,
            &file.path,
            src,
            Span::new(0, n),
            file.parsed.root(),
            mapping,
        );
        return Ok(());
    };

    let current_dir = file.path.parent().unwrap_or(root_dir).to_path_buf();

    for stmt in AstNodeExt::children::<Stmt>(root_node.syntax()) {
        match &stmt {
            Stmt::Include(inc) => {
                let Some(lit) = inc.path() else {
                    emit_stmt_text(out, file, &stmt, mapping);
                    continue;
                };
                let arg = lit.value();
                let resolved = super::try_resolve_include_file_with_overlay(
                    root_dir,
                    &current_dir,
                    &arg,
                    open_overlay,
                )
                .map_err(|e| match e {
                    ResolveError::EmptyPath => MergeIncludesError::Resolve(file.path.clone(), e),
                    ResolveError::NoMatchingFile { logical } => {
                        MergeIncludesError::MissingLoadedFile(logical)
                    }
                })?;
                let key = merge_path_key(&resolved)?;
                let rel = display_path_relative(root_dir, &key);

                if emitted.contains(&key) {
                    out.push_str("// leekscript-include: already merged: ");
                    out.push_str(&rel);
                    out.push('\n');
                    continue;
                }

                let Some(&idx) = index.get(&key) else {
                    return Err(MergeIncludesError::MissingLoadedFile(resolved));
                };
                let included = &project.files[idx];
                emitted.insert(key);

                out.push_str("// leekscript-include: begin ");
                out.push_str(&rel);
                out.push('\n');
                emit_top_level(
                    included,
                    root_dir,
                    project,
                    index,
                    emitted,
                    out,
                    mapping,
                    open_overlay,
                )?;
            }
            _ => emit_stmt_text(out, file, &stmt, mapping),
        }
    }

    Ok(())
}

#[cfg(test)]
mod merged_offset_tests {
    use super::{MergedSourceMapping, MergedSpanMap};
    use std::path::PathBuf;

    #[test]
    fn merged_offset_for_file_byte_round_trip() {
        let p = PathBuf::from("/proj/a.leek");
        let mut m = MergedSourceMapping::default();
        m.spans.push(MergedSpanMap {
            merged_start: 100,
            merged_end: 105,
            path: p.clone(),
            file_offset: 10,
        });
        assert_eq!(m.merged_offset_for_file_byte(&p, 12), Some(102));
        assert_eq!(m.merged_offset_for_file_byte(&p, 9), None);
        assert_eq!(m.merged_offset_for_file_byte(&p, 15), None);
    }
}
