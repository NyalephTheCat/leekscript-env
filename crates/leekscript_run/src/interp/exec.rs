//! Statement execution: control flow, loops, `try`/`catch`, `for`-`in`.

use super::context::InterpCx;
use super::error::{ExecAbort, InterpretError};
use super::expr::eval_expr;
use super::flow::StmtFlow;
use super::lvalue::assign_place;
use super::util::{
    pass_parameter_value, value_to_for_in_key_value_pairs, value_to_for_in_sequence, value_truthy,
    values_equal_for_compare,
};
use super::value::Value;
use leekscript_hir::{HirAssignOp, HirExpr, HirForStep, HirForUpdate, HirStmt, HirSwitchClause};
use std::path::PathBuf;

#[inline]
fn pop_block_release_ram(cx: &mut InterpCx) {
    let dropped = cx.env.pop_block();
    super::ram::release_dropped_binding_values_ram(cx, dropped);
}

fn strict_infer_expect_tag(s: &str) -> Option<&str> {
    s.strip_prefix("#infer:")
}

fn strict_var_init_infer_tag(v: &Value) -> &'static str {
    match v {
        Value::Bool(_) => "boolean",
        Value::Integer(_) => "integer",
        Value::Real(_) | Value::RealDotZero(_) => "real",
        Value::String(_) => "string",
        Value::Array(_) => "Array",
        Value::Map(_) => "Map",
        Value::Object(_) => "Object",
        Value::Set(_) => "Set",
        Value::Interval(_) => "Interval",
        Value::Function(_) => "Function",
        Value::Native(_) => "Function",
        Value::Instance(_) => "Object",
        Value::UserClass(_) => "Class",
        Value::Null => "null",
        Value::Super => "super",
    }
}

fn strict_literal_tag(e: &HirExpr) -> Option<&'static str> {
    match e {
        HirExpr::Integer(_) => Some("integer"),
        HirExpr::Real(_) => Some("real"),
        HirExpr::String(_) => Some("string"),
        HirExpr::Bool(_) => Some("boolean"),
        HirExpr::Null => Some("null"),
        HirExpr::ArrayLiteral { .. } => Some("Array"),
        HirExpr::MapLiteral { .. } => Some("Map"),
        HirExpr::ObjectLiteral { .. } => Some("Object"),
        HirExpr::New { type_name, .. } => match type_name.as_str() {
            "Map" => Some("Map"),
            "Set" => Some("Set"),
            "Interval" => Some("Interval"),
            _ => None,
        },
        _ => None,
    }
}

fn strict_infer_numeric_ok(expect: &str, got: &str) -> bool {
    matches!((expect, got), ("integer" | "real", "integer" | "real"))
}

fn strict_scan_stmts_for_infer_assignment_errors(
    cx: &InterpCx,
    stmts: &[HirStmt],
) -> Result<(), ExecAbort> {
    for s in stmts {
        match s {
            HirStmt::Block(b) => strict_scan_stmts_for_infer_assignment_errors(cx, b)?,
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                strict_scan_stmts_for_infer_assignment_errors(cx, then_body)?;
                if let Some(els) = else_body {
                    strict_scan_stmts_for_infer_assignment_errors(cx, els)?;
                }
            }
            HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. }
            | HirStmt::ForIn { body, .. }
            | HirStmt::ForInKeyValue { body, .. } => {
                strict_scan_stmts_for_infer_assignment_errors(cx, body)?;
            }
            HirStmt::Switch { clauses, .. } => {
                for cl in clauses {
                    match cl {
                        HirSwitchClause::Case { body, .. } => {
                            strict_scan_stmts_for_infer_assignment_errors(cx, body)?;
                        }
                        HirSwitchClause::Default { body } => {
                            strict_scan_stmts_for_infer_assignment_errors(cx, body)?;
                        }
                    }
                }
            }
            HirStmt::Try {
                try_body,
                catch,
                finally_body,
            } => {
                strict_scan_stmts_for_infer_assignment_errors(cx, try_body)?;
                if let Some((_, body)) = catch {
                    strict_scan_stmts_for_infer_assignment_errors(cx, body)?;
                }
                if let Some(body) = finally_body {
                    strict_scan_stmts_for_infer_assignment_errors(cx, body)?;
                }
            }
            HirStmt::Assign { place, value, op } => {
                if cx.strict != Some(true) || !matches!(op, HirAssignOp::Assign) {
                    continue;
                }
                let HirExpr::Ident { name, .. } = place.as_ref() else {
                    continue;
                };
                let Some(Some(t)) = cx.binding_decl_ty.get(name) else {
                    continue;
                };
                let Some(expect) = strict_infer_expect_tag(t) else {
                    continue;
                };
                let Some(got) = strict_literal_tag(value) else {
                    continue;
                };
                if got == "null" || expect == "any" {
                    continue;
                }
                if strict_infer_numeric_ok(expect, got) {
                    continue;
                }
                return Err(InterpretError::assignment_incompatible_type().into());
            }
            HirStmt::Expr(HirExpr::AssignExpr {
                place, op, value, ..
            }) => {
                if cx.strict != Some(true) || !matches!(op, HirAssignOp::Assign) {
                    continue;
                }
                let HirExpr::Ident { name, .. } = place.as_ref() else {
                    continue;
                };
                let Some(Some(t)) = cx.binding_decl_ty.get(name) else {
                    continue;
                };
                let Some(expect) = strict_infer_expect_tag(t) else {
                    continue;
                };
                let Some(got) = strict_literal_tag(value) else {
                    continue;
                };
                if got == "null" || expect == "any" {
                    continue;
                }
                if strict_infer_numeric_ok(expect, got) {
                    continue;
                }
                return Err(InterpretError::assignment_incompatible_type().into());
            }
            _ => {}
        }
    }
    Ok(())
}

/// Leek **v1**: `var` / `global` copies containers when the initializer reads storage (`a`, `a[i]`, …).
/// Values from calls, literals, `new`, or `@expr` are not copied again here (call/return already applied copy vs ref).
fn v1_var_init_skip_container_copy(init: &HirExpr) -> bool {
    match init {
        HirExpr::RefTo { .. } => true,
        HirExpr::Call { .. } => true,
        HirExpr::ArrayLiteral { .. }
        | HirExpr::MapLiteral { .. }
        | HirExpr::ObjectLiteral { .. }
        | HirExpr::New { .. } => true,
        HirExpr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            v1_var_init_skip_container_copy(then_expr) && v1_var_init_skip_container_copy(else_expr)
        }
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => {
            v1_var_init_skip_container_copy(expr)
        }
        HirExpr::FunctionLiteral { .. } | HirExpr::ArrowClosure { .. } => true,
        _ => false,
    }
}

fn pass_return_value(cx: &InterpCx, v: Value, by_ref: bool) -> Value {
    // Java suite: in v1, `return` at the script/top level preserves container identity;
    // only user-callable returns are copied by default.
    let v1_script_return_by_ref = cx.language_version == 1 && !cx.env.in_user_callable();
    let v1_cb_container_return = cx.language_version == 1
        && cx.v1_array_cb_depth > 0
        && !by_ref
        && matches!(
            &v,
            Value::Array(_) | Value::Map(_) | Value::Object(_) | Value::Set(_)
        );
    pass_parameter_value(
        cx.language_version,
        v,
        by_ref || v1_cb_container_return || v1_script_return_by_ref,
    )
}

fn strict_check_declared_return_type(cx: &InterpCx, v: &Value) -> Result<(), InterpretError> {
    if cx.strict != Some(true) {
        return Ok(());
    }
    let Some(Some(rt)) = cx.fn_return_ty_stack.last().map(|x| x.as_ref()) else {
        return Ok(());
    };
    // Minimal parity: strict v4 enforces declared return types.
    let ok = match rt.as_str() {
        "void" => false,
        "integer" | "int" => matches!(v, Value::Integer(_)),
        "real" | "float" | "double" => matches!(v, Value::Real(_) | Value::Integer(_)),
        "number" => matches!(v, Value::Real(_) | Value::Integer(_)),
        "string" => matches!(v, Value::String(_)),
        "boolean" | "bool" => matches!(v, Value::Bool(_)),
        "any" => true,
        // For now, only enforce primitives used by the Java suite.
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(InterpretError::incompatible_type())
    }
}

fn coerce_var_init(
    v: Value,
    decl_ty: Option<&str>,
    language_version: u8,
) -> Result<Value, InterpretError> {
    super::util::coerce_var_init_value(v, decl_ty, language_version)
}

/// Java-style hoisting: file-scope `function name(...) {}` is visible to every top-level statement,
/// including `var` initializers that appear earlier in source order.
pub(super) fn hoist_top_level_function_decls(cx: &mut InterpCx, stmts: &[HirStmt]) {
    for st in stmts {
        let HirStmt::FnDecl {
            name,
            params,
            return_ty,
            body,
        } = st
        else {
            continue;
        };
        let pnames: Vec<String> = params.iter().map(|p| p.name.name.clone()).collect();
        let pref: Vec<bool> = params.iter().map(|p| p.by_ref).collect();
        let pty: Vec<Option<String>> = params.iter().map(|p| p.decl_ty.clone()).collect();
        let pdef: Vec<Option<HirExpr>> = params.iter().map(|p| p.default.clone()).collect();
        cx.env.insert(
            name.name.clone(),
            Value::Function(std::rc::Rc::new(super::value::FunctionValue {
                params: pnames,
                param_by_ref: pref,
                param_decl_tys: pty,
                param_defaults: pdef,
                body: body.clone(),
                captured_locals: cx
                    .env
                    .in_user_callable()
                    .then(|| cx.env.snapshot_callable_visible_non_global()),
                captured_aliases: cx
                    .env
                    .in_user_callable()
                    .then(|| cx.env.snapshot_callable_aliases_non_global()),
                declared_return_ty: return_ty.clone(),
                unbound_method_ref: false,
            })),
        );
    }
}

pub(super) fn exec_stmts(
    cx: &mut InterpCx,
    stmts: &[HirStmt],
    record_script_tail: bool,
) -> Result<StmtFlow, InterpretError> {
    for s in stmts {
        match exec_stmt(cx, s, record_script_tail) {
            Ok(StmtFlow::Throw(v)) | Err(ExecAbort::Throw(v)) => return Ok(StmtFlow::Throw(v)),
            Err(ExecAbort::Error(e)) => return Err(e),
            Ok(flow) => match flow {
                StmtFlow::Continue => {}
                StmtFlow::Return(v) => return Ok(StmtFlow::Return(v)),
                StmtFlow::Break => return Ok(StmtFlow::Break),
                StmtFlow::ContinueLoop => return Ok(StmtFlow::ContinueLoop),
                StmtFlow::Throw(_) => unreachable!("handled above"),
            },
        }
    }
    Ok(StmtFlow::Continue)
}

/// Like [`exec_stmts`], but sets [`InterpCx::debug_active_file`] per statement for `debug*()` metadata.
pub(super) fn exec_stmts_with_debug_files(
    cx: &mut InterpCx,
    stmts: &[HirStmt],
    files: &[PathBuf],
    record_script_tail: bool,
) -> Result<StmtFlow, InterpretError> {
    if stmts.len() != files.len() {
        return Err(InterpretError {
            reference: "INTERNAL_ERROR",
            message: format!(
                "debug file list length {} does not match stmt list {}",
                files.len(),
                stmts.len(),
            ),
        });
    }
    for (s, f) in stmts.iter().zip(files.iter()) {
        let prev = cx.debug_active_file.replace(f.clone());
        match exec_stmt(cx, s, record_script_tail) {
            Ok(StmtFlow::Throw(v)) | Err(ExecAbort::Throw(v)) => return Ok(StmtFlow::Throw(v)),
            Err(ExecAbort::Error(e)) => return Err(e),
            Ok(flow) => match flow {
                StmtFlow::Continue => {}
                StmtFlow::Return(v) => return Ok(StmtFlow::Return(v)),
                StmtFlow::Break => return Ok(StmtFlow::Break),
                StmtFlow::ContinueLoop => return Ok(StmtFlow::ContinueLoop),
                StmtFlow::Throw(_) => unreachable!("handled above"),
            },
        }
        cx.debug_active_file = prev;
    }
    Ok(StmtFlow::Continue)
}

pub(super) fn exec_stmt(
    cx: &mut InterpCx,
    s: &HirStmt,
    record_script_tail: bool,
) -> Result<StmtFlow, ExecAbort> {
    if record_script_tail && !matches!(s, HirStmt::Expr(_)) {
        cx.script_result_expr = None;
    }
    match s {
        HirStmt::Var {
            name,
            init,
            decl_ty,
        } => {
            let init_ops = init
                .as_ref()
                .map_or(0, super::java_ops_budget::hir_java_expr_ops_budget);
            cx.charge_ops(1u64.saturating_add(init_ops))
                .map_err(ExecAbort::Error)?;
            let v = match init {
                Some(e) => {
                    let ref_initializer = matches!(e, HirExpr::RefTo { .. });
                    let raw = eval_expr(cx, e)?;
                    let bound = if ref_initializer
                        || (cx.language_version == 1 && v1_var_init_skip_container_copy(e))
                    {
                        raw
                    } else {
                        pass_parameter_value(cx.language_version, raw, false)
                    };
                    coerce_var_init(bound, decl_ty.as_deref(), cx.language_version)?
                }
                None => Value::Null,
            };
            let inferred_ty =
                if cx.strict == Some(true) && decl_ty.is_none() && !matches!(v, Value::Null) {
                    // Strict `var` infers a stable type from the initializer.
                    let tag = strict_var_init_infer_tag(&v);
                    Some(format!("#infer:{tag}"))
                } else {
                    None
                };
            cx.insert_local_var(name.name.clone(), v)?;
            if let Some(inferred) = inferred_ty {
                cx.binding_decl_ty.insert(name.name.clone(), Some(inferred));
            } else {
                cx.binding_decl_ty
                    .insert(name.name.clone(), decl_ty.clone());
            }
            Ok(StmtFlow::Continue)
        }
        HirStmt::Global { decl_ty, entries } => {
            for (name, init) in entries {
                let init_ops = init
                    .as_ref()
                    .map_or(0, super::java_ops_budget::hir_java_expr_ops_budget);
                cx.charge_ops(1u64.saturating_add(init_ops))
                    .map_err(ExecAbort::Error)?;
                let v = match init {
                    Some(e) => {
                        let ref_initializer = matches!(e, HirExpr::RefTo { .. });
                        let raw = eval_expr(cx, e)?;
                        let bound = if ref_initializer
                            || (cx.language_version == 1 && v1_var_init_skip_container_copy(e))
                        {
                            raw
                        } else {
                            pass_parameter_value(cx.language_version, raw, false)
                        };
                        coerce_var_init(bound, decl_ty.as_deref(), cx.language_version)?
                    }
                    None => {
                        if cx.language_version >= 4 {
                            if let Some(ty) = decl_ty.as_deref() {
                                if ty.trim_start().starts_with("Map") {
                                    Value::map_from(Vec::new())
                                } else {
                                    Value::Null
                                }
                            } else {
                                Value::Null
                            }
                        } else {
                            Value::Null
                        }
                    }
                };
                cx.insert_global_var(name.name.clone(), v)?;
                if let Some(t) = decl_ty.as_ref() {
                    cx.binding_decl_ty
                        .insert(name.name.clone(), Some(t.clone()));
                }
            }
            Ok(StmtFlow::Continue)
        }
        HirStmt::Include { .. } => Err(InterpretError {
            reference: "INTERNAL_ERROR",
            message: "`include` was not expanded before execution".into(),
        }
        .into()),
        HirStmt::FnDecl {
            name,
            params,
            return_ty,
            body,
        } => {
            let pnames: Vec<String> = params.iter().map(|p| p.name.name.clone()).collect();
            let pref: Vec<bool> = params.iter().map(|p| p.by_ref).collect();
            let pty: Vec<Option<String>> = params.iter().map(|p| p.decl_ty.clone()).collect();
            let pdef: Vec<Option<HirExpr>> = params.iter().map(|p| p.default.clone()).collect();
            cx.env.insert(
                name.name.clone(),
                Value::Function(std::rc::Rc::new(super::value::FunctionValue {
                    params: pnames,
                    param_by_ref: pref,
                    param_decl_tys: pty,
                    param_defaults: pdef,
                    body: body.clone(),
                    captured_locals: cx
                        .env
                        .in_user_callable()
                        .then(|| cx.env.snapshot_callable_visible_non_global()),
                    captured_aliases: cx
                        .env
                        .in_user_callable()
                        .then(|| cx.env.snapshot_callable_aliases_non_global()),
                    declared_return_ty: return_ty.clone(),
                    unbound_method_ref: false,
                })),
            );
            Ok(StmtFlow::Continue)
        }
        HirStmt::ClassDecl { name, .. } => {
            let cn = name.name.clone();
            let (order, inits, decl_tys) = cx
                .classes
                .get(&cn)
                .map(|def| {
                    (
                        def.static_field_order.clone(),
                        def.static_field_inits.clone(),
                        def.static_field_decl_tys.clone(),
                    )
                })
                .unwrap_or_default();
            // `UserClass` binding + lexical class for visibility must exist before static field
            // initializers run (`class A { static y = A.x }`, private `A.x`, …).
            cx.env
                .insert(cn.clone(), Value::UserClass(name.name.clone()));
            cx.enclosing_class_stack.push(cn.clone());
            cx.env.push_block();
            let inits_result = (|| -> Result<(), ExecAbort> {
                for fname in order {
                    if let Some(expr) = inits.get(&fname) {
                        let raw = eval_expr(cx, expr)?;
                        let decl = decl_tys.get(&fname).map(std::string::String::as_str);
                        let v = super::util::coerce_var_init_value(raw, decl, cx.language_version)?;
                        if let Some(def) = cx.classes.get_mut(&cn) {
                            def.static_fields.insert(fname.clone(), v.clone());
                        }
                        cx.env.insert(fname, v);
                    }
                }
                Ok(())
            })();
            pop_block_release_ram(cx);
            cx.enclosing_class_stack.pop();
            inits_result?;
            Ok(StmtFlow::Continue)
        }
        HirStmt::Assign { place, op, value } => {
            let n = super::java_ops_budget::hir_java_assign_ops(place.as_ref(), *op, value);
            if n > 0 {
                cx.charge_ops(n).map_err(ExecAbort::Error)?;
            }
            assign_place(cx, place.as_ref(), *op, value)?;
            Ok(StmtFlow::Continue)
        }
        HirStmt::Try {
            try_body,
            catch,
            finally_body,
        } => {
            let try_out = exec_stmts(cx, try_body, false)?;
            let after_try_catch = match try_out {
                StmtFlow::Throw(val) => {
                    if let Some((param, catch_body)) = catch {
                        cx.env.push_block();
                        cx.env
                            .insert(param.name.clone(), val.clone().unwrap_or(Value::Null));
                        let out = exec_stmts(cx, catch_body, false)?;
                        pop_block_release_ram(cx);
                        out
                    } else {
                        let fin = match finally_body {
                            Some(fb) => exec_stmts(cx, fb, false)?,
                            None => StmtFlow::Continue,
                        };
                        if !matches!(fin, StmtFlow::Continue) {
                            return Ok(fin);
                        }
                        return Ok(StmtFlow::Throw(val));
                    }
                }
                other => other,
            };
            let fin = match finally_body {
                Some(fb) => exec_stmts(cx, fb, false)?,
                None => StmtFlow::Continue,
            };
            if !matches!(fin, StmtFlow::Continue) {
                return Ok(fin);
            }
            Ok(after_try_catch)
        }
        HirStmt::Throw(e) => {
            let v = match e {
                Some(x) => Some(eval_expr(cx, x)?),
                None => Some(Value::Null),
            };
            Ok(StmtFlow::Throw(v))
        }
        HirStmt::Break => Ok(StmtFlow::Break),
        HirStmt::Continue => Ok(StmtFlow::ContinueLoop),
        HirStmt::Empty => Ok(StmtFlow::Continue),
        HirStmt::Expr(e) => {
            let stmt_ops = super::java_ops_budget::hir_java_expr_ops_budget(e);
            if stmt_ops > 0 {
                cx.charge_ops(stmt_ops).map_err(ExecAbort::Error)?;
            }
            let v = eval_expr(cx, e)?;
            if record_script_tail {
                cx.script_result_expr = Some(v);
            }
            Ok(StmtFlow::Continue)
        }
        HirStmt::Return {
            value,
            if_truthy,
            by_ref,
        } => match value {
            None => Ok(StmtFlow::Return(None)),
            Some(x) if *if_truthy => {
                let ro = super::java_ops_budget::hir_java_expr_ops_budget(x);
                if ro > 0 {
                    cx.charge_ops(ro).map_err(ExecAbort::Error)?;
                }
                let v = eval_expr(cx, x)?;
                if value_truthy(&v) {
                    strict_check_declared_return_type(cx, &v)?;
                    Ok(StmtFlow::Return(Some(pass_return_value(cx, v, *by_ref))))
                } else {
                    Ok(StmtFlow::Continue)
                }
            }
            Some(x) => {
                let ro = super::java_ops_budget::hir_java_expr_ops_budget(x);
                if ro > 0 {
                    cx.charge_ops(ro).map_err(ExecAbort::Error)?;
                }
                let v = eval_expr(cx, x)?;
                strict_check_declared_return_type(cx, &v)?;
                Ok(StmtFlow::Return(Some(pass_return_value(cx, v, *by_ref))))
            }
        },
        HirStmt::Block(stmts) => {
            cx.env.push_block();
            let out = exec_stmts(cx, stmts, record_script_tail);
            pop_block_release_ram(cx);
            Ok(out?)
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let co = super::java_ops_budget::hir_java_cond_outer_charge(cond);
            if co > 0 {
                cx.charge_ops(co).map_err(ExecAbort::Error)?;
            }
            if cx.strict == Some(true) {
                strict_scan_stmts_for_infer_assignment_errors(cx, then_body)?;
                if let Some(els) = else_body.as_ref() {
                    strict_scan_stmts_for_infer_assignment_errors(cx, els)?;
                }
            }
            if value_truthy(&eval_expr(cx, cond)?) {
                cx.env.push_block();
                let out = exec_stmts(cx, then_body, false)?;
                pop_block_release_ram(cx);
                match out {
                    StmtFlow::Return(v) => Ok(StmtFlow::Return(v)),
                    StmtFlow::Break => Ok(StmtFlow::Break),
                    StmtFlow::ContinueLoop => Ok(StmtFlow::ContinueLoop),
                    StmtFlow::Throw(v) => Ok(StmtFlow::Throw(v)),
                    StmtFlow::Continue => Ok(StmtFlow::Continue),
                }
            } else if let Some(els) = else_body {
                cx.env.push_block();
                let out = exec_stmts(cx, els, false)?;
                pop_block_release_ram(cx);
                match out {
                    StmtFlow::Return(v) => Ok(StmtFlow::Return(v)),
                    StmtFlow::Break => Ok(StmtFlow::Break),
                    StmtFlow::ContinueLoop => Ok(StmtFlow::ContinueLoop),
                    StmtFlow::Throw(v) => Ok(StmtFlow::Throw(v)),
                    StmtFlow::Continue => Ok(StmtFlow::Continue),
                }
            } else {
                Ok(StmtFlow::Continue)
            }
        }
        HirStmt::While { cond, body } => loop {
            let co = super::java_ops_budget::hir_java_loop_cond_outer_charge(cond);
            if co > 0 {
                cx.charge_ops(co).map_err(ExecAbort::Error)?;
            }
            if !value_truthy(&eval_expr(cx, cond)?) {
                break Ok(StmtFlow::Continue);
            }
            cx.charge_ops(1).map_err(ExecAbort::Error)?;
            cx.env.push_block();
            let flow = exec_stmts(cx, body, false)?;
            pop_block_release_ram(cx);
            match flow {
                StmtFlow::Continue => {}
                StmtFlow::Return(v) => break Ok(StmtFlow::Return(v)),
                StmtFlow::Break => break Ok(StmtFlow::Continue),
                StmtFlow::ContinueLoop => continue,
                StmtFlow::Throw(v) => break Ok(StmtFlow::Throw(v)),
            }
        },
        HirStmt::DoWhile { body, cond } => {
            loop {
                cx.charge_ops(1).map_err(ExecAbort::Error)?;
                cx.env.push_block();
                let flow = exec_stmts(cx, body, false)?;
                pop_block_release_ram(cx);
                match flow {
                    StmtFlow::Return(v) => return Ok(StmtFlow::Return(v)),
                    StmtFlow::Break => return Ok(StmtFlow::Continue),
                    StmtFlow::Throw(v) => return Ok(StmtFlow::Throw(v)),
                    StmtFlow::Continue | StmtFlow::ContinueLoop => {}
                }
                let co = super::java_ops_budget::hir_java_loop_cond_outer_charge(cond);
                if co > 0 {
                    cx.charge_ops(co).map_err(ExecAbort::Error)?;
                }
                if !value_truthy(&eval_expr(cx, cond)?) {
                    break;
                }
            }
            Ok(StmtFlow::Continue)
        }
        HirStmt::Switch { discr, clauses } => {
            let dv = eval_expr(cx, discr)?;
            let mut start: Option<usize> = None;
            'arm_search: for (i, cl) in clauses.iter().enumerate() {
                if let HirSwitchClause::Case { labels, .. } = cl {
                    for lab in labels {
                        let v = eval_expr(cx, lab)?;
                        if values_equal_for_compare(&dv, &v) {
                            start = Some(i);
                            break 'arm_search;
                        }
                    }
                }
            }
            let start = start.or_else(|| {
                clauses
                    .iter()
                    .position(|c| matches!(c, HirSwitchClause::Default { .. }))
            });
            let Some(start) = start else {
                return Ok(StmtFlow::Continue);
            };
            for cl in &clauses[start..] {
                let body = match cl {
                    HirSwitchClause::Case { body, .. } | HirSwitchClause::Default { body } => body,
                };
                match exec_stmts(cx, body, false)? {
                    StmtFlow::Return(v) => return Ok(StmtFlow::Return(v)),
                    StmtFlow::Break => return Ok(StmtFlow::Continue),
                    StmtFlow::ContinueLoop => return Ok(StmtFlow::ContinueLoop),
                    StmtFlow::Throw(v) => return Ok(StmtFlow::Throw(v)),
                    StmtFlow::Continue => {}
                }
            }
            Ok(StmtFlow::Continue)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            cx.env.push_block();
            if let Some(init_stmt) = init {
                match exec_stmt(cx, init_stmt.as_ref(), record_script_tail)? {
                    StmtFlow::Continue => {}
                    f => {
                        pop_block_release_ram(cx);
                        return Ok(f);
                    }
                }
            }
            loop {
                let co = cond
                    .as_ref()
                    .map_or(0, super::java_ops_budget::hir_java_loop_cond_outer_charge);
                if co > 0 {
                    cx.charge_ops(co).map_err(ExecAbort::Error)?;
                }
                let cond_ok = match cond {
                    Some(c) => value_truthy(&eval_expr(cx, c)?),
                    None => true,
                };
                if !cond_ok {
                    break;
                }
                cx.charge_ops(1).map_err(ExecAbort::Error)?;
                cx.env.push_block();
                let flow = exec_stmts(cx, body, false)?;
                pop_block_release_ram(cx);
                match flow {
                    StmtFlow::Return(v) => {
                        pop_block_release_ram(cx);
                        return Ok(StmtFlow::Return(v));
                    }
                    StmtFlow::Break => {
                        pop_block_release_ram(cx);
                        return Ok(StmtFlow::Continue);
                    }
                    StmtFlow::Throw(v) => {
                        pop_block_release_ram(cx);
                        return Ok(StmtFlow::Throw(v));
                    }
                    StmtFlow::Continue | StmtFlow::ContinueLoop => {}
                }
                if let Some(u) = update.as_ref() {
                    let uo = super::java_ops_budget::hir_java_for_step_ops(u);
                    if uo > 0 {
                        cx.charge_ops(uo).map_err(ExecAbort::Error)?;
                    }
                    match u {
                        HirForStep::Assign(fu) => apply_ident_update(cx, fu)?,
                        HirForStep::Expr(e) => {
                            eval_expr(cx, e)?;
                        }
                    }
                }
            }
            pop_block_release_ram(cx);
            Ok(StmtFlow::Continue)
        }
        HirStmt::ForIn {
            name,
            is_declaration,
            name_by_ref,
            container,
            body,
        } => {
            let v = eval_expr(cx, container)?;
            cx.env.push_block();
            match v {
                Value::Array(arr) if *name_by_ref && *is_declaration => {
                    let len = arr.borrow().len();
                    for idx in 0..len {
                        let item = arr.borrow()[idx].clone();
                        cx.env.push_block();
                        cx.env.insert_maybe_array_cell(
                            name.name.clone(),
                            item,
                            Some((arr.clone(), idx)),
                        );
                        let flow = exec_stmts(cx, body, false)?;
                        pop_block_release_ram(cx);
                        match flow {
                            StmtFlow::Return(ret) => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Return(ret));
                            }
                            StmtFlow::Break => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Continue);
                            }
                            StmtFlow::Throw(v) => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Throw(v));
                            }
                            StmtFlow::ContinueLoop | StmtFlow::Continue => {}
                        }
                    }
                }
                _ => {
                    let items = value_to_for_in_sequence(v)?;
                    for item in items {
                        cx.env.push_block();
                        if *is_declaration {
                            cx.insert_local_var(name.name.clone(), item)?;
                        } else {
                            cx.assign_with_ram(&name.name, item)?;
                        }
                        let flow = exec_stmts(cx, body, false)?;
                        pop_block_release_ram(cx);
                        match flow {
                            StmtFlow::Return(ret) => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Return(ret));
                            }
                            StmtFlow::Break => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Continue);
                            }
                            StmtFlow::Throw(v) => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Throw(v));
                            }
                            StmtFlow::ContinueLoop | StmtFlow::Continue => {}
                        }
                    }
                }
            }
            pop_block_release_ram(cx);
            Ok(StmtFlow::Continue)
        }
        HirStmt::ForInKeyValue {
            key,
            key_is_declaration,
            key_by_ref: _,
            value,
            value_is_declaration,
            value_by_ref,
            container,
            body,
        } => {
            let v = eval_expr(cx, container)?;
            cx.env.push_block();
            match v {
                Value::Array(arr) if *value_by_ref => {
                    let len = arr.borrow().len();
                    for idx in 0..len {
                        let k = Value::Integer(idx as i64);
                        let val = arr.borrow()[idx].clone();
                        cx.env.push_block();
                        if *key_is_declaration {
                            cx.insert_local_var(key.name.clone(), k)?;
                        } else {
                            cx.assign_with_ram(&key.name, k)?;
                        }
                        if *value_is_declaration {
                            cx.env.insert_maybe_array_cell(
                                value.name.clone(),
                                val,
                                Some((arr.clone(), idx)),
                            );
                        } else {
                            cx.assign_with_ram(&value.name, val)?;
                        }
                        let flow = exec_stmts(cx, body, false)?;
                        pop_block_release_ram(cx);
                        match flow {
                            StmtFlow::Return(ret) => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Return(ret));
                            }
                            StmtFlow::Break => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Continue);
                            }
                            StmtFlow::Throw(v) => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Throw(v));
                            }
                            StmtFlow::ContinueLoop | StmtFlow::Continue => {}
                        }
                    }
                }
                _ => {
                    let pairs = value_to_for_in_key_value_pairs(v)?;
                    for (k, val) in pairs {
                        cx.env.push_block();
                        if *key_is_declaration {
                            cx.insert_local_var(key.name.clone(), k)?;
                        } else {
                            cx.assign_with_ram(&key.name, k)?;
                        }
                        if *value_is_declaration {
                            cx.insert_local_var(value.name.clone(), val)?;
                        } else {
                            cx.assign_with_ram(&value.name, val)?;
                        }
                        let flow = exec_stmts(cx, body, false)?;
                        pop_block_release_ram(cx);
                        match flow {
                            StmtFlow::Return(ret) => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Return(ret));
                            }
                            StmtFlow::Break => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Continue);
                            }
                            StmtFlow::Throw(v) => {
                                pop_block_release_ram(cx);
                                return Ok(StmtFlow::Throw(v));
                            }
                            StmtFlow::ContinueLoop | StmtFlow::Continue => {}
                        }
                    }
                }
            }
            pop_block_release_ram(cx);
            Ok(StmtFlow::Continue)
        }
    }
}

fn apply_ident_update(cx: &mut InterpCx, u: &HirForUpdate) -> Result<(), ExecAbort> {
    use super::lvalue::combine_assign_old_rhs;
    let rhs = eval_expr(cx, &u.value)?;
    let newv = if matches!(u.op, HirAssignOp::Assign) {
        rhs
    } else {
        let cur = cx
            .env
            .get(&u.name.name)
            .ok_or_else(|| InterpretError::variable_not_exists(&u.name.name))?;
        combine_assign_old_rhs(cx, u.op, cur, rhs)?
    };
    cx.assign_with_ram(&u.name.name, newv)?;
    Ok(())
}
