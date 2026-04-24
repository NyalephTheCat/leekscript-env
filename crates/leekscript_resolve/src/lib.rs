//! Lexical name resolution over [`HirFile`](leekscript_hir::HirFile) (no types yet).

use leekscript_hir::{
    HirBinOp, HirClassMember, HirExpr, HirFieldVisibility, HirFile, HirStmt, HirSwitchClause, NameDef,
};
use leekscript_span::Span;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Static resolution issue (unknown name, duplicate binding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveDiagnostic {
    pub reference: &'static str,
    pub span: Span,
    pub message: String,
    pub source_file: PathBuf,
}

/// References emitted by this pass — keep in sync with [`crate::resolve_hir`].
pub const EMITTED_REFERENCES: &[&str] = &[
    "CLASS_STATIC_MEMBER_DOES_NOT_EXIST",
    "INCLUDE_ONLY_IN_MAIN_BLOCK",
    "VARIABLE_NAME_UNAVAILABLE",
    "VARIABLE_NOT_EXISTS",
];

/// Interpreter builtins pre-seeded in the global environment (`interpret_hir`).
/// Union of `data/signatures/core.sig.leek` and Leek Wars host natives (`getLife`, …).
/// `leekscript_run` re-exports this slice as `STDLIB_GLOBAL_IDENTIFIERS` — keep a single definition here.
pub const STDLIB_GLOBAL_IDENTIFIERS: &[&str] = &[
    "abs",
    "acos",
    "arrayChunk",
    "arrayClear",
    "arrayConcat",
    "arrayEvery",
    "arrayFilter",
    "arrayFind",
    "arrayFlatten",
    "arrayFoldLeft",
    "arrayFoldRight",
    "arrayFrequencies",
    "arrayGet",
    "arrayGetOrElse",
    "arrayIter",
    "arrayMap",
    "arrayMax",
    "arrayMin",
    "arrayPartition",
    "arrayRandom",
    "arrayRemove",
    "arrayRemoveAll",
    "arraySlice",
    "arraySome",
    "arraySort",
    "arrayToSet",
    "arrayUnique",
    "asin",
    "assocReverse",
    "assocSort",
    "atan",
    "atan2",
    "attack",
    "average",
    "binString",
    "bitCount",
    "bitReverse",
    "bitsToReal",
    "byteReverse",
    "cbrt",
    "ceil",
    "charAt",
    "clone",
    "color",
    "codePointAt",
    "contains",
    "cos",
    "count",
    "debug",
    "debugC",
    "debugE",
    "debugW",
    "endsWith",
    "exp",
    "fill",
    "floor",
    "getBlue",
    "getColor",
    "getGreen",
    "getLeek",
    "getLife",
    "getOperations",
    "getRed",
    "hexString",
    "hypot",
    "inArray",
    "indexOf",
    "insert",
    "intervalAverage",
    "intervalCombine",
    "intervalContains",
    "intervalIntersection",
    "intervalIsBounded",
    "intervalIsClosed",
    "intervalIsEmpty",
    "intervalIsLeftBounded",
    "intervalIsLeftClosed",
    "intervalIsRightBounded",
    "intervalIsRightClosed",
    "intervalMax",
    "intervalMin",
    "intervalSize",
    "intervalToArray",
    "intervalToSet",
    "intervalValues",
    "isEmpty",
    "isFinite",
    "isInfinite",
    "isNaN",
    "isPermutation",
    "join",
    "keySort",
    "jsonDecode",
    "jsonEncode",
    "leadingZeros",
    "length",
    "log",
    "log10",
    "log2",
    "mapAverage",
    "mapClear",
    "mapContains",
    "mapContainsKey",
    "mapEvery",
    "mapFill",
    "mapFilter",
    "mapFold",
    "mapGet",
    "mapIsEmpty",
    "mapIter",
    "mapKeys",
    "mapMap",
    "mapMax",
    "mapMerge",
    "mapMin",
    "mapPut",
    "mapRemove",
    "mapRemoveAll",
    "mapReplace",
    "mapReplaceAll",
    "mapSearch",
    "mapSize",
    "mapSome",
    "mapSum",
    "mapValues",
    "max",
    "min",
    "number",
    "Object",
    "pop",
    "pow",
    "push",
    "pushAll",
    "rand",
    "randFloat",
    "randInt",
    "randReal",
    "realBits",
    "remove",
    "removeElement",
    "removeKey",
    "replace",
    "resize",
    "reverse",
    "search",
    "rotateLeft",
    "rotateRight",
    "round",
    "setClear",
    "setContains",
    "setDifference",
    "setDisjunction",
    "setInsert",
    "setIntersection",
    "setIsEmpty",
    "setIsSubsetOf",
    "setPut",
    "setRemove",
    "setSize",
    "setToArray",
    "setUnion",
    "shift",
    "shuffle",
    "signum",
    "sin",
    "sort",
    "split",
    "sqrt",
    "startsWith",
    "string",
    "subArray",
    "substring",
    "subString",
    "sum",
    "tan",
    "toDegrees",
    "toLower",
    "toRadians",
    "toUpper",
    "trailingZeros",
    "typeOf",
    "unknown",
    "unshift",
];

/// Resolve names in a single file (block scope + function parameters + top-level declarations).
///
/// `main_source` is the path of the root compile unit (used for top-level statements when
/// `hir.stmt_sources` is empty or after `include` expansion for attribution).
///
/// [`resolve_hir_with_extra_globals`] adds names from a signature file (Leek Wars natives, etc.).
pub fn resolve_hir(hir: &HirFile, main_source: &Path) -> Vec<ResolveDiagnostic> {
    resolve_hir_with_extra_globals(hir, main_source, &[], 3)
}

/// Like [`resolve_hir`], but pre-declare additional global names (from `*.toml` signature bundles).
pub fn resolve_hir_with_extra_globals(
    hir: &HirFile,
    main_source: &Path,
    extra_globals: &[String],
    language_version: u8,
) -> Vec<ResolveDiagnostic> {
    let class_layout_fields = build_class_merged_instance_fields(hir);
    let class_static_layout_fields = build_class_merged_static_fields(hir);
    let class_static_member_names = build_class_merged_static_member_names(hir);
    let class_instance_method_names = build_class_merged_instance_method_names(hir);
    let class_instance_method_arities = build_class_merged_instance_method_arities(hir);
    let class_field_access = build_class_merged_field_access(hir);
    let mut globals = HashMap::new();
    for name in STDLIB_GLOBAL_IDENTIFIERS {
        globals.insert((*name).to_string(), Span::point(0));
    }
    globals.insert("Infinity".into(), Span::point(0));
    globals.insert("PI".into(), Span::point(0));
    globals.insert("E".into(), Span::point(0));
    globals.insert("NaN".into(), Span::point(0));
    for name in extra_globals {
        globals.entry(name.clone()).or_insert(Span::point(0));
    }
    globals.insert("Integer".into(), Span::point(0));
    globals.insert("Real".into(), Span::point(0));
    globals.insert("Number".into(), Span::point(0));
    if language_version >= 3 {
        globals.insert("Array".into(), Span::point(0));
    }
    globals.insert("SORT_ASC".into(), Span::point(0));
    globals.insert("SORT_DESC".into(), Span::point(0));
    // v4+: `Map` is a global type name (`new Map`, `instanceof Map`); user `class Map` conflicts (Java VM).
    if language_version >= 4 {
        globals.insert("Map".into(), Span::point(0));
    }
    // v3+: `Value` is a global type name in the Java suite.
    if language_version >= 3 {
        globals.insert("Value".into(), Span::point(0));
        globals.insert("Null".into(), Span::point(0));
        globals.insert("String".into(), Span::point(0));
        globals.insert("Boolean".into(), Span::point(0));
        globals.insert("Object".into(), Span::point(0));
        globals.insert("Function".into(), Span::point(0));
        globals.insert("Class".into(), Span::point(0));
        globals.insert("Interval".into(), Span::point(0));
        globals.insert("JSON".into(), Span::point(0));
        globals.insert("System".into(), Span::point(0));
    }
    // Inner scope so file-level `var` / `function` can shadow stdlib globals (`min`, …) like Java.
    let mut file_fn_decl_names: HashSet<String> = HashSet::new();
    let mut file_var_decl_names: HashSet<String> = HashSet::new();
    for s in &hir.stmts {
        match s {
            HirStmt::FnDecl { name, .. } => {
                file_fn_decl_names.insert(name.name.clone());
            }
            HirStmt::Var { name, .. } => {
                file_var_decl_names.insert(name.name.clone());
            }
            _ => {}
        }
    }
    let mut r = Resolver {
        scopes: vec![globals, HashMap::new()],
        diags: Vec::new(),
        class_layout_fields,
        class_static_layout_fields,
        class_static_member_names,
        class_instance_method_names,
        class_instance_method_arities,
        class_field_access,
        current_class_name: None,
        current_class_static_method_arities: None,
        current_class_instance_method_arities: None,
        current_in_static_method: false,
        language_version,
        file_fn_decl_names,
        file_var_decl_names,
        seen_global_decl_names: HashSet::new(),
        seen_stub_fn_decl_names: HashSet::new(),
    };
    let use_sources = hir.stmt_sources.len() == hir.stmts.len();
    // Hoist `var` and `function` names in the file scope so bodies can reference later declarations
    // (`function f(){ g = 1 } function g() {}` in Java suite).
    for (i, s) in hir.stmts.iter().enumerate() {
        let source_file = if use_sources {
            hir.stmt_sources[i].as_path()
        } else {
            main_source
        };
        match s {
            HirStmt::Var { name, .. } => {
                let cur = r.scopes.last_mut().expect("scope stack");
                if cur.contains_key(&name.name) {
                    r.diags.push(ResolveDiagnostic {
                        reference: "VARIABLE_NAME_UNAVAILABLE",
                        span: name.span,
                        message: format!("variable `{}` is already defined in this scope", name.name),
                        source_file: source_file.to_path_buf(),
                    });
                }
                cur.entry(name.name.clone()).or_insert(name.span);
            }
            HirStmt::FnDecl { name, body, .. } => {
                // Only hoist non-empty function bodies (stubs are merged by `walk_stmt`).
                if body.is_empty() {
                    continue;
                }
                let cur = r.scopes.last_mut().expect("scope stack");
                if cur.contains_key(&name.name) {
                    r.diags.push(ResolveDiagnostic {
                        reference: "VARIABLE_NAME_UNAVAILABLE",
                        span: name.span,
                        message: format!("function `{}` is already defined in this scope", name.name),
                        source_file: source_file.to_path_buf(),
                    });
                }
                cur.entry(name.name.clone()).or_insert(name.span);
            }
            HirStmt::Global { entries, .. } => {
                for (name, _init) in entries {
                    let root = r.scopes.first_mut().expect("scope stack");
                    root.entry(name.name.clone()).or_insert(name.span);
                }
            }
            _ => {}
        }
        // Preserve existing diagnostics behavior for duplicate `global` by still letting `walk_stmt`
        // run normally below.
        let _ = source_file;
    }
    for (i, s) in hir.stmts.iter().enumerate() {
        let source_file = if use_sources {
            hir.stmt_sources[i].as_path()
        } else {
            main_source
        };
        r.walk_stmt(s, true, source_file);
    }
    r.diags
}

/// Per-class merged instance field names (same order as the interpreter’s layout merge).
fn build_class_merged_instance_fields(hir: &HirFile) -> HashMap<String, Vec<String>> {
    type Raw = (Option<String>, Vec<String>);
    let mut raw: HashMap<String, Raw> = HashMap::new();
    for s in &hir.stmts {
        let HirStmt::ClassDecl {
            name,
            extends,
            members,
        } = s
        else {
            continue;
        };
        let mut own = Vec::new();
        for m in members {
            if let HirClassMember::Field {
                name: fnm,
                is_static: false,
                ..
            } = m
            {
                own.push(fnm.name.clone());
            }
        }
        raw.insert(
            name.name.clone(),
            (
                extends.as_ref().map(|e| e.name.clone()),
                own,
            ),
        );
    }
    let mut memo: HashMap<String, Vec<String>> = HashMap::new();
    fn merged(
        raw: &HashMap<String, Raw>,
        cn: &str,
        memo: &mut HashMap<String, Vec<String>>,
    ) -> Vec<String> {
        if let Some(c) = memo.get(cn) {
            return c.clone();
        }
        let Some((ext, own)) = raw.get(cn) else {
            memo.insert(cn.to_string(), vec![]);
            return vec![];
        };
        let mut out = Vec::new();
        if let Some(p) = ext {
            let parent_list = merged(raw, p, memo);
            let own_set: HashSet<_> = own.iter().cloned().collect();
            for f in parent_list {
                if !own_set.contains(&f) {
                    out.push(f);
                }
            }
        }
        for f in own {
            out.push(f.clone());
        }
        memo.insert(cn.to_string(), out.clone());
        out
    }
    let keys: Vec<String> = raw.keys().cloned().collect();
    let mut out = HashMap::new();
    for k in keys {
        let v = merged(&raw, &k, &mut memo);
        out.insert(k, v);
    }
    out
}

/// Per-class merged **static** field names (inheritance like instance layout).
fn build_class_merged_static_fields(hir: &HirFile) -> HashMap<String, Vec<String>> {
    type Raw = (Option<String>, Vec<String>);
    let mut raw: HashMap<String, Raw> = HashMap::new();
    for s in &hir.stmts {
        let HirStmt::ClassDecl {
            name,
            extends,
            members,
        } = s
        else {
            continue;
        };
        let mut own = Vec::new();
        for m in members {
            if let HirClassMember::Field {
                name: fnm,
                is_static: true,
                ..
            } = m
            {
                own.push(fnm.name.clone());
            }
        }
        raw.insert(
            name.name.clone(),
            (
                extends.as_ref().map(|e| e.name.clone()),
                own,
            ),
        );
    }
    let mut memo: HashMap<String, Vec<String>> = HashMap::new();
    fn merged(
        raw: &HashMap<String, Raw>,
        cn: &str,
        memo: &mut HashMap<String, Vec<String>>,
    ) -> Vec<String> {
        if let Some(c) = memo.get(cn) {
            return c.clone();
        }
        let Some((ext, own)) = raw.get(cn) else {
            memo.insert(cn.to_string(), vec![]);
            return vec![];
        };
        let mut out = Vec::new();
        if let Some(p) = ext {
            let parent_list = merged(raw, p, memo);
            let own_set: HashSet<_> = own.iter().cloned().collect();
            for f in parent_list {
                if !own_set.contains(&f) {
                    out.push(f);
                }
            }
        }
        for f in own {
            out.push(f.clone());
        }
        memo.insert(cn.to_string(), out.clone());
        out
    }
    let keys: Vec<String> = raw.keys().cloned().collect();
    let mut out = HashMap::new();
    for k in keys {
        let v = merged(&raw, &k, &mut memo);
        out.insert(k, v);
    }
    out
}

/// Static **field** and **static method** names per class (inheritance like instance layout).
fn build_class_merged_static_member_names(hir: &HirFile) -> HashMap<String, HashSet<String>> {
    type Raw = (Option<String>, Vec<String>);
    let mut raw: HashMap<String, Raw> = HashMap::new();
    for s in &hir.stmts {
        let HirStmt::ClassDecl {
            name,
            extends,
            members,
        } = s
        else {
            continue;
        };
        let mut own = Vec::new();
        for m in members {
            match m {
                HirClassMember::Field {
                    name: fnm,
                    is_static: true,
                    ..
                } => own.push(fnm.name.clone()),
                HirClassMember::Method {
                    name: mn,
                    is_static: true,
                    ..
                } => own.push(mn.name.clone()),
                _ => {}
            }
        }
        raw.insert(
            name.name.clone(),
            (
                extends.as_ref().map(|e| e.name.clone()),
                own,
            ),
        );
    }
    let mut memo: HashMap<String, HashSet<String>> = HashMap::new();
    fn merged(
        raw: &HashMap<String, Raw>,
        cn: &str,
        memo: &mut HashMap<String, HashSet<String>>,
    ) -> HashSet<String> {
        if let Some(c) = memo.get(cn) {
            return c.clone();
        }
        let Some((ext, own)) = raw.get(cn) else {
            memo.insert(cn.to_string(), HashSet::new());
            return HashSet::new();
        };
        let mut out = HashSet::new();
        if let Some(p) = ext {
            let parent = merged(raw, p, memo);
            let own_set: HashSet<_> = own.iter().cloned().collect();
            for x in parent {
                if !own_set.contains(&x) {
                    out.insert(x);
                }
            }
        }
        for x in own {
            out.insert(x.clone());
        }
        memo.insert(cn.to_string(), out.clone());
        out
    }
    let keys: Vec<String> = raw.keys().cloned().collect();
    let mut out = HashMap::new();
    for k in keys {
        let v = merged(&raw, &k, &mut memo);
        out.insert(k, v);
    }
    out
}

fn build_class_merged_instance_method_names(hir: &HirFile) -> HashMap<String, HashSet<String>> {
    type Raw = (Option<String>, Vec<String>);
    let mut raw: HashMap<String, Raw> = HashMap::new();
    for s in &hir.stmts {
        let HirStmt::ClassDecl {
            name,
            extends,
            members,
        } = s
        else {
            continue;
        };
        let mut own = Vec::new();
        for m in members {
            if let HirClassMember::Method {
                name: mn,
                is_static: false,
                ..
            } = m
            {
                own.push(mn.name.clone());
            }
        }
        raw.insert(
            name.name.clone(),
            (extends.as_ref().map(|e| e.name.clone()), own),
        );
    }
    let mut memo: HashMap<String, HashSet<String>> = HashMap::new();
    fn merged(
        raw: &HashMap<String, Raw>,
        cn: &str,
        memo: &mut HashMap<String, HashSet<String>>,
    ) -> HashSet<String> {
        if let Some(c) = memo.get(cn) {
            return c.clone();
        }
        let Some((ext, own)) = raw.get(cn) else {
            memo.insert(cn.to_string(), HashSet::new());
            return HashSet::new();
        };
        let mut out = HashSet::new();
        if let Some(p) = ext {
            let parent = merged(raw, p, memo);
            let own_set: HashSet<_> = own.iter().cloned().collect();
            for x in parent {
                if !own_set.contains(&x) {
                    out.insert(x);
                }
            }
        }
        for x in own {
            out.insert(x.clone());
        }
        memo.insert(cn.to_string(), out.clone());
        out
    }
    let keys: Vec<String> = raw.keys().cloned().collect();
    let mut out = HashMap::new();
    for k in keys {
        let v = merged(&raw, &k, &mut memo);
        out.insert(k, v);
    }
    out
}

fn build_class_merged_instance_method_arities(
    hir: &HirFile,
) -> HashMap<String, HashMap<String, HashSet<usize>>> {
    #[derive(Clone)]
    struct Raw {
        extends: Option<String>,
        own: HashMap<String, HashSet<usize>>,
    }
    let mut raw: HashMap<String, Raw> = HashMap::new();
    for s in &hir.stmts {
        let HirStmt::ClassDecl {
            name,
            extends,
            members,
        } = s
        else {
            continue;
        };
        let mut own: HashMap<String, HashSet<usize>> = HashMap::new();
        for m in members {
            if let HirClassMember::Method {
                name: mn,
                is_static: false,
                params,
                ..
            } = m
            {
                own.entry(mn.name.clone()).or_default().insert(params.len());
            }
        }
        raw.insert(
            name.name.clone(),
            Raw {
                extends: extends.as_ref().map(|e| e.name.clone()),
                own,
            },
        );
    }
    let mut memo: HashMap<String, HashMap<String, HashSet<usize>>> = HashMap::new();
    fn merged(
        raw: &HashMap<String, Raw>,
        cn: &str,
        memo: &mut HashMap<String, HashMap<String, HashSet<usize>>>,
    ) -> HashMap<String, HashSet<usize>> {
        if let Some(c) = memo.get(cn) {
            return c.clone();
        }
        let Some(r) = raw.get(cn) else {
            memo.insert(cn.to_string(), HashMap::new());
            return HashMap::new();
        };
        let mut out = HashMap::<String, HashSet<usize>>::new();
        if let Some(p) = &r.extends {
            let parent = merged(raw, p, memo);
            for (k, v) in parent {
                out.entry(k).or_default().extend(v);
            }
        }
        for (k, v) in &r.own {
            out.entry(k.clone()).or_default().extend(v.iter().copied());
        }
        memo.insert(cn.to_string(), out.clone());
        out
    }
    let keys: Vec<String> = raw.keys().cloned().collect();
    let mut out = HashMap::new();
    for k in keys {
        out.insert(k.clone(), merged(&raw, &k, &mut memo));
    }
    out
}

type FieldAccessInfo = (String, HirFieldVisibility);

fn build_class_merged_field_access(hir: &HirFile) -> HashMap<String, HashMap<String, FieldAccessInfo>> {
    #[derive(Clone)]
    struct Raw {
        extends: Option<String>,
        own: Vec<(String, String, HirFieldVisibility)>,
    }
    let mut raw: HashMap<String, Raw> = HashMap::new();
    for s in &hir.stmts {
        let HirStmt::ClassDecl {
            name,
            extends,
            members,
        } = s
        else {
            continue;
        };
        let mut own = Vec::new();
        for m in members {
            if let HirClassMember::Field {
                name: fnm,
                is_static: false,
                visibility,
                ..
            } = m
            {
                own.push((
                    fnm.name.clone(),
                    name.name.clone(),
                    *visibility,
                ));
            }
        }
        raw.insert(
            name.name.clone(),
            Raw {
                extends: extends.as_ref().map(|e| e.name.clone()),
                own,
            },
        );
    }
    fn merged(
        raw: &HashMap<String, Raw>,
        cn: &str,
        memo: &mut HashMap<String, HashMap<String, FieldAccessInfo>>,
    ) -> HashMap<String, FieldAccessInfo> {
        if let Some(c) = memo.get(cn) {
            return c.clone();
        }
        let Some(r) = raw.get(cn) else {
            memo.insert(cn.to_string(), HashMap::new());
            return HashMap::new();
        };
        let mut out: HashMap<String, FieldAccessInfo> = HashMap::new();
        if let Some(p) = &r.extends {
            let parent_map = merged(raw, p, memo);
            let own_names: HashSet<_> = r.own.iter().map(|(f, _, _)| f.clone()).collect();
            for (f, info) in parent_map {
                if !own_names.contains(&f) {
                    out.insert(f, info);
                }
            }
        }
        for (f, decl, vis) in &r.own {
            out.insert(f.clone(), (decl.clone(), *vis));
        }
        memo.insert(cn.to_string(), out.clone());
        out
    }
    let keys: Vec<String> = raw.keys().cloned().collect();
    let mut out = HashMap::new();
    let mut memo = HashMap::new();
    for k in keys {
        let v = merged(&raw, &k, &mut memo);
        out.insert(k, v);
    }
    out
}

struct Resolver {
    scopes: Vec<HashMap<String, Span>>,
    diags: Vec<ResolveDiagnostic>,
    class_layout_fields: HashMap<String, Vec<String>>,
    class_static_layout_fields: HashMap<String, Vec<String>>,
    class_static_member_names: HashMap<String, HashSet<String>>,
    class_instance_method_names: HashMap<String, HashSet<String>>,
    class_instance_method_arities: HashMap<String, HashMap<String, HashSet<usize>>>,
    class_field_access: HashMap<String, HashMap<String, FieldAccessInfo>>,
    current_class_name: Option<String>,
    current_class_static_method_arities: Option<HashMap<String, HashSet<usize>>>,
    current_class_instance_method_arities: Option<HashMap<String, HashSet<usize>>>,
    current_in_static_method: bool,
    language_version: u8,
    file_fn_decl_names: HashSet<String>,
    file_var_decl_names: HashSet<String>,
    seen_global_decl_names: HashSet<String>,
    seen_stub_fn_decl_names: HashSet<String>,
}

impl Resolver {
    fn name_resolves_to_stdlib_global(&self, name: &str) -> bool {
        for (si, m) in self.scopes.iter().enumerate().rev() {
            if let Some(sp) = m.get(name) {
                // Root scope contains both stdlib seeds (Span::point(0)) and user `global` declarations.
                // Only stdlib seeds participate in "cannot redefine function".
                return si == 0 && *sp == Span::point(0);
            }
        }
        false
    }
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn try_define(&mut self, def: &NameDef, kind: &str, source_file: &Path) {
        let cur = self.scopes.last_mut().expect("scope stack");
        if cur.insert(def.name.clone(), def.span).is_some() {
            self.diags.push(ResolveDiagnostic {
                reference: "VARIABLE_NAME_UNAVAILABLE",
                span: def.span,
                message: format!("duplicate {kind} `{}`", def.name),
                source_file: source_file.to_path_buf(),
            });
        }
    }

    /// Define a name that is not allowed to shadow any existing binding in an enclosing scope.
    fn try_define_no_shadow(&mut self, def: &NameDef, kind: &str, source_file: &Path) {
        let shadows_nonstdlib = self.scopes.iter().enumerate().rev().any(|(si, m)| {
            let Some(sp) = m.get(def.name.as_str()) else {
                return false;
            };
            // Allow shadowing stdlib seeds (root scope, Span::point(0)) in inner scopes.
            if si == 0 && *sp == Span::point(0) {
                // Allow shadowing stdlib seeds by default, except for v4 reserved names like `Map`.
                self.language_version >= 4 && def.name == "Map"
            } else {
                true
            }
        });
        if shadows_nonstdlib {
            self.diags.push(ResolveDiagnostic {
                reference: "VARIABLE_NAME_UNAVAILABLE",
                span: def.span,
                message: format!("{kind} `{}` shadows an existing binding", def.name),
                source_file: source_file.to_path_buf(),
            });
            return;
        }
        self.try_define(def, kind, source_file);
    }

    fn resolve_ident(&mut self, name: &str, span: Span, source_file: &Path) {
        // `super(...)` in constructors — same as Java `WordCompiler` (not a normal binding).
        if name == "super" {
            return;
        }
        for m in self.scopes.iter().rev() {
            if m.contains_key(name) {
                return;
            }
        }
        self.diags.push(ResolveDiagnostic {
            reference: "VARIABLE_NOT_EXISTS",
            span,
            message: format!("undefined variable `{name}`"),
            source_file: source_file.to_path_buf(),
        });
    }

    fn walk_stmt(&mut self, s: &HirStmt, include_allowed: bool, source_file: &Path) {
        match s {
            HirStmt::Var { name, init, decl_ty: _ } => {
                // May have been hoisted for this scope already.
                if !self.scopes.last().is_some_and(|m| m.contains_key(&name.name)) {
                    self.try_define(name, "variable", source_file);
                }
                if let Some(init) = init {
                    self.walk_expr(init, source_file);
                }
            }
            HirStmt::Global { entries, .. } => {
                for (name, init) in entries {
                    let root = self.scopes.first_mut().expect("scope stack");
                    if root
                        .get(&name.name)
                        .is_some_and(|existing| *existing != name.span)
                    {
                        self.diags.push(ResolveDiagnostic {
                            reference: "VARIABLE_NAME_UNAVAILABLE",
                            span: name.span,
                            message: format!("global `{}` conflicts with an existing binding", name.name),
                            source_file: source_file.to_path_buf(),
                        });
                    }
                    if !self.seen_global_decl_names.insert(name.name.clone()) {
                        self.diags.push(ResolveDiagnostic {
                            reference: "VARIABLE_NAME_UNAVAILABLE",
                            span: name.span,
                            message: format!("duplicate global `{}`", name.name),
                            source_file: source_file.to_path_buf(),
                        });
                    }
                    root.entry(name.name.clone()).or_insert(name.span);
                    if let Some(e) = init {
                        self.walk_expr(e, source_file);
                    }
                }
            }
            HirStmt::Include { span, .. } => {
                if !include_allowed {
                    self.diags.push(ResolveDiagnostic {
                        reference: "INCLUDE_ONLY_IN_MAIN_BLOCK",
                        span: *span,
                        message: "`include` is only allowed at the top level of a file".into(),
                        source_file: source_file.to_path_buf(),
                    });
                }
            }
            HirStmt::Expr(e) => self.walk_expr(e, source_file),
            HirStmt::Return { value, .. } => {
                if let Some(x) = value {
                    self.walk_expr(x, source_file);
                }
            }
            HirStmt::Block(stmts) => {
                self.push();
                // Hoist within the block scope.
                {
                    let cur = self.scopes.last_mut().expect("scope stack");
                    for x in stmts {
                        match x {
                            HirStmt::Var { name, .. } => {
                                if cur.contains_key(&name.name) {
                                    self.diags.push(ResolveDiagnostic {
                                        reference: "VARIABLE_NAME_UNAVAILABLE",
                                        span: name.span,
                                        message: format!("variable `{}` is already defined in this block", name.name),
                                        source_file: source_file.to_path_buf(),
                                    });
                                }
                                cur.entry(name.name.clone()).or_insert(name.span);
                            }
                            HirStmt::FnDecl { name, body, .. } => {
                                // Only hoist non-empty function bodies for recursion; empty stubs are merged.
                                if body.is_empty() {
                                    continue;
                                }
                                if cur.contains_key(&name.name) {
                                    self.diags.push(ResolveDiagnostic {
                                        reference: "VARIABLE_NAME_UNAVAILABLE",
                                        span: name.span,
                                        message: format!("function `{}` is already defined in this block", name.name),
                                        source_file: source_file.to_path_buf(),
                                    });
                                }
                                cur.entry(name.name.clone()).or_insert(name.span);
                            }
                            _ => {}
                        }
                    }
                }
                for x in stmts {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
            }
            HirStmt::FnDecl {
                name,
                params,
                return_ty: _,
                body,
            } => {
                // Empty body: API / `.sig.leek` stub or overload declaration — merge with an existing
                // binding (including stdlib seeds) instead of treating as a duplicate.
                if body.is_empty() {
                    self.seen_stub_fn_decl_names.insert(name.name.clone());
                    let cur = self.scopes.last_mut().expect("scope stack");
                    cur.entry(name.name.clone()).or_insert(name.span);
                } else {
                    if self.seen_stub_fn_decl_names.contains(&name.name) {
                        self.diags.push(ResolveDiagnostic {
                            reference: "VARIABLE_NAME_UNAVAILABLE",
                            span: name.span,
                            message: format!(
                                "function `{}` has both a stub and a body in the same file",
                                name.name
                            ),
                            source_file: source_file.to_path_buf(),
                        });
                    }
                    // May have been hoisted for this scope already.
                    if !self.scopes.last().is_some_and(|m| m.contains_key(&name.name)) {
                        self.try_define(name, "function", source_file);
                    }
                }
                self.push();
                for p in params {
                    self.try_define(&p.name, "parameter", source_file);
                    if let Some(e) = &p.default {
                        self.walk_expr(e, source_file);
                    }
                }
                for x in body {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.walk_expr(cond, source_file);
                self.push();
                for x in then_body {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
                if let Some(els) = else_body {
                    self.push();
                    for x in els {
                        self.walk_stmt(x, false, source_file);
                    }
                    self.pop();
                }
            }
            HirStmt::While { cond, body } => {
                self.walk_expr(cond, source_file);
                self.push();
                for x in body {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
            }
            HirStmt::DoWhile { body, cond } => {
                self.walk_expr(cond, source_file);
                self.push();
                for x in body {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
            }
            HirStmt::Switch { discr, clauses } => {
                self.walk_expr(discr, source_file);
                for cl in clauses {
                    match cl {
                        HirSwitchClause::Case { labels, body } => {
                            for e in labels {
                                self.walk_expr(e, source_file);
                            }
                            self.push();
                            for x in body {
                                self.walk_stmt(x, false, source_file);
                            }
                            self.pop();
                        }
                        HirSwitchClause::Default { body } => {
                            self.push();
                            for x in body {
                                self.walk_stmt(x, false, source_file);
                            }
                            self.pop();
                        }
                    }
                }
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                self.push();
                if let Some(s) = init {
                    // Java suite: `for (var x = ...)` cannot reuse an existing binding name.
                    match s.as_ref() {
                        HirStmt::Var { name, init, .. } => {
                            if let Some(init) = init {
                                self.walk_expr(init, source_file);
                            }
                            self.try_define_no_shadow(name, "for variable", source_file);
                        }
                        other => self.walk_stmt(other, false, source_file),
                    }
                }
                if let Some(c) = cond {
                    self.walk_expr(c, source_file);
                }
                if let Some(upd) = update {
                    match upd {
                        leekscript_hir::HirForStep::Assign(u) => {
                            self.walk_expr(&u.value, source_file);
                            self.resolve_ident(&u.name.name, u.name.span, source_file);
                        }
                        leekscript_hir::HirForStep::Expr(e) => {
                            self.walk_expr(e, source_file);
                        }
                    }
                }
                for x in body {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
            }
            HirStmt::ForIn {
                name,
                is_declaration,
                container,
                body,
                ..
            } => {
                self.walk_expr(container, source_file);
                self.push();
                if *is_declaration {
                    // Java suite: for-in bindings cannot reuse an existing name (even in nested scopes).
                    self.try_define_no_shadow(name, "for-in variable", source_file);
                } else {
                    self.resolve_ident(&name.name, name.span, source_file);
                }
                for x in body {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
            }
            HirStmt::ForInKeyValue {
                key,
                key_is_declaration,
                value,
                value_is_declaration,
                container,
                body,
                ..
            } => {
                self.walk_expr(container, source_file);
                self.push();
                if *key_is_declaration {
                    self.try_define_no_shadow(key, "for-in key variable", source_file);
                } else {
                    self.resolve_ident(&key.name, key.span, source_file);
                }
                if *value_is_declaration {
                    self.try_define_no_shadow(value, "for-in value variable", source_file);
                } else {
                    self.resolve_ident(&value.name, value.span, source_file);
                }
                for x in body {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
            }
            HirStmt::Assign { place, value, .. } => {
                self.walk_expr(place, source_file);
                self.walk_expr(value, source_file);
            }
            HirStmt::Try {
                try_body,
                catch,
                finally_body,
            } => {
                self.push();
                for x in try_body {
                    self.walk_stmt(x, false, source_file);
                }
                self.pop();
                if let Some((param, catch_body)) = catch {
                    self.push();
                    self.try_define(param, "catch parameter", source_file);
                    for x in catch_body {
                        self.walk_stmt(x, false, source_file);
                    }
                    self.pop();
                }
                if let Some(fb) = finally_body {
                    self.push();
                    for x in fb {
                        self.walk_stmt(x, false, source_file);
                    }
                    self.pop();
                }
            }
            HirStmt::Throw(e) => {
                if let Some(x) = e {
                    self.walk_expr(x, source_file);
                }
            }
            HirStmt::ClassDecl {
                name,
                extends,
                members,
            } => {
                // v4: class names must not shadow reserved global type names like `Map`.
                if self.language_version >= 4 && name.name == "Map" {
                    self.try_define_no_shadow(name, "class", source_file);
                } else {
                    self.try_define(name, "class", source_file);
                }
                let prev_class = self.current_class_name.replace(name.name.clone());
                let prev_static_arities = self.current_class_static_method_arities.take();
                let prev_instance_arities = self.current_class_instance_method_arities.take();
                let mut static_arities: HashMap<String, HashSet<usize>> = HashMap::new();
                for m in members {
                    if let HirClassMember::Method {
                        name: mn,
                        is_static: true,
                        params,
                        ..
                    } = m
                    {
                        static_arities
                            .entry(mn.name.clone())
                            .or_default()
                            .insert(params.len());
                    }
                }
                self.current_class_static_method_arities = Some(static_arities);
                self.current_class_instance_method_arities =
                    self.class_instance_method_arities.get(&name.name).cloned();
                self.push();
                let own_fields: HashSet<String> = members
                    .iter()
                    .filter_map(|m| {
                        if let HirClassMember::Field {
                            name: fnm,
                            is_static: false,
                            ..
                        } = m
                        {
                            Some(fnm.name.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                let own_static_fields: HashSet<String> = members
                    .iter()
                    .filter_map(|m| {
                        if let HirClassMember::Field {
                            name: fnm,
                            is_static: true,
                            ..
                        } = m
                        {
                            Some(fnm.name.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                // Java reserves some pseudo static members on class values.
                for reserved in ["name", "fields", "staticFields", "methods", "staticMethods", "class", "super"] {
                    if own_static_fields.contains(reserved) {
                        self.diags.push(ResolveDiagnostic {
                            reference: "VARIABLE_NAME_UNAVAILABLE",
                            span: name.span,
                            message: format!("static field name `{reserved}` is reserved"),
                            source_file: source_file.to_path_buf(),
                        });
                    }
                }
                let own_static_methods: HashSet<String> = members
                    .iter()
                    .filter_map(|m| {
                        if let HirClassMember::Method {
                            name: mn,
                            is_static: true,
                            ..
                        } = m
                        {
                            Some(mn.name.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                for reserved in ["name", "fields", "staticFields", "methods", "staticMethods", "class", "super"] {
                    if own_static_methods.contains(reserved) {
                        self.diags.push(ResolveDiagnostic {
                            reference: "VARIABLE_NAME_UNAVAILABLE",
                            span: name.span,
                            message: format!("static method name `{reserved}` is reserved"),
                            source_file: source_file.to_path_buf(),
                        });
                    }
                }
                if let Some(layout) = self.class_layout_fields.get(&name.name) {
                    let cur = self.scopes.last_mut().expect("scope stack");
                    for f in layout {
                        if !own_fields.contains(f) {
                            cur.entry(f.clone()).or_insert(Span::point(0));
                        }
                    }
                }
                if let Some(layout) = self.class_static_layout_fields.get(&name.name) {
                    let cur = self.scopes.last_mut().expect("scope stack");
                    for f in layout {
                        if !own_static_fields.contains(f) {
                            cur.entry(f.clone()).or_insert(Span::point(0));
                        }
                    }
                }
                let mut method_arities: HashMap<String, HashSet<usize>> = HashMap::new();
                for m in members {
                    if let HirClassMember::Method { name: mn, params, .. } = m {
                        let set = method_arities.entry(mn.name.clone()).or_default();
                        if !set.insert(params.len()) {
                            self.diags.push(ResolveDiagnostic {
                                reference: "VARIABLE_NAME_UNAVAILABLE",
                                span: mn.span,
                                message: format!(
                                    "duplicate method `{}` with the same arity",
                                    mn.name
                                ),
                                source_file: source_file.to_path_buf(),
                            });
                        }
                    }
                }
                // Java VM: a class with **no** `extends` cannot declare arity overloads when every
                // overload body is empty (`m() {} m(x) {}`). Subclasses may (`extends B { b() {} b(x) {} }`).
                if extends.is_none() {
                    let mut by_static_name: HashMap<(bool, String), Vec<(usize, bool, Span)>> =
                        HashMap::new();
                    for m in members {
                        if let HirClassMember::Method {
                            name: mn,
                            is_static,
                            params,
                            body,
                            ..
                        } = m
                        {
                            by_static_name
                                .entry((*is_static, mn.name.clone()))
                                .or_default()
                                .push((params.len(), body.is_empty(), mn.span));
                        }
                    }
                    for ((_is_static, mname), variants) in by_static_name {
                        let arity_count = variants.iter().map(|(a, _, _)| *a).collect::<HashSet<_>>();
                        if arity_count.len() > 1
                            && variants.iter().all(|(_, empty, _)| *empty)
                        {
                            let span = variants
                                .iter()
                                .map(|(_, _, sp)| *sp)
                                .min_by_key(|s| (s.start, s.end))
                                .unwrap_or(name.span);
                            self.diags.push(ResolveDiagnostic {
                                reference: "VARIABLE_NAME_UNAVAILABLE",
                                span,
                                message: format!(
                                    "invalid overloads of `{}`: empty bodies require a superclass",
                                    mname
                                ),
                                source_file: source_file.to_path_buf(),
                            });
                        }
                    }
                }
                for m in members {
                    match m {
                        leekscript_hir::HirClassMember::Field { name, init, .. } => {
                            self.try_define(name, "field", source_file);
                            if let Some(e) = init {
                                self.walk_expr(e, source_file);
                            }
                        }
                        leekscript_hir::HirClassMember::Method {
                            name,
                            is_static,
                            params,
                            body,
                            ..
                        } => {
                            // Java overloads: multiple methods may share a name (arity resolves at runtime).
                            let cur = self.scopes.last_mut().expect("scope stack");
                            cur.entry(name.name.clone()).or_insert(name.span);
                            self.push();
                            let prev_static = self.current_in_static_method;
                            self.current_in_static_method = *is_static;
                            // Unqualified calls may refer to static methods on the enclosing class
                            // (e.g. default args `y = v()`).
                            if let Some(cur_class) = self.current_class_name.as_deref() {
                                if let Some(s) = self.class_static_member_names.get(cur_class) {
                                    let cur = self.scopes.last_mut().expect("scope stack");
                                    for n in s {
                                        cur.entry(n.clone()).or_insert(Span::point(0));
                                    }
                                }
                            }
                            for p in params {
                                self.try_define(&p.name, "param", source_file);
                                if let Some(e) = &p.default {
                                    self.walk_expr(e, source_file);
                                }
                            }
                            for st in body {
                                self.walk_stmt(st, false, source_file);
                            }
                            self.pop();
                            self.current_in_static_method = prev_static;
                        }
                        leekscript_hir::HirClassMember::Constructor { params, body, .. } => {
                            self.push();
                            let prev_static = self.current_in_static_method;
                            self.current_in_static_method = false;
                            if let Some(cur_class) = self.current_class_name.as_deref() {
                                if let Some(s) = self.class_static_member_names.get(cur_class) {
                                    let cur = self.scopes.last_mut().expect("scope stack");
                                    for n in s {
                                        cur.entry(n.clone()).or_insert(Span::point(0));
                                    }
                                }
                            }
                            for p in params {
                                self.try_define(&p.name, "param", source_file);
                                if let Some(e) = &p.default {
                                    self.walk_expr(e, source_file);
                                }
                            }
                            for st in body {
                                self.walk_stmt(st, false, source_file);
                            }
                            self.pop();
                            self.current_in_static_method = prev_static;
                        }
                    }
                }
                self.pop();
                self.current_class_name = prev_class;
                self.current_class_static_method_arities = prev_static_arities;
                self.current_class_instance_method_arities = prev_instance_arities;
            }
            HirStmt::Break | HirStmt::Continue => {}
            HirStmt::Empty => {}
        }
    }

    fn walk_expr(&mut self, e: &HirExpr, source_file: &Path) {
        match e {
            HirExpr::Ident { name, span } => self.resolve_ident(name, *span, source_file),
            HirExpr::Unary { expr, .. } | HirExpr::RefTo { expr, .. } => {
                self.walk_expr(expr, source_file);
            }
            HirExpr::Binary {
                op: HirBinOp::Instanceof,
                left,
                right: _,
            } => {
                self.walk_expr(left, source_file);
            }
            HirExpr::Binary { left, right, .. } => {
                self.walk_expr(left, source_file);
                self.walk_expr(right, source_file);
            }
            HirExpr::Index { base, index, .. } => {
                self.walk_expr(base, source_file);
                self.walk_expr(index, source_file);
            }
            HirExpr::ArraySlice { base, start, end, step, .. } => {
                self.walk_expr(base, source_file);
                if let Some(s) = start {
                    self.walk_expr(s, source_file);
                }
                if let Some(e) = end {
                    self.walk_expr(e, source_file);
                }
                if let Some(st) = step {
                    self.walk_expr(st, source_file);
                }
            }
            HirExpr::Member {
                base,
                field,
                span,
            } => {
                if let HirExpr::ClassSelf { .. } = base.as_ref() {
                    if let Some(cur_class) = self.current_class_name.as_deref() {
                        // `class.name` and `class.class` are built-in pseudo static members.
                        if field == "name" || field == "class" {
                            // still walk the base expression for any nested diagnostics
                        } else {
                        let ok = self
                            .class_static_member_names
                            .get(cur_class)
                            .is_some_and(|s| s.contains(field as &str));
                        if !ok {
                            self.diags.push(ResolveDiagnostic {
                                reference: "CLASS_STATIC_MEMBER_DOES_NOT_EXIST",
                                span: *span,
                                message: format!(
                                    "static member `{field}` does not exist on class `{cur_class}`"
                                ),
                                source_file: source_file.to_path_buf(),
                            });
                        }
                        }
                    }
                }
                if let HirExpr::This = base.as_ref() {
                    if let Some(cur_class) = self.current_class_name.as_deref() {
                        if let Some(access) = self.class_field_access.get(cur_class) {
                            if let Some((decl, vis)) = access.get(field) {
                                if *vis == HirFieldVisibility::Private && decl.as_str() != cur_class {
                                    self.diags.push(ResolveDiagnostic {
                                        reference: "PRIVATE_FIELD",
                                        span: *span,
                                        message: format!(
                                            "private field `{field}` is not accessible from class `{cur_class}`"
                                        ),
                                        source_file: source_file.to_path_buf(),
                                    });
                                }
                            }
                        }
                    }
                }
                self.walk_expr(base, source_file);
            }
            HirExpr::Call { callee, args, .. } => {
                // Validate `class.f(...)` arity against declared static methods (Java compile-time error).
                if let HirExpr::Member { base, field, span } = callee.as_ref() {
                    if matches!(base.as_ref(), HirExpr::ClassSelf { .. }) {
                        if let Some(arities) = self
                            .current_class_static_method_arities
                            .as_ref()
                            .and_then(|m| m.get(field.as_str()))
                        {
                            if !arities.contains(&args.len()) {
                                self.diags.push(ResolveDiagnostic {
                                    reference: "INVALID_PARAMETER_COUNT",
                                    span: *span,
                                    message: format!(
                                        "invalid parameter count for static method `{}`",
                                        field
                                    ),
                                    source_file: source_file.to_path_buf(),
                                });
                            }
                        }
                    }
                    if matches!(base.as_ref(), HirExpr::This) {
                        if let Some(arities) = self
                            .current_class_instance_method_arities
                            .as_ref()
                            .and_then(|m| m.get(field.as_str()))
                        {
                            if !arities.contains(&args.len()) {
                                self.diags.push(ResolveDiagnostic {
                                    reference: "INVALID_PARAMETER_COUNT",
                                    span: *span,
                                    message: format!(
                                        "invalid parameter count for method `{}`",
                                        field
                                    ),
                                    source_file: source_file.to_path_buf(),
                                });
                            }
                        }
                    }
                }
                // Validate unqualified static calls inside a static method body (`static g() { f() }`).
                if self.current_in_static_method {
                    if let HirExpr::Ident { name, span } = callee.as_ref() {
                        let global_defined = self
                            .scopes
                            .first()
                            .is_some_and(|g| g.contains_key(name.as_str()));
                        if let Some(arities) = self
                            .current_class_static_method_arities
                            .as_ref()
                            .and_then(|m| m.get(name.as_str()))
                        {
                            if !global_defined && !arities.contains(&args.len()) {
                                self.diags.push(ResolveDiagnostic {
                                    reference: "INVALID_PARAMETER_COUNT",
                                    span: *span,
                                    message: format!(
                                        "invalid parameter count for static method `{}`",
                                        name
                                    ),
                                    source_file: source_file.to_path_buf(),
                                });
                            }
                        }
                    }
                }
                // Validate unqualified instance calls inside instance methods (`a(x){ b(x) }`).
                if !self.current_in_static_method {
                    if let HirExpr::Ident { name, span } = callee.as_ref() {
                        let global_defined = self
                            .scopes
                            .first()
                            .is_some_and(|g| g.contains_key(name.as_str()));
                        if let Some(arities) = self
                            .current_class_instance_method_arities
                            .as_ref()
                            .and_then(|m| m.get(name.as_str()))
                        {
                            if !global_defined && !arities.contains(&args.len()) {
                                self.diags.push(ResolveDiagnostic {
                                    reference: "INVALID_PARAMETER_COUNT",
                                    span: *span,
                                    message: format!(
                                        "invalid parameter count for method `{}`",
                                        name
                                    ),
                                    source_file: source_file.to_path_buf(),
                                });
                            }
                        }
                    }
                }
                // Inside a class, unqualified calls like `n(this)` can target instance methods.
                // Java resolves them even when there is no local/global binding `n`.
                if let HirExpr::Ident { name, span } = callee.as_ref() {
                    let defined = self
                        .scopes
                        .iter()
                        .rev()
                        .any(|m| m.contains_key(name.as_str()));
                    if !defined {
                        if let Some(cn) = self.current_class_name.as_deref() {
                            let ok = self
                                .class_instance_method_names
                                .get(cn)
                                .is_some_and(|s| s.contains(name as &str));
                            if ok {
                                for a in args {
                                    self.walk_expr(a, source_file);
                                }
                                return;
                            }
                        }
                    }
                    self.resolve_ident(name, *span, source_file);
                } else {
                    self.walk_expr(callee, source_file);
                }
                for a in args {
                    self.walk_expr(a, source_file);
                }
            }
            HirExpr::ArrayLiteral { elements, .. } => {
                for e in elements {
                    self.walk_expr(e, source_file);
                }
            }
            HirExpr::MapLiteral { entries, .. } | HirExpr::ObjectLiteral { entries, .. } => {
                for (k, v) in entries {
                    self.walk_expr(k, source_file);
                    self.walk_expr(v, source_file);
                }
            }
            HirExpr::New { args, .. } => {
                for a in args {
                    self.walk_expr(a, source_file);
                }
            }
            HirExpr::Ternary {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.walk_expr(cond, source_file);
                self.walk_expr(then_expr, source_file);
                self.walk_expr(else_expr, source_file);
            }
            HirExpr::Cast { expr, .. } => {
                self.walk_expr(expr, source_file);
            }
            HirExpr::ArrowClosure { params, body, .. } => {
                self.push();
                for p in params {
                    self.try_define(&p.name, "param", source_file);
                }
                self.walk_expr(body, source_file);
                self.pop();
            }
            HirExpr::FunctionLiteral { params, body, .. } => {
                self.push();
                for p in params {
                    self.try_define(&p.name, "param", source_file);
                }
                for st in body {
                    self.walk_stmt(st, false, source_file);
                }
                self.pop();
            }
            HirExpr::AssignExpr { place, value, .. } => {
                if self.language_version >= 4 {
                    if let HirExpr::Ident { name, span } = place.as_ref() {
                        let is_stdlib = self.name_resolves_to_stdlib_global(name.as_str());
                        let is_file_fn = self.file_fn_decl_names.contains(name.as_str())
                            && !self.file_var_decl_names.contains(name.as_str());
                        if is_stdlib || is_file_fn {
                            self.diags.push(ResolveDiagnostic {
                                reference: "CANNOT_REDEFINE_FUNCTION",
                                span: *span,
                                message: format!("cannot redefine function `{name}`"),
                                source_file: source_file.to_path_buf(),
                            });
                        }
                    }
                }
                self.walk_expr(place, source_file);
                self.walk_expr(value, source_file);
            }
            HirExpr::PostUpdate { target, .. } | HirExpr::PreUpdate { target, .. } => {
                self.walk_expr(target, source_file);
            }
            HirExpr::Integer(_)
            | HirExpr::Real(_)
            | HirExpr::String(_)
            | HirExpr::Bool(_)
            | HirExpr::Null
            | HirExpr::This
            | HirExpr::ClassSelf { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leekscript_hir::{HirExpr, HirFile, HirParam, HirStmt, NameDef};
    use std::path::Path;

    fn sp(n: u32) -> Span {
        Span::new((n as usize)..(n as usize + 1))
    }

    #[test]
    fn duplicate_var_same_block() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::Var {
                    name: NameDef {
                        name: "a".into(),
                        span: sp(0),
                    },
                    init: Some(HirExpr::Integer(1)),
                    decl_ty: None,
                },
                HirStmt::Var {
                    name: NameDef {
                        name: "a".into(),
                        span: sp(1),
                    },
                    init: Some(HirExpr::Integer(2)),
                    decl_ty: None,
                },
            ],
        };
        let d = resolve_hir(&hir, Path::new("t.leek"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].reference, "VARIABLE_NAME_UNAVAILABLE");
    }

    #[test]
    fn inner_block_hides_outer() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Block(vec![
                HirStmt::Var {
                    name: NameDef {
                        name: "a".into(),
                        span: sp(0),
                    },
                    init: Some(HirExpr::Integer(1)),
                    decl_ty: None,
                },
                HirStmt::Block(vec![HirStmt::Var {
                    name: NameDef {
                        name: "a".into(),
                        span: sp(2),
                    },
                    init: Some(HirExpr::Integer(2)),
                    decl_ty: None,
                }]),
            ])],
        };
        let d = resolve_hir(&hir, Path::new("t.leek"));
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn for_in_without_decl_requires_existing_var() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::ForIn {
                name: NameDef {
                    name: "x".into(),
                    span: sp(1),
                },
                is_declaration: false,
                name_by_ref: false,
                container: HirExpr::ArrayLiteral {
                    elements: vec![],
                    span: sp(0),
                },
                body: vec![],
            }],
        };
        let d = resolve_hir(&hir, Path::new("t.leek"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].reference, "VARIABLE_NOT_EXISTS");
    }

    #[test]
    fn stdlib_global_does_not_resolve_as_missing() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Expr(HirExpr::Ident {
                name: "abs".into(),
                span: sp(0),
            })],
        };
        assert!(
            resolve_hir(&hir, Path::new("t.leek")).is_empty(),
            "abs is a pre-defined global"
        );
    }

    #[test]
    fn signature_extra_global_resolves() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Expr(HirExpr::Ident {
                name: "useChip".into(),
                span: sp(0),
            })],
        };
        assert_eq!(resolve_hir(&hir, Path::new("t.leek")).len(), 1);
        assert!(
            resolve_hir_with_extra_globals(
                &hir,
                Path::new("t.leek"),
                &["useChip".into()],
                3,
            )
            .is_empty()
        );
    }

    #[test]
    fn file_level_var_may_shadow_stdlib_global() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Var {
                name: NameDef {
                    name: "abs".into(),
                    span: sp(1),
                },
                init: Some(HirExpr::Integer(1)),
                decl_ty: None,
            }],
        };
        assert!(
            resolve_hir(&hir, Path::new("t.leek")).is_empty(),
            "top-level `var` lives in an inner scope and may shadow stdlib (matches Java Leek)"
        );
    }

    #[test]
    fn global_binding_conflicts_with_stdlib_in_root_scope() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Global {
                decl_ty: None,
                entries: vec![(
                    NameDef {
                        name: "abs".into(),
                        span: sp(1),
                    },
                    Some(HirExpr::Integer(1)),
                )],
            }],
        };
        let d = resolve_hir(&hir, Path::new("t.leek"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].reference, "VARIABLE_NAME_UNAVAILABLE");
    }

    #[test]
    fn assign_to_unknown_name() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Assign {
                place: Box::new(HirExpr::Ident {
                    name: "z".into(),
                    span: sp(0),
                }),
                op: leekscript_hir::HirAssignOp::Assign,
                value: HirExpr::Integer(1),
            }],
        };
        let d = resolve_hir(&hir, Path::new("t.leek"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].reference, "VARIABLE_NOT_EXISTS");
    }

    #[test]
    fn include_at_file_root_ok_for_resolve() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Include {
                path: "x.leek".into(),
                span: sp(0),
            }],
        };
        assert!(resolve_hir(&hir, Path::new("t.leek")).is_empty());
    }

    #[test]
    fn include_nested_in_block_emits_java_reference() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![HirStmt::Block(vec![HirStmt::Include {
                path: "x.leek".into(),
                span: sp(0),
            }])],
        };
        let d = resolve_hir(&hir, Path::new("t.leek"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].reference, "INCLUDE_ONLY_IN_MAIN_BLOCK");
    }

    #[test]
    fn duplicate_empty_body_functions_merge_for_overloads_and_stubs() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::FnDecl {
                    name: NameDef {
                        name: "f".into(),
                        span: sp(0),
                    },
                    params: vec![],
                    return_ty: None,
                    body: vec![],
                },
                HirStmt::FnDecl {
                    name: NameDef {
                        name: "f".into(),
                        span: sp(1),
                    },
                    params: vec![HirParam {
                        name: NameDef {
                            name: "a".into(),
                            span: sp(2),
                        },
                        by_ref: false,
                        default: None,
                        decl_ty: None,
                    }],
                    return_ty: None,
                    body: vec![],
                },
            ],
        };
        assert!(resolve_hir(&hir, Path::new("t.leek")).is_empty());
    }

    #[test]
    fn stub_then_nonempty_function_is_duplicate() {
        let hir = HirFile {
            stmt_sources: vec![],
            stmts: vec![
                HirStmt::FnDecl {
                    name: NameDef {
                        name: "g".into(),
                        span: sp(0),
                    },
                    params: vec![],
                    return_ty: None,
                    body: vec![],
                },
                HirStmt::FnDecl {
                    name: NameDef {
                        name: "g".into(),
                        span: sp(1),
                    },
                    params: vec![],
                    return_ty: None,
                    body: vec![HirStmt::Return {
                        value: Some(HirExpr::Integer(1)),
                        if_truthy: false,
                        by_ref: false,
                    }],
                },
            ],
        };
        let d = resolve_hir(&hir, Path::new("t.leek"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].reference, "VARIABLE_NAME_UNAVAILABLE");
    }
}
