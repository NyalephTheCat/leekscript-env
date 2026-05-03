//! Minimal type analysis for Java parity checks.
//!
//! This crate intentionally starts small: today it only validates `as` casts where the result is
//! provably impossible from local information (literals and obvious constructors).

use leekscript_hir::{HirExpr, HirFile, HirStmt, HirSwitchClause, HirTypeExpr};
use leekscript_span::Span;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDiagnostic {
    pub reference: &'static str,
    pub span: Span,
    pub message: String,
    pub source_file: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ValueTy {
    Integer,
    Real,
    String,
    Boolean,
    Null,
    /// Bracket map / `new Map` — Java `MapLeekValue` / `MapType`.
    Map,
    /// `{ key: value }` literal — Java `ObjectLeekValue` / `ObjectType` (not `MapType`).
    LeekObject,
    Class(String),
    Unknown,
}

#[must_use]
pub fn check_hir_types(hir: &HirFile, main_source: &Path) -> Vec<TypeDiagnostic> {
    let mut out = Vec::new();
    let use_sources = hir.stmt_sources.len() == hir.stmts.len();
    for (i, s) in hir.stmts.iter().enumerate() {
        let source_file = if use_sources {
            hir.stmt_sources[i].as_path()
        } else {
            main_source
        };
        walk_stmt(s, source_file, &mut out);
    }
    out
}

fn walk_stmt(s: &HirStmt, source_file: &Path, out: &mut Vec<TypeDiagnostic>) {
    match s {
        HirStmt::Var { init, .. } => {
            if let Some(init) = init {
                walk_expr(init, source_file, out);
            }
        }
        HirStmt::Expr(e) => walk_expr(e, source_file, out),
        HirStmt::Return { value, .. } => {
            if let Some(v) = value {
                walk_expr(v, source_file, out);
            }
        }
        HirStmt::Block(stmts) => {
            for st in stmts {
                walk_stmt(st, source_file, out);
            }
        }
        HirStmt::FnDecl { body, .. } => {
            for st in body {
                walk_stmt(st, source_file, out);
            }
        }
        HirStmt::ClassDecl { members, .. } => {
            for m in members {
                match m {
                    leekscript_hir::HirClassMember::Field { .. } => {}
                    leekscript_hir::HirClassMember::Method { body, .. } => {
                        for st in body {
                            walk_stmt(st, source_file, out);
                        }
                    }
                    leekscript_hir::HirClassMember::Constructor { body, .. } => {
                        for st in body {
                            walk_stmt(st, source_file, out);
                        }
                    }
                }
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            walk_expr(cond, source_file, out);
            for st in then_body {
                walk_stmt(st, source_file, out);
            }
            if let Some(eb) = else_body {
                for st in eb {
                    walk_stmt(st, source_file, out);
                }
            }
        }
        HirStmt::While { cond, body } => {
            walk_expr(cond, source_file, out);
            for st in body {
                walk_stmt(st, source_file, out);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for st in body {
                walk_stmt(st, source_file, out);
            }
            walk_expr(cond, source_file, out);
        }
        HirStmt::Switch { discr, clauses } => {
            walk_expr(discr, source_file, out);
            for c in clauses {
                match c {
                    HirSwitchClause::Case { labels, body } => {
                        for l in labels {
                            walk_expr(l, source_file, out);
                        }
                        for st in body {
                            walk_stmt(st, source_file, out);
                        }
                    }
                    HirSwitchClause::Default { body } => {
                        for st in body {
                            walk_stmt(st, source_file, out);
                        }
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
            if let Some(i) = init {
                walk_stmt(i, source_file, out);
            }
            if let Some(c) = cond {
                walk_expr(c, source_file, out);
            }
            if let Some(u) = update {
                match u {
                    leekscript_hir::HirForStep::Assign(h) => {
                        walk_expr(&h.value, source_file, out);
                    }
                    leekscript_hir::HirForStep::Expr(e) => {
                        walk_expr(e, source_file, out);
                    }
                }
            }
            for st in body {
                walk_stmt(st, source_file, out);
            }
        }
        HirStmt::ForIn {
            container, body, ..
        } => {
            walk_expr(container, source_file, out);
            for st in body {
                walk_stmt(st, source_file, out);
            }
        }
        HirStmt::ForInKeyValue {
            container, body, ..
        } => {
            walk_expr(container, source_file, out);
            for st in body {
                walk_stmt(st, source_file, out);
            }
        }
        HirStmt::Assign { place, value, .. } => {
            walk_expr(place, source_file, out);
            walk_expr(value, source_file, out);
        }
        HirStmt::Break | HirStmt::Continue | HirStmt::Empty => {}
        HirStmt::Try {
            try_body,
            catch,
            finally_body,
        } => {
            for st in try_body {
                walk_stmt(st, source_file, out);
            }
            if let Some(c) = catch {
                for st in &c.1 {
                    walk_stmt(st, source_file, out);
                }
            }
            if let Some(f) = finally_body {
                for st in f {
                    walk_stmt(st, source_file, out);
                }
            }
        }
        HirStmt::Throw(value) => {
            if let Some(v) = value {
                walk_expr(v, source_file, out);
            }
        }
        HirStmt::Global { entries, .. } => {
            for (_, init) in entries {
                if let Some(e) = init {
                    walk_expr(e, source_file, out);
                }
            }
        }
        HirStmt::Include { .. } => {}
    }
}

fn walk_expr(e: &HirExpr, source_file: &Path, out: &mut Vec<TypeDiagnostic>) {
    match e {
        HirExpr::Unary { expr, .. } | HirExpr::RefTo { expr, .. } => {
            walk_expr(expr, source_file, out);
        }
        HirExpr::Binary { left, right, .. } => {
            walk_expr(left, source_file, out);
            walk_expr(right, source_file, out);
        }
        HirExpr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            walk_expr(cond, source_file, out);
            walk_expr(then_expr, source_file, out);
            walk_expr(else_expr, source_file, out);
        }
        HirExpr::Cast { expr, ty, span } => {
            walk_expr(expr, source_file, out);
            if let Some(diag) = check_cast(expr, ty, *span, source_file) {
                out.push(diag);
            }
        }
        HirExpr::Call { callee, args, .. } => {
            walk_expr(callee, source_file, out);
            for a in args {
                walk_expr(a, source_file, out);
            }
        }
        HirExpr::ArrayLiteral { elements, .. } => {
            for el in elements {
                walk_expr(el, source_file, out);
            }
        }
        HirExpr::MapLiteral { entries, .. } | HirExpr::ObjectLiteral { entries, .. } => {
            for (k, v) in entries {
                walk_expr(k, source_file, out);
                walk_expr(v, source_file, out);
            }
        }
        HirExpr::New { args, .. } => {
            for a in args {
                walk_expr(a, source_file, out);
            }
        }
        HirExpr::Index { base, index, .. } => {
            walk_expr(base, source_file, out);
            walk_expr(index, source_file, out);
        }
        HirExpr::Member { base, .. } => walk_expr(base, source_file, out),
        HirExpr::ArrowClosure { body, .. } => walk_expr(body, source_file, out),
        HirExpr::FunctionLiteral { body, .. } => {
            for st in body {
                walk_stmt(st, source_file, out);
            }
        }
        HirExpr::AssignExpr { place, value, .. } => {
            walk_expr(place, source_file, out);
            walk_expr(value, source_file, out);
        }
        HirExpr::PostUpdate { target, .. } | HirExpr::PreUpdate { target, .. } => {
            walk_expr(target, source_file, out);
        }
        HirExpr::ArraySlice {
            base,
            start,
            end,
            step,
            ..
        } => {
            walk_expr(base, source_file, out);
            if let Some(x) = start {
                walk_expr(x, source_file, out);
            }
            if let Some(x) = end {
                walk_expr(x, source_file, out);
            }
            if let Some(x) = step {
                walk_expr(x, source_file, out);
            }
        }
        HirExpr::Ident { .. }
        | HirExpr::Integer(_)
        | HirExpr::Real(_)
        | HirExpr::String(_)
        | HirExpr::Bool(_)
        | HirExpr::Null
        | HirExpr::This
        | HirExpr::ClassSelf { .. } => {}
    }
}

fn check_cast(
    expr: &HirExpr,
    ty: &HirTypeExpr,
    span: Span,
    source_file: &Path,
) -> Option<TypeDiagnostic> {
    // Start conservative: only emit when *provably* impossible from local info.
    let src = expr_value_ty(expr);
    let dst = normalize_target_ty(ty);

    // `null` can be cast to anything in the Java runtime model (it stays null).
    if matches!(src, ValueTy::Null) {
        return None;
    }

    match dst {
        None => None,
        Some(ValueTy::Unknown) => None,
        Some(ValueTy::String) => None, // Stringification is always possible at runtime.
        Some(ValueTy::Real | ValueTy::Integer) => match src {
            ValueTy::Integer | ValueTy::Real => None,
            ValueTy::Unknown => None,
            _ => Some(TypeDiagnostic {
                reference: "IMPOSSIBLE_CAST",
                span,
                message: "impossible cast to numeric type".into(),
                source_file: source_file.to_path_buf(),
            }),
        },
        Some(ValueTy::Boolean) => match src {
            ValueTy::Boolean => None,
            ValueTy::Unknown => None,
            _ => Some(TypeDiagnostic {
                reference: "IMPOSSIBLE_CAST",
                span,
                message: "impossible cast to boolean".into(),
                source_file: source_file.to_path_buf(),
            }),
        },
        Some(ValueTy::Class(class_name)) => match src {
            ValueTy::Class(src_class) if src_class == class_name => None,
            ValueTy::Unknown => None,
            _ => Some(TypeDiagnostic {
                reference: "IMPOSSIBLE_CAST",
                span,
                message: format!("impossible cast to `{class_name}`"),
                source_file: source_file.to_path_buf(),
            }),
        },
        Some(ValueTy::Map) => match src {
            ValueTy::Map => None,
            ValueTy::Unknown | ValueTy::Null => None,
            _ => Some(TypeDiagnostic {
                reference: "IMPOSSIBLE_CAST",
                span,
                message: "impossible cast to `Map`".into(),
                source_file: source_file.to_path_buf(),
            }),
        },
        Some(ValueTy::LeekObject) => match src {
            ValueTy::LeekObject => None,
            ValueTy::Unknown | ValueTy::Null => None,
            _ => Some(TypeDiagnostic {
                reference: "IMPOSSIBLE_CAST",
                span,
                message: "impossible cast to `Object`".into(),
                source_file: source_file.to_path_buf(),
            }),
        },
        Some(ValueTy::Null) => None,
    }
}

fn normalize_target_ty(ty: &HirTypeExpr) -> Option<ValueTy> {
    match ty {
        HirTypeExpr::Named(n) => Some(match n.as_str() {
            "integer" => ValueTy::Integer,
            "real" => ValueTy::Real,
            "string" => ValueTy::String,
            "boolean" => ValueTy::Boolean,
            // Java `ObjectType` — distinct from `MapType` and from user `class` types.
            "Object" => ValueTy::LeekObject,
            "any" | "Class" | "void" => ValueTy::Unknown,
            other => ValueTy::Class(other.to_string()),
        }),
        HirTypeExpr::Nullable(inner) => normalize_target_ty(inner),
        HirTypeExpr::Union(_tys) => Some(ValueTy::Unknown),
        HirTypeExpr::Generic { base, .. } => {
            if base == "Map" {
                Some(ValueTy::Map)
            } else {
                Some(ValueTy::Class(base.clone()))
            }
        }
    }
}

fn expr_value_ty(e: &HirExpr) -> ValueTy {
    match e {
        HirExpr::Integer(_) => ValueTy::Integer,
        HirExpr::Real(_) => ValueTy::Real,
        HirExpr::String(_) => ValueTy::String,
        HirExpr::Bool(_) => ValueTy::Boolean,
        HirExpr::Null => ValueTy::Null,
        HirExpr::MapLiteral { .. } => ValueTy::Map,
        HirExpr::ObjectLiteral { .. } => ValueTy::LeekObject,
        HirExpr::New { type_name, .. } => ValueTy::Class(type_name.clone()),
        HirExpr::Cast { ty, .. } => normalize_target_ty(ty).unwrap_or(ValueTy::Unknown),
        _ => ValueTy::Unknown,
    }
}
