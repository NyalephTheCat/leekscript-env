//! Merge a loaded include graph into one source string: expand top-level `include(...)` like the
//! loader, emit a metadata line before each file’s body the first time it is inlined, and emit a
//! short comment at duplicate include sites so diamond graphs do not duplicate definitions.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use sipha::tree::ast::{AstNode, AstNodeExt};

use crate::ast::{Root, Stmt};

use super::{LoadedProject, LoadedSourceFile, ResolveError, try_resolve_include_file};

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
        let i = self.spans.partition_point(|s| s.merged_start <= merged_offset);
        let idx = i.checked_sub(1)?;
        let s = self.spans.get(idx)?;
        if merged_offset < s.merged_end {
            Some(s)
        } else {
            None
        }
    }
}

/// Failure while merging includes into one buffer.
#[derive(Debug)]
pub enum MergeIncludesError {
    /// Same as [`fs::canonicalize`] when building path keys.
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
                write!(f, "could not canonicalize {}: {e}", p.display())
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

fn canonical_key(path: &Path) -> Result<PathBuf, MergeIncludesError> {
    fs::canonicalize(path).map_err(|e| MergeIncludesError::Io(path.to_path_buf(), e))
}

fn display_path_relative(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn build_file_index(project: &LoadedProject) -> Result<HashMap<PathBuf, usize>, MergeIncludesError> {
    let mut map = HashMap::new();
    for (i, file) in project.files.iter().enumerate() {
        let key = canonical_key(&file.path)?;
        map.insert(key, i);
    }
    Ok(map)
}

fn emit_stmt_text(
    out: &mut String,
    file: &LoadedSourceFile,
    stmt: &Stmt,
    mapping: &mut MergedSourceMapping,
) {
    let src = file.parsed.source();
    let range = stmt.syntax().text_range();
    let start = range.start as usize;
    let end = (range.end as usize).min(src.len());
    let merged_start = out.len() as u32;
    let file_offset = range.start;
    if start <= src.len() && start < end {
        let slice = &src[start..end];
        out.push_str(std::str::from_utf8(slice).unwrap_or(""));
    } else {
        out.push_str(&stmt.syntax().collect_text());
    }
    let merged_end = out.len() as u32;
    if merged_end > merged_start {
        mapping.spans.push(MergedSpanMap {
            merged_start,
            merged_end,
            path: file.path.clone(),
            file_offset,
        });
    }
}

/// Expand top-level includes and concatenate into one UTF-8 string.
///
/// - The **entry** file is copied as-is except each top-level `include("…")` is either replaced by
///   the included file’s expansion (first time that file appears) or by a one-line metadata comment
///   if that file was already merged earlier in the graph.
/// - Each time a file’s body is inlined for the first time, a `// leekscript-include: begin …`
///   line is written immediately before its top-level statements (nested `include` statements are
///   handled the same way).
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
    let project_root = project_root.as_ref();
    let root_dir = fs::canonicalize(project_root)
        .map_err(|e| MergeIncludesError::Io(project_root.to_path_buf(), e))?;

    let index = build_file_index(project)?;
    let entry_key = canonical_key(&project.entry)?;
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
) -> Result<(), MergeIncludesError> {
    let Some(root_node) = Root::cast(file.parsed.root().clone()) else {
        let merged_start = out.len() as u32;
        out.push_str(file.parsed.source_str());
        let merged_end = out.len() as u32;
        if merged_end > merged_start {
            mapping.spans.push(MergedSpanMap {
                merged_start,
                merged_end,
                path: file.path.clone(),
                file_offset: 0,
            });
        }
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
                let resolved = try_resolve_include_file(root_dir, &current_dir, &arg).map_err(
                    |e| match e {
                        ResolveError::EmptyPath => {
                            MergeIncludesError::Resolve(file.path.clone(), e)
                        }
                        ResolveError::NoMatchingFile { logical } => {
                            MergeIncludesError::MissingLoadedFile(logical)
                        }
                    },
                )?;
                let key = canonical_key(&resolved)?;
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
                emit_top_level(included, root_dir, project, index, emitted, out, mapping)?;
            }
            _ => emit_stmt_text(out, file, &stmt, mapping),
        }
    }

    Ok(())
}
