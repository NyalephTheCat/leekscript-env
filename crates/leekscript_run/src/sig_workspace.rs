//! Workspace `data/signatures/*.sig.leek` bundles (edit those files; Rust loads them via `include_str!`).
//!
//! - [`CORE_SIG_LEEK`]: stdlib / type constants (`core.sig.leek`)
//! - [`LEEKWARS_SIG_LEEK`]: Leek Wars fight constants + API stubs (`leekwars.sig.leek`)
//!
//! [`merged_workspace_sig_bundle`] merges both for tools that need the full Leek Wars compile surface.
//!
//! **Note:** `core.sig.leek` contains multi-line `function` signatures the green-tree parser does not
//! accept as a single file today. We still load it in an **editable** way by stripping doc comments
//! and scanning for `global` / `function` declarations (same names and integer literals HIR would
//! expose for the supported subset). `leekwars.sig.leek` uses full [`parse_sig_leek`].

use crate::pipeline::{parse_sig_leek, CompileOptions, SigLeekUnit};
use regex::Regex;
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

/// `data/signatures/*.sig.leek` use JavaDoc-style `/* … */` blocks; strip them so the normal lexer/HIR path can run.
///
/// Does not try to model string literals (signature fixtures rarely embed `/*` inside strings).
#[must_use]
pub fn strip_block_comments_for_sig_parse(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut it = src.chars().peekable();
    while let Some(c) = it.next() {
        if c == '/' && it.peek() == Some(&'*') {
            it.next();
            let mut prev_star = false;
            for c2 in it.by_ref() {
                if prev_star && c2 == '/' {
                    break;
                }
                prev_star = c2 == '*';
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Path (repo layout): `data/signatures/core.sig.leek`
pub const CORE_SIG_LEEK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/signatures/core.sig.leek"
));

/// Path (repo layout): `data/signatures/leekwars.sig.leek`
pub const LEEKWARS_SIG_LEEK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/signatures/leekwars.sig.leek"
));

fn re_core_global_decl() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*global[ \t]+(?:integer|real)[ \t]+(\w+)\s*=")
            .expect("regex CORE_GLOBAL_DECL")
    })
}

fn re_core_global_int() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*global[ \t]+integer[ \t]+(\w+)\s*=\s*(-?[0-9]+)\s*;")
            .expect("regex CORE_GLOBAL_INT")
    })
}

fn re_core_fn_name() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^[ \t]*function[ \t]+(\w+)\s*\(").expect("regex CORE_FN_NAME")
    })
}

/// Line-based extraction for `core.sig.leek` (multi-line `function` headers are not valid in one parse unit yet).
#[must_use]
pub fn extract_core_sig_line_based(cleaned: &str) -> SigLeekUnit {
    let mut names: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push_name = |n: String| {
        if seen.insert(n.clone()) {
            names.push(n);
        }
    };

    for cap in re_core_global_decl().captures_iter(cleaned) {
        push_name(cap[1].to_string());
    }

    let mut integer_globals: Vec<(String, i64)> = Vec::new();
    for cap in re_core_global_int().captures_iter(cleaned) {
        let n = cap[1].to_string();
        let v: i64 = cap[2].parse().expect("integer literal");
        integer_globals.push((n, v));
    }

    for cap in re_core_fn_name().captures_iter(cleaned) {
        push_name(cap[1].to_string());
    }

    integer_globals.sort_by(|a, b| a.0.cmp(&b.0));
    SigLeekUnit {
        names,
        integer_globals,
    }
}

fn panic_sig_diags(path: &str, err: Vec<crate::CompileDiagnostic>) -> ! {
    panic!(
        "failed to parse signature `{}`: {:?}",
        path,
        err.iter()
            .map(|d| format!("{}: {}", d.reference, d.message))
            .collect::<Vec<_>>()
    )
}

fn parse_leekwars_embedded(path: &'static str, src: &'static str) -> SigLeekUnit {
    let opts = CompileOptions::default();
    let cleaned = strip_block_comments_for_sig_parse(src);
    parse_sig_leek(&cleaned, &opts).unwrap_or_else(|e| panic_sig_diags(path, e))
}

/// Merge order: **core** first (stdlib names + integer constants), then **leekwars** (append new names; integer map overwritten by leekwars on key clash).
#[must_use]
pub fn merge_sig_units(core: SigLeekUnit, leekwars: SigLeekUnit) -> SigLeekUnit {
    let mut seen: HashSet<String> = HashSet::new();
    let mut names: Vec<String> = Vec::new();
    for n in core.names.into_iter().chain(leekwars.names) {
        if seen.insert(n.clone()) {
            names.push(n);
        }
    }

    let mut ints: BTreeMap<String, i64> = BTreeMap::new();
    for (k, v) in core.integer_globals {
        ints.insert(k, v);
    }
    for (k, v) in leekwars.integer_globals {
        ints.insert(k, v);
    }
    let integer_globals: Vec<(String, i64)> = ints.into_iter().collect();

    SigLeekUnit {
        names,
        integer_globals,
    }
}

/// Parsed **`core.sig.leek` + `leekwars.sig.leek`**. Core uses line extraction; leekwars uses full HIR.
/// Panics if `leekwars.sig.leek` fails to parse.
#[must_use]
pub fn merged_workspace_sig_bundle() -> SigLeekUnit {
    let core = extract_core_sig_line_based(&strip_block_comments_for_sig_parse(CORE_SIG_LEEK));
    let lw = parse_leekwars_embedded("data/signatures/leekwars.sig.leek", LEEKWARS_SIG_LEEK);
    merge_sig_units(core, lw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{parse_sig_leek, CompileOptions};

    #[test]
    fn core_sig_line_extract_finds_stdlib() {
        let u = extract_core_sig_line_based(&strip_block_comments_for_sig_parse(CORE_SIG_LEEK));
        assert!(u.names.iter().any(|n| n == "abs"));
        assert!(u.integer_globals.iter().any(|(n, _)| n == "SORT_ASC"));
    }

    #[test]
    fn leekwars_sig_parses_after_comment_strip() {
        let cleaned = strip_block_comments_for_sig_parse(LEEKWARS_SIG_LEEK);
        let u = parse_sig_leek(&cleaned, &CompileOptions::default()).expect("leekwars.sig.leek");
        assert!(u.names.iter().any(|n| n == "WEAPON_PISTOL"));
        assert!(u
            .integer_globals
            .iter()
            .any(|(n, v)| n == "WEAPON_PISTOL" && *v == 37));
    }

    #[test]
    fn merged_includes_weapon_pistol_and_sort_asc() {
        let m = merged_workspace_sig_bundle();
        assert!(m.names.iter().any(|n| n == "WEAPON_PISTOL"));
        assert!(m.names.iter().any(|n| n == "SORT_ASC"));
        assert_eq!(
            m.integer_globals
                .iter()
                .find(|(n, _)| n == "WEAPON_PISTOL")
                .map(|(_, v)| *v),
            Some(37)
        );
        assert_eq!(
            m.integer_globals
                .iter()
                .find(|(n, _)| n == "SORT_ASC")
                .map(|(_, v)| *v),
            Some(0)
        );
    }
}
