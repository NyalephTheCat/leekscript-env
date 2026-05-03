//! Expression evaluation.

use super::call::{eval_call, invoke_value, InvokeOptions};
use super::context::InterpCx;
use super::error::{ExecAbort, InterpretError};
use super::exec::exec_stmts;
use super::flow::StmtFlow;
use super::instance::{
    callable_accepts_arg_count, enforce_constructor_callable, enforce_instance_field_visibility,
    read_class_ref_instance_callable, read_instance_callable_member, read_instance_member,
    read_super_instance_member, read_visible_class_static_field, resolve_static_method_owner,
};
use super::java_export::{
    charge_java_ai_add_string_branch, value_java_string_coerce, ARRAY_CLASS_EXPORT_NATIVE,
    BOOLEAN_CLASS_EXPORT_NATIVE, CLASS_METACLASS_EXPORT_NATIVE, FUNCTION_CLASS_EXPORT_NATIVE,
    INTEGER_CLASS_EXPORT_NATIVE, INTERVAL_CLASS_EXPORT_NATIVE, NULL_CLASS_EXPORT_NATIVE,
    NUMBER_CLASS_EXPORT_NATIVE, OBJECT_CLASS_EXPORT_NATIVE, REAL_CLASS_EXPORT_NATIVE,
    STRING_CLASS_EXPORT_NATIVE,
};
use super::lvalue::{assign_place, instance_has_field_named};
use super::native::{number_from_value, runtime_typeof_value};
use super::ops::{
    eval_binary, eval_bitnot, eval_bitxor, eval_interval_endpoints, instanceof_leek_type,
};
use super::util::{
    array_index_for_read, eval_in, interval_array_values, map_find_key, map_find_key_legacy,
    neg_slice_end, neg_slice_start, pass_parameter_value, pos_slice_end, pos_slice_start,
    value_truthy,
};
use super::value::{InstanceData, IntervalValue, Value};
use leekscript_hir::{HirAssignOp, HirBinOp, HirExpr, HirStmt, HirTypeExpr, HirUnaryOp};
use std::cell::RefCell;
use std::rc::Rc;

pub(super) fn eval_expr(cx: &mut InterpCx, e: &HirExpr) -> Result<Value, ExecAbort> {
    match e {
        HirExpr::Integer(i) => Ok(Value::Integer(*i)),
        HirExpr::Real(r) => Ok(Value::Real(*r)),
        HirExpr::String(s) => Ok(Value::String(s.clone())),
        HirExpr::Bool(b) => Ok(Value::Bool(*b)),
        HirExpr::Null => Ok(Value::Null),
        HirExpr::This => cx
            .this_stack
            .last()
            .cloned()
            .ok_or_else(|| InterpretError::this_not_allowed_here().into()),
        HirExpr::ClassSelf { .. } => cx
            .enclosing_class_stack
            .last()
            .cloned()
            .map(Value::UserClass)
            .ok_or_else(|| InterpretError::class_self_not_allowed_here().into()),
        HirExpr::Ident { name, .. } => {
            if name == "super" && cx.language_version >= 2 {
                if let (Some(Value::Instance(_)), Some(cur_class)) =
                    (cx.this_stack.last(), cx.enclosing_class_stack.last())
                {
                    if cx
                        .classes
                        .get(cur_class.as_str())
                        .is_some_and(|d| d.extends.is_some())
                    {
                        return Ok(Value::Super);
                    }
                }
            }
            if cx.env.in_user_callable() {
                if let Some(v) = cx.env.get_callable_local(name) {
                    return Ok(v);
                }
                if let Some(Value::Instance(rc)) = cx.this_stack.last() {
                    if instance_has_field_named(cx, rc, name.as_str()) {
                        return read_instance_member(cx, rc, name.as_str()).map_err(Into::into);
                    }
                }
                if let Some(enc) = cx.enclosing_class_stack.last() {
                    if let Some(def) = cx.classes.get(enc.as_str()) {
                        // Static fields are visible by unqualified name inside static methods.
                        if let Some(s) = read_visible_class_static_field(cx, enc, name.as_str()) {
                            return Ok(s);
                        }
                        if let Some(m) = def
                            .static_methods
                            .get(name.as_str())
                            .and_then(|vs| vs.first())
                        {
                            return Ok(m.clone());
                        }
                    }
                }
                if let Some(v) = cx.env.get_callable_outer_lexical(name) {
                    return Ok(v);
                }
                return Err(InterpretError::variable_not_exists(name.as_str()).into());
            }
            if let Some(v) = cx.env.get(name) {
                return Ok(v.clone());
            }
            if let Some(Value::Instance(rc)) = cx.this_stack.last() {
                if instance_has_field_named(cx, rc, name.as_str()) {
                    return read_instance_member(cx, rc, name.as_str()).map_err(Into::into);
                }
            }
            Err(InterpretError::variable_not_exists(name.as_str()).into())
        }
        HirExpr::RefTo { expr, .. } => {
            let v = eval_expr(cx, expr)?;
            Ok(pass_parameter_value(cx.language_version, v, true))
        }
        HirExpr::Unary { op, expr } => eval_unary(*op, cx, expr),
        HirExpr::Binary { op, left, right } => match op {
            HirBinOp::LogicalAnd | HirBinOp::LogicalOr => {
                eval_logical_short_circuit(*op, cx, left, right)
            }
            HirBinOp::NullishCoalesce => {
                let l = eval_expr(cx, left)?;
                if matches!(l, Value::Null) {
                    eval_expr(cx, right)
                } else {
                    Ok(l)
                }
            }
            HirBinOp::BitXor => {
                let l = eval_expr(cx, left)?;
                let r = eval_expr(cx, right)?;
                eval_bitxor(l, r).map_err(Into::into)
            }
            HirBinOp::Instanceof => eval_instanceof(cx, left, right),
            HirBinOp::In => {
                let l = eval_expr(cx, left)?;
                let r = eval_expr(cx, right)?;
                if matches!(&r, Value::Interval(_)) {
                    cx.charge_ops(1).map_err(ExecAbort::Error)?;
                }
                eval_in(l, r).map_err(Into::into)
            }
            HirBinOp::Add => {
                let l = eval_expr(cx, left)?;
                let r = eval_expr(cx, right)?;
                if matches!(&l, Value::String(_)) || matches!(&r, Value::String(_)) {
                    charge_java_ai_add_string_branch(cx, &l, &r, cx.language_version)
                        .map_err(ExecAbort::Error)?;
                }
                super::ops::eval_add(l, r, cx.language_version).map_err(Into::into)
            }
            _ => {
                let l = eval_expr(cx, left)?;
                let r = eval_expr(cx, right)?;
                eval_binary(*op, l, r, cx.language_version).map_err(Into::into)
            }
        },
        HirExpr::Cast { expr, ty, .. } => {
            let v = eval_expr(cx, expr)?;
            eval_cast(&v, ty, cx.language_version).map_err(Into::into)
        }
        HirExpr::ArrayLiteral { elements, span: _ } => {
            let mut out = Vec::with_capacity(elements.len());
            for e in elements {
                out.push(eval_expr(cx, e)?);
            }
            Ok(Value::array_from(out))
        }
        HirExpr::MapLiteral { entries, span: _ } => eval_keyed_literal(cx, entries, false),
        HirExpr::ObjectLiteral { entries, span: _ } => eval_keyed_literal(cx, entries, true),
        HirExpr::New {
            type_name,
            args,
            span: _,
        } => eval_new(cx, type_name, args),
        HirExpr::Call { callee, args, span } => {
            let prev = cx.pending_call_span.replace(*span);
            let out = eval_call(cx, callee, args);
            cx.pending_call_span = prev;
            out
        }
        HirExpr::Index { base, index, .. } => {
            let container = eval_expr(cx, base)?;
            let key = eval_expr(cx, index)?;
            eval_index_read(cx, &container, &key, cx.language_version)
        }
        HirExpr::ArraySlice {
            base,
            start,
            end,
            step,
            span: _,
        } => {
            let base_v = eval_expr(cx, base)?;
            if let Value::Interval(iv) = base_v {
                return eval_interval_slice(cx, iv, start, end, step);
            }
            let Value::Array(arr) = base_v else {
                return Err(InterpretError::not_indexable().into());
            };
            let len = arr.borrow().len();
            // JVM `ArrayLeekValue.arraySlice`: `stride == 0` is treated as `1`.
            let step_n: i64 = match step {
                None => 1,
                Some(e) => match eval_expr(cx, e)? {
                    Value::Integer(n) => {
                        if n == 0 {
                            1
                        } else {
                            n
                        }
                    }
                    _ => {
                        return Err(InterpretError {
                            reference: "WRONG_ARGUMENT_TYPE",
                            message: "array slice step must be an integer".into(),
                        }
                        .into());
                    }
                },
            };

            if step_n < 0 {
                if len == 0 {
                    return Ok(Value::array_from(vec![]));
                }
                let s_val = match start {
                    Some(e) => Some(eval_expr(cx, e)?),
                    None => None,
                };
                let e_val = match end {
                    Some(e) => Some(eval_expr(cx, e)?),
                    None => None,
                };
                let start_idx = neg_slice_start(s_val.as_ref(), len)?;
                let end_bound = neg_slice_end(e_val.as_ref(), len)?;
                if start_idx <= end_bound {
                    return Ok(Value::array_from(vec![]));
                }
                let b = arr.borrow();
                let mut out = Vec::new();
                let mut i = start_idx;
                while i > end_bound {
                    out.push(b[i as usize].clone());
                    i += step_n;
                }
                return Ok(Value::array_from(out));
            }

            let s_val = match start {
                Some(e) => Some(eval_expr(cx, e)?),
                None => None,
            };
            let e_val = match end {
                Some(e) => Some(eval_expr(cx, e)?),
                None => None,
            };
            let start_idx = pos_slice_start(s_val.as_ref(), len)?;
            let end_bound = pos_slice_end(e_val.as_ref(), len)?;
            // Match JVM `for (i = start; i < end; …)` (e.g. `end` may stay negative after `+len`).
            if start_idx >= end_bound {
                return Ok(Value::array_from(vec![]));
            }
            let b = arr.borrow();
            if step_n == 1 {
                return Ok(Value::array_from(
                    b[start_idx as usize..end_bound as usize].to_vec(),
                ));
            }
            let mut out = Vec::new();
            let mut i = start_idx;
            while i < end_bound {
                out.push(b[i as usize].clone());
                i += step_n;
            }
            Ok(Value::array_from(out))
        }
        HirExpr::Member { base, field, .. } => eval_member_read(cx, base, field),
        HirExpr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            if value_truthy(&eval_expr(cx, cond)?) {
                eval_expr(cx, then_expr)
            } else {
                eval_expr(cx, else_expr)
            }
        }
        HirExpr::FunctionLiteral { params, body, .. } => {
            let pnames: Vec<String> = params.iter().map(|p| p.name.name.clone()).collect();
            let pref: Vec<bool> = params.iter().map(|p| p.by_ref).collect();
            let pty: Vec<Option<String>> = params.iter().map(|p| p.decl_ty.clone()).collect();
            let pdef: Vec<Option<HirExpr>> = params.iter().map(|p| p.default.clone()).collect();
            Ok(Value::Function(std::rc::Rc::new(
                super::value::FunctionValue {
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
                    declared_return_ty: None,
                    unbound_method_ref: false,
                },
            )))
        }
        HirExpr::ArrowClosure { params, body, .. } => {
            let pnames: Vec<String> = params.iter().map(|p| p.name.name.clone()).collect();
            let pref: Vec<bool> = params.iter().map(|p| p.by_ref).collect();
            let pty: Vec<Option<String>> = params.iter().map(|p| p.decl_ty.clone()).collect();
            let pdef: Vec<Option<HirExpr>> = params.iter().map(|p| p.default.clone()).collect();
            Ok(Value::Function(std::rc::Rc::new(
                super::value::FunctionValue {
                    params: pnames,
                    param_by_ref: pref,
                    param_decl_tys: pty,
                    param_defaults: pdef,
                    body: vec![HirStmt::ret(Some((**body).clone()))],
                    captured_locals: cx
                        .env
                        .in_user_callable()
                        .then(|| cx.env.snapshot_callable_visible_non_global()),
                    captured_aliases: cx
                        .env
                        .in_user_callable()
                        .then(|| cx.env.snapshot_callable_aliases_non_global()),
                    declared_return_ty: None,
                    unbound_method_ref: false,
                },
            )))
        }
        HirExpr::AssignExpr {
            place, op, value, ..
        } => Ok(assign_place(cx, place.as_ref(), *op, value.as_ref())?),
        HirExpr::PostUpdate {
            target,
            increment,
            span: _,
        } => {
            let old = eval_expr(cx, target.as_ref())?;
            let op = if *increment {
                HirAssignOp::AddAssign
            } else {
                HirAssignOp::SubAssign
            };
            assign_place(cx, target.as_ref(), op, &HirExpr::Integer(1))?;
            Ok(old)
        }
        HirExpr::PreUpdate {
            target,
            increment,
            span: _,
        } => {
            let op = if *increment {
                HirAssignOp::AddAssign
            } else {
                HirAssignOp::SubAssign
            };
            assign_place(cx, target.as_ref(), op, &HirExpr::Integer(1))?;
            eval_expr(cx, target.as_ref())
        }
    }
}

fn eval_unary(op: HirUnaryOp, cx: &mut InterpCx, expr: &HirExpr) -> Result<Value, ExecAbort> {
    use HirUnaryOp::{BitNot, Neg, Not, Typeof};
    match op {
        Neg => {
            let v = eval_expr(cx, expr)?;
            match v {
                Value::Integer(n) => Ok(Value::Integer(n.wrapping_neg())),
                Value::Real(n) => Ok(Value::Real(-n)),
                Value::Bool(b) => Ok(Value::Integer(if b { -1 } else { 0 })),
                Value::Null => Ok(Value::Integer(0)),
                _ => Err(InterpretError::wrong_unary_operand().into()),
            }
        }
        Not => Ok(Value::Bool(!value_truthy(&eval_expr(cx, expr)?))),
        BitNot => eval_bitnot(eval_expr(cx, expr)?).map_err(Into::into),
        Typeof => Ok(runtime_typeof_value(&eval_expr(cx, expr)?)),
    }
}

fn eval_hir_logical_and(cx: &mut InterpCx, e: &HirExpr) -> Result<bool, ExecAbort> {
    let HirExpr::Binary {
        op: HirBinOp::LogicalAnd,
        left,
        right,
    } = e
    else {
        let w = super::java_ops_budget::hir_java_expr_ops_budget(e).saturating_add(1);
        if w > 0 {
            cx.charge_ops(w).map_err(ExecAbort::Error)?;
        }
        return Ok(value_truthy(&eval_expr(cx, e)?));
    };
    eval_and_components(cx, left, right)
}

fn eval_and_components(
    cx: &mut InterpCx,
    left: &HirExpr,
    right: &HirExpr,
) -> Result<bool, ExecAbort> {
    let w1 = super::java_ops_budget::hir_java_expr_ops_budget(left).saturating_add(1);
    if w1 > 0 {
        cx.charge_ops(w1).map_err(ExecAbort::Error)?;
    }
    let ltruthy = match left {
        HirExpr::Binary {
            op: HirBinOp::LogicalAnd,
            ..
        } => eval_hir_logical_and(cx, left)?,
        HirExpr::Binary {
            op: HirBinOp::LogicalOr,
            ..
        } => eval_hir_logical_or(cx, left)?,
        _ => value_truthy(&eval_expr(cx, left)?),
    };
    if !ltruthy {
        return Ok(false);
    }
    let w2 = super::java_ops_budget::hir_java_expr_ops_budget(right);
    if w2 > 0 {
        cx.charge_ops(w2).map_err(ExecAbort::Error)?;
    }
    let rtruthy = match right {
        HirExpr::Binary {
            op: HirBinOp::LogicalAnd,
            ..
        } => eval_hir_logical_and(cx, right)?,
        HirExpr::Binary {
            op: HirBinOp::LogicalOr,
            ..
        } => eval_hir_logical_or(cx, right)?,
        _ => value_truthy(&eval_expr(cx, right)?),
    };
    Ok(rtruthy)
}

fn eval_hir_logical_or(cx: &mut InterpCx, e: &HirExpr) -> Result<bool, ExecAbort> {
    let HirExpr::Binary {
        op: HirBinOp::LogicalOr,
        left,
        right,
    } = e
    else {
        let w = super::java_ops_budget::hir_java_expr_ops_budget(e).saturating_add(1);
        if w > 0 {
            cx.charge_ops(w).map_err(ExecAbort::Error)?;
        }
        return Ok(value_truthy(&eval_expr(cx, e)?));
    };
    eval_or_components(cx, left, right)
}

fn eval_or_components(
    cx: &mut InterpCx,
    left: &HirExpr,
    right: &HirExpr,
) -> Result<bool, ExecAbort> {
    // Left-associated `a or b or c` is `((a or b) or c)`: the outer `or` must not add a second
    // `+1` wrapper on top of the inner `LeekExpression` OR node (`operations` is already 0 there).
    if !matches!(
        left,
        HirExpr::Binary {
            op: HirBinOp::LogicalOr,
            ..
        }
    ) {
        let w1 = super::java_ops_budget::hir_java_expr_ops_budget(left).saturating_add(1);
        if w1 > 0 {
            cx.charge_ops(w1).map_err(ExecAbort::Error)?;
        }
    }
    let ltruthy = match left {
        HirExpr::Binary {
            op: HirBinOp::LogicalOr,
            ..
        } => eval_hir_logical_or(cx, left)?,
        HirExpr::Binary {
            op: HirBinOp::LogicalAnd,
            ..
        } => eval_hir_logical_and(cx, left)?,
        _ => value_truthy(&eval_expr(cx, left)?),
    };
    if ltruthy {
        return Ok(true);
    }
    let w2 = super::java_ops_budget::hir_java_expr_ops_budget(right);
    if w2 > 0 {
        cx.charge_ops(w2).map_err(ExecAbort::Error)?;
    }
    let rtruthy = match right {
        HirExpr::Binary {
            op: HirBinOp::LogicalOr,
            ..
        } => eval_hir_logical_or(cx, right)?,
        HirExpr::Binary {
            op: HirBinOp::LogicalAnd,
            ..
        } => eval_hir_logical_and(cx, right)?,
        _ => value_truthy(&eval_expr(cx, right)?),
    };
    Ok(rtruthy)
}

fn eval_logical_short_circuit(
    op: HirBinOp,
    cx: &mut InterpCx,
    left: &HirExpr,
    right: &HirExpr,
) -> Result<Value, ExecAbort> {
    use HirBinOp::{LogicalAnd, LogicalOr};
    match op {
        LogicalAnd => eval_and_components(cx, left, right).map(Value::Bool),
        LogicalOr => eval_or_components(cx, left, right).map(Value::Bool),
        _ => unreachable!("eval_logical_short_circuit only handles && and ||"),
    }
}

fn eval_index_read(
    cx: &InterpCx,
    container: &Value,
    key: &Value,
    language_version: u8,
) -> Result<Value, ExecAbort> {
    match container {
        Value::Null => Ok(Value::Null),
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) => Ok(Value::Null),
        Value::Array(arr) => {
            let b = arr.borrow();
            let Some(i) = array_index_for_read(key, language_version, b.as_slice()) else {
                return Ok(Value::Null);
            };
            Ok(b[i].clone())
        }
        Value::Map(m) | Value::Object(m) => {
            let b = m.borrow();
            let p = if language_version < 4 {
                map_find_key_legacy(&b, key)
            } else {
                map_find_key(&b, key)
            };
            if let Some(p) = p {
                Ok(b[p].1.clone())
            } else {
                Ok(Value::Null)
            }
        }
        Value::Instance(rc) => {
            let b = rc.borrow();
            if b.extends.as_deref() == Some("Array") {
                if let Some(arr) = &b.array_backing {
                    let bb = arr.borrow();
                    let Some(i) = array_index_for_read(key, language_version, bb.as_slice()) else {
                        return Ok(Value::Null);
                    };
                    return Ok(bb[i].clone());
                }
            }
            drop(b);
            if let Value::String(f) = key {
                enforce_instance_field_visibility(cx, rc, f.as_str())?;
                return read_instance_callable_member(cx, rc, f.as_str()).map_err(Into::into);
            }
            Err(InterpretError::not_indexable().into())
        }
        Value::UserClass(class_name) => {
            let Value::String(f) = key else {
                return Err(InterpretError::not_indexable().into());
            };
            if !cx.classes.contains_key(class_name) {
                return Ok(Value::Null);
            }
            if let Some(v) = read_visible_class_static_field(cx, class_name.as_str(), f.as_str()) {
                return Ok(v);
            }
            // `A['m']` can also reference a static method like `A.m`.
            if let Some(owner) = resolve_static_method_owner(cx, class_name.as_str(), f.as_str()) {
                if let Some(odef) = cx.classes.get(&owner) {
                    if let Some(v) = odef
                        .static_methods
                        .get(f.as_str())
                        .and_then(|vs| vs.first())
                    {
                        return Ok(v.clone());
                    }
                }
            }
            Ok(Value::Null)
        }
        _ => Err(InterpretError::not_indexable().into()),
    }
}

fn eval_keyed_literal(
    cx: &mut InterpCx,
    entries: &[(HirExpr, HirExpr)],
    object_literal: bool,
) -> Result<Value, ExecAbort> {
    if entries.is_empty() && !object_literal && cx.language_version <= 3 {
        return Ok(Value::array_from(Vec::new()));
    }
    let mut m = super::map_store::MapStore::new();
    for (ke, ve) in entries {
        let k = eval_expr(cx, ke)?;
        let v = eval_expr(cx, ve)?;
        // Match Leek Wars JVM: later entry wins for v1–3 map literals, `{}` object literals, and
        // duplicate keys in object literals at any version. Map literals `[…]` at language v4+
        // reject duplicate keys (`MAP_DUPLICATED_KEY`).
        if let Some(j) = m.find_key(&k) {
            if !object_literal && cx.language_version >= 4 {
                return Err(InterpretError::map_duplicated_key().into());
            }
            m[j].1 = v;
        } else {
            m.push_kv(k, v);
        }
    }
    Ok(if object_literal {
        Value::object_from_store(m)
    } else {
        Value::map_from_store(m)
    })
}

/// JVM doubles explicit start/end bounds before applying normal `arraySlice` indices on the
/// unit-step materialization of a bounded interval.
fn interval_slice_java_doubled_index(v: &Value) -> Value {
    match v {
        Value::Integer(n) => Value::Integer(*n * 2),
        Value::Real(r) if r.is_finite() && r.fract() == 0.0 => Value::Integer(*r as i64 * 2),
        Value::Real(r) => Value::Real(*r * 2.0),
        _ => v.clone(),
    }
}

/// Negative `arraySlice` stride on an interval uses `2*start+1` / `2*end - len/2` (JVM), not `2*…` like `stride > 0`.
fn interval_slice_java_neg_doubled_start(v: &Value) -> Value {
    match v {
        Value::Integer(n) => Value::Integer(*n * 2 + 1),
        Value::Real(r) if r.is_finite() && r.fract() == 0.0 => Value::Integer(*r as i64 * 2 + 1),
        Value::Real(r) => Value::Real(*r * 2.0 + 1.0),
        _ => v.clone(),
    }
}

fn interval_slice_java_neg_doubled_end(v: &Value, len: usize) -> Value {
    let half = (len as i64) / 2;
    match v {
        Value::Integer(n) => Value::Integer(*n * 2 - half - 1),
        Value::Real(r) if r.is_finite() && r.fract() == 0.0 => {
            Value::Integer(*r as i64 * 2 - half - 1)
        }
        Value::Real(r) => Value::Real(*r * 2.0 - half as f64 - 1.0),
        _ => v.clone(),
    }
}

fn coerce_interval_slice_for_export(iv: &IntervalValue, ver: u8, v: Value) -> Value {
    if ver < 4 || !iv.integer_lattice {
        return v;
    }
    let Value::Array(a) = v else {
        return v;
    };
    let next: Vec<Value> = a
        .borrow()
        .iter()
        .map(|x| match x {
            Value::Integer(n) => Value::Real(*n as f64),
            other => other.clone(),
        })
        .collect();
    Value::array_from(next)
}

fn eval_interval_slice(
    cx: &mut InterpCx,
    iv: IntervalValue,
    start: &Option<Box<HirExpr>>,
    end: &Option<Box<HirExpr>>,
    step: &Option<Box<HirExpr>>,
) -> Result<Value, ExecAbort> {
    let ver = cx.language_version;
    let eval_step_f = |cx: &mut InterpCx, e: &HirExpr| -> Result<f64, ExecAbort> {
        match eval_expr(cx, e)? {
            Value::Integer(n) => {
                if n == 0 {
                    Ok(1.0)
                } else {
                    Ok(n as f64)
                }
            }
            Value::Real(r) => {
                if r == 0.0 {
                    Err(InterpretError {
                        reference: "WRONG_ARGUMENT_TYPE",
                        message: "array slice step must be non-zero".into(),
                    }
                    .into())
                } else {
                    Ok(r)
                }
            }
            _ => Err(InterpretError {
                reference: "WRONG_ARGUMENT_TYPE",
                message: "array slice step must be numeric".into(),
            }
            .into()),
        }
    };

    let step_f: f64 = match step {
        None => 1.0,
        Some(e) => eval_step_f(cx, e.as_ref())?,
    };

    if start.is_none() && end.is_none() {
        let vals = match interval_array_values(
            &iv,
            if (step_f - 1.0).abs() < f64::EPSILON {
                None
            } else {
                Some(step_f)
            },
        )? {
            None => return Ok(Value::Null),
            Some(v) => v,
        };
        return Ok(coerce_interval_slice_for_export(
            &iv,
            ver,
            Value::array_from(vals),
        ));
    }

    let Some(vals) = interval_array_values(&iv, None)? else {
        return Ok(Value::Null);
    };
    let len = vals.len();
    let arr = Value::array_from(vals);
    let Value::Array(arr_rc) = arr else {
        unreachable!("array_from always returns Array");
    };

    if !step_f.is_finite() || step_f.fract() != 0.0 {
        return Err(InterpretError {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "array slice step must be integral for bounded interval slice".into(),
        }
        .into());
    }
    let step_n = step_f as i64;
    if step_n == 0 {
        return Err(InterpretError {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "array slice step must be non-zero".into(),
        }
        .into());
    }

    let start_v = start
        .as_ref()
        .map(|e| eval_expr(cx, e.as_ref()))
        .transpose()?;
    let end_v = end
        .as_ref()
        .map(|e| eval_expr(cx, e.as_ref()))
        .transpose()?;
    let double_bounds = start_v.is_some() && end_v.is_some();
    let start_for_slice: Option<Value> = match (&start_v, double_bounds, step_n < 0) {
        (Some(v), true, true) => Some(interval_slice_java_neg_doubled_start(v)),
        (Some(v), true, false) => Some(interval_slice_java_doubled_index(v)),
        (Some(v), false, _) => Some(v.clone()),
        (None, _, _) => None,
    };
    let end_for_slice: Option<Value> = match (&end_v, double_bounds, step_n < 0) {
        (Some(v), true, true) => Some(interval_slice_java_neg_doubled_end(v, len)),
        (Some(v), true, false) => Some(interval_slice_java_doubled_index(v)),
        (Some(v), false, _) => Some(v.clone()),
        (None, _, _) => None,
    };

    if step_n < 0 {
        if len == 0 {
            return Ok(coerce_interval_slice_for_export(
                &iv,
                ver,
                Value::array_from(vec![]),
            ));
        }
        let start_idx = neg_slice_start(start_for_slice.as_ref(), len)?;
        let end_bound = neg_slice_end(end_for_slice.as_ref(), len)?;
        if start_idx <= end_bound {
            return Ok(coerce_interval_slice_for_export(
                &iv,
                ver,
                Value::array_from(vec![]),
            ));
        }
        let b = arr_rc.borrow();
        let mut out = Vec::new();
        let mut i = start_idx;
        while i > end_bound {
            out.push(b[i as usize].clone());
            i += step_n;
        }
        return Ok(coerce_interval_slice_for_export(
            &iv,
            ver,
            Value::array_from(out),
        ));
    }

    let start_idx = pos_slice_start(start_for_slice.as_ref(), len)?;
    let end_bound = pos_slice_end(end_for_slice.as_ref(), len)?;
    if start_idx >= end_bound {
        return Ok(coerce_interval_slice_for_export(
            &iv,
            ver,
            Value::array_from(vec![]),
        ));
    }
    let b = arr_rc.borrow();
    if step_n == 1 {
        return Ok(coerce_interval_slice_for_export(
            &iv,
            ver,
            Value::array_from(b[start_idx as usize..end_bound as usize].to_vec()),
        ));
    }
    let mut out = Vec::new();
    let mut i = start_idx;
    while i < end_bound {
        out.push(b[i as usize].clone());
        i += step_n;
    }
    Ok(coerce_interval_slice_for_export(
        &iv,
        ver,
        Value::array_from(out),
    ))
}

fn eval_member_read(cx: &mut InterpCx, base: &HirExpr, field: &str) -> Result<Value, ExecAbort> {
    let base_v = eval_expr(cx, base)?;
    match base_v {
        Value::Interval(_) if field == "class" => Ok(Value::Native(INTERVAL_CLASS_EXPORT_NATIVE)),
        Value::Integer(_) if field == "class" && cx.language_version >= 2 => {
            Ok(Value::Native(INTEGER_CLASS_EXPORT_NATIVE))
        }
        Value::Real(_) if field == "class" && cx.language_version >= 2 => {
            Ok(Value::Native(REAL_CLASS_EXPORT_NATIVE))
        }
        Value::Bool(_) if field == "class" && cx.language_version >= 2 => {
            Ok(Value::Native(BOOLEAN_CLASS_EXPORT_NATIVE))
        }
        Value::Null if field == "class" && cx.language_version >= 2 => {
            Ok(Value::Native(NULL_CLASS_EXPORT_NATIVE))
        }
        Value::String(_) if field == "class" && cx.language_version >= 2 => {
            Ok(Value::Native(STRING_CLASS_EXPORT_NATIVE))
        }
        Value::Array(_) if field == "class" && cx.language_version >= 2 => {
            Ok(Value::Native(ARRAY_CLASS_EXPORT_NATIVE))
        }
        Value::Object(_) if field == "class" && cx.language_version >= 2 => {
            Ok(Value::Native(OBJECT_CLASS_EXPORT_NATIVE))
        }
        Value::Function(_) | Value::Native(_) if field == "class" && cx.language_version >= 2 => {
            Ok(Value::Native(FUNCTION_CLASS_EXPORT_NATIVE))
        }
        Value::Native(n) if n == INTEGER_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Integer".into())),
            "MIN_VALUE" => Ok(Value::Integer(i64::MIN)),
            "MAX_VALUE" => Ok(Value::Integer(i64::MAX)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == REAL_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Real".into())),
            // Java `Double.MIN_VALUE`: smallest positive subnormal, not `f64::MIN_POSITIVE`.
            "MIN_VALUE" => Ok(Value::Real(f64::from_bits(1))),
            "MAX_VALUE" => Ok(Value::Real(f64::MAX)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == NUMBER_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Number".into())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == NULL_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Null".into())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == STRING_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("String".into())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == BOOLEAN_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Boolean".into())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == OBJECT_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Object".into())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == FUNCTION_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Function".into())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == INTERVAL_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Interval".into())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == ARRAY_CLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Array".into())),
            "fields" => Ok(Value::array_from(Vec::new())),
            "methods" => Ok(Value::array_from(Vec::new())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            _ => Ok(Value::Null),
        },
        Value::Native(n) if n == CLASS_METACLASS_EXPORT_NATIVE => match field {
            "name" => Ok(Value::String("Class".into())),
            "super" => Ok(Value::UserClass("Value".into())),
            "class" if cx.language_version >= 2 => Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE)),
            _ => Ok(Value::Null),
        },
        Value::Super => {
            let Some(Value::Instance(rc)) = cx.this_stack.last() else {
                return Err(InterpretError::super_not_available_parent().into());
            };
            let Some(enc) = cx.enclosing_class_stack.last() else {
                return Err(InterpretError::super_not_available_parent().into());
            };
            read_super_instance_member(cx, rc, field, enc.as_str())
                .map(|(v, _)| v)
                .map_err(Into::into)
        }
        Value::UserClass(class_name) => {
            let Some(def) = cx.classes.get(&class_name) else {
                return Ok(Value::Null);
            };
            if field == "name" && cx.language_version >= 2 {
                return Ok(Value::String(class_name));
            }
            if field == "class" && cx.language_version >= 2 {
                return Ok(Value::Native(CLASS_METACLASS_EXPORT_NATIVE));
            }
            if field == "super" {
                return Ok(def
                    .extends
                    .as_ref()
                    .map_or(Value::Null, |p| Value::UserClass(p.clone())));
            }
            if field == "fields" {
                let names: Vec<Value> = def
                    .instance_fields
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect();
                return Ok(Value::array_from(names));
            }
            if field == "methods" {
                use std::collections::HashSet;
                let mut seen = HashSet::<String>::new();
                let mut out = Vec::<String>::new();
                let mut cursor: Option<String> = Some(class_name.clone());
                for _ in 0..64 {
                    let Some(cn) = cursor else { break };
                    if !seen.insert(cn.clone()) {
                        break;
                    }
                    let Some(cdef) = cx.classes.get(&cn) else {
                        break;
                    };
                    for (k, vs) in &cdef.methods {
                        if k.as_str() == "constructor" {
                            continue;
                        }
                        for _ in 0..vs.len() {
                            out.push(k.clone());
                        }
                    }
                    cursor = cdef.extends.clone();
                }
                out.sort();
                return Ok(Value::array_from(
                    out.into_iter().map(Value::String).collect(),
                ));
            }
            if field == "staticMethods" {
                use std::collections::HashSet;
                let mut seen = HashSet::<String>::new();
                let mut out = Vec::<String>::new();
                let mut cursor: Option<String> = Some(class_name.clone());
                for _ in 0..64 {
                    let Some(cn) = cursor else { break };
                    if !seen.insert(cn.clone()) {
                        break;
                    }
                    let Some(cdef) = cx.classes.get(&cn) else {
                        break;
                    };
                    for (k, vs) in &cdef.static_methods {
                        for _ in 0..vs.len() {
                            out.push(k.clone());
                        }
                    }
                    cursor = cdef.extends.clone();
                }
                out.sort();
                return Ok(Value::array_from(
                    out.into_iter().map(Value::String).collect(),
                ));
            }
            if field == "staticFields" {
                let names: Vec<Value> = def
                    .static_field_order
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect();
                return Ok(Value::array_from(names));
            }
            if let Some(v) = read_visible_class_static_field(cx, class_name.as_str(), field) {
                return Ok(v);
            }
            if let Some(owner) = resolve_static_method_owner(cx, class_name.as_str(), field) {
                if let Some(odef) = cx.classes.get(&owner) {
                    if let Some(v) = odef.static_methods.get(field).and_then(|vs| vs.first()) {
                        return Ok(v.clone());
                    }
                }
            }
            if let Some(v) = read_class_ref_instance_callable(cx, class_name.as_str(), field) {
                // Tag unbound method refs (`A.m`) so calls can treat the first argument as `this`.
                if let Value::Function(f) = &v {
                    let mut fv = (*f.as_ref()).clone();
                    fv.unbound_method_ref = true;
                    return Ok(Value::Function(std::rc::Rc::new(fv)));
                }
                return Ok(v);
            }
            if cx.language_version >= 2 {
                return Err(InterpretError::class_static_member_does_not_exist(
                    class_name.as_str(),
                    field,
                )
                .into());
            }
            Ok(Value::Null)
        }
        Value::Instance(rc) => {
            enforce_instance_field_visibility(cx, &rc, field)?;
            let v = read_instance_callable_member(cx, &rc, field)?;
            if cx.language_version >= 2
                && cx.strict == Some(true)
                && matches!(v, Value::Null)
                && field != "class"
            {
                let cn = rc.borrow().class_name.clone();
                return Err(InterpretError::class_member_does_not_exist(cn.as_str(), field).into());
            }
            Ok(v)
        }
        Value::Map(m) | Value::Object(m) => {
            let key = Value::String(field.to_string());
            let b = m.borrow();
            if let Some(p) = map_find_key(&b, &key) {
                Ok(b[p].1.clone())
            } else {
                Ok(Value::Null)
            }
        }
        _ => Err(InterpretError::member_requires_instance().into()),
    }
}

/// Run a user class constructor body on an existing instance (`new` / `super()`).
pub(super) fn run_user_class_constructor_with_arg_values(
    cx: &mut InterpCx,
    type_name: &str,
    _rc: &Rc<RefCell<InstanceData>>,
    inst_val: Value,
    arg_vals: Vec<Value>,
) -> Result<(), ExecAbort> {
    enforce_constructor_callable(cx, type_name).map_err(ExecAbort::from)?;
    let ctor = cx.classes.get(type_name).and_then(|class_def| {
        class_def
            .methods
            .get("constructor")
            .and_then(|vs| vs.first())
            .cloned()
            .or_else(|| {
                class_def
                    .methods
                    .get(type_name)
                    .and_then(|vs| vs.first())
                    .cloned()
            })
    });
    if let Some(Value::Function(f)) = ctor {
        let params = &f.params;
        let param_by_ref = &f.param_by_ref;
        let param_defaults = &f.param_defaults;
        let body = &f.body;
        if arg_vals.len() > params.len() {
            return Err(
                InterpretError::invalid_parameter_count(params.len(), arg_vals.len()).into(),
            );
        }
        cx.this_stack.push(inst_val.clone());
        cx.enclosing_class_stack.push(type_name.to_string());
        cx.final_field_assign_stack.push(true);
        cx.env.begin_callable_frame();
        let body_result = (|| -> Result<StmtFlow, ExecAbort> {
            for i in 0..params.len() {
                let by_ref = i < param_by_ref.len() && param_by_ref[i];
                let v = if i < arg_vals.len() {
                    super::util::pass_parameter_value(
                        cx.language_version,
                        arg_vals[i].clone(),
                        by_ref,
                    )
                } else {
                    match param_defaults.get(i).and_then(|x| x.as_ref()) {
                        Some(expr) => eval_expr(cx, expr)?,
                        None => {
                            if cx.language_version >= 2 {
                                Value::Null
                            } else {
                                return Err(InterpretError::invalid_parameter_count(
                                    params.len(),
                                    arg_vals.len(),
                                )
                                .into());
                            }
                        }
                    }
                };
                cx.env.insert(params[i].clone(), v);
            }
            exec_stmts(cx, body, false).map_err(Into::into)
        })();
        cx.env.end_callable_frame();
        cx.this_stack.pop();
        cx.enclosing_class_stack.pop();
        cx.final_field_assign_stack.pop();
        let flow = body_result?;
        match flow {
            StmtFlow::Continue | StmtFlow::Return(_) => Ok(()),
            StmtFlow::Throw(v) => Err(ExecAbort::Throw(v)),
            StmtFlow::Break => Err(InterpretError::break_out_of_loop().into()),
            StmtFlow::ContinueLoop => Err(InterpretError::continue_out_of_loop().into()),
        }
    } else if !arg_vals.is_empty() {
        let Some(class_def) = cx.classes.get(type_name) else {
            return Ok(());
        };
        let mut candidates: Vec<Value> = Vec::new();
        for (mn, vs) in &class_def.methods {
            if mn == "constructor" {
                continue;
            }
            for f in vs {
                if callable_accepts_arg_count(f, arg_vals.len()) {
                    candidates.push(f.clone());
                }
            }
        }
        if candidates.len() == 1 {
            invoke_value(
                cx,
                Some(inst_val.clone()),
                Some(type_name),
                candidates.pop().expect("one candidate"),
                arg_vals,
                InvokeOptions::strict(),
            )?;
            Ok(())
        } else if candidates.is_empty() {
            Err(InterpretError::invalid_constructor(type_name, "no matching constructor").into())
        } else {
            Err(InterpretError::invalid_constructor(
                type_name,
                "ambiguous arguments for implicit instance initializer",
            )
            .into())
        }
    } else {
        Ok(())
    }
}

pub(super) fn run_user_class_constructor_if_any(
    cx: &mut InterpCx,
    type_name: &str,
    rc: &Rc<RefCell<InstanceData>>,
    inst_val: Value,
    args: &[HirExpr],
) -> Result<(), ExecAbort> {
    let mut arg_vals = Vec::with_capacity(args.len());
    for a in args {
        arg_vals.push(eval_expr(cx, a)?);
    }
    run_user_class_constructor_with_arg_values(cx, type_name, rc, inst_val, arg_vals)
}

/// `super(...)` — invokes the immediate superclass constructor on `this`.
pub(super) fn eval_super_constructor(
    cx: &mut InterpCx,
    args: &[HirExpr],
) -> Result<Value, ExecAbort> {
    if cx.language_version < 2 {
        return Err(InterpretError::variable_not_exists("super").into());
    }
    let Some(child_class) = cx.enclosing_class_stack.last() else {
        return Err(InterpretError::super_not_available_parent().into());
    };
    let Some(parent_name) = cx
        .classes
        .get(child_class.as_str())
        .and_then(|d| d.extends.clone())
    else {
        return Err(InterpretError::super_not_available_parent().into());
    };
    let Some(inst_val) = cx.this_stack.last().cloned() else {
        return Err(InterpretError::this_not_allowed_here().into());
    };
    let Value::Instance(rc) = inst_val else {
        return Err(InterpretError::this_not_allowed_here().into());
    };
    run_user_class_constructor_if_any(
        cx,
        parent_name.as_str(),
        &rc,
        Value::Instance(rc.clone()),
        args,
    )?;
    Ok(Value::Null)
}

pub(super) fn eval_new(
    cx: &mut InterpCx,
    type_name: &str,
    args: &[HirExpr],
) -> Result<Value, ExecAbort> {
    // Java VM operation accounting for constructor-like expressions (subset used by `*.ops` tests).
    if let ("SetLiteral", n) = (type_name, args.len()) {
        cx.charge_ops((n as u64).saturating_mul(2))
            .map_err(ExecAbort::Error)?;
    }
    if cx.classes.contains_key(type_name) {
        let mut arg_vals = Vec::with_capacity(args.len());
        for a in args {
            arg_vals.push(eval_expr(cx, a)?);
        }
        return eval_new_with_arg_values(cx, type_name, arg_vals);
    }
    match type_name {
        "Object" => match args.len() {
            0 => Ok(Value::object_from(vec![])),
            n => Err(InterpretError::invalid_constructor(
                "Object",
                &format!("expected 0 arguments, got {n}"),
            )
            .into()),
        },
        "Integer" => match args.len() {
            0 => Ok(Value::Integer(0)),
            1 => {
                let v = eval_expr(cx, &args[0])?;
                Ok(match v {
                    Value::Integer(i) => Value::Integer(i),
                    Value::Real(r) if r.is_finite() => Value::Integer(r as i64),
                    Value::Bool(b) => Value::Integer(i64::from(b)),
                    Value::Null => Value::Integer(0),
                    _ => Value::Integer(number_from_value(&v)? as i64),
                })
            }
            n => Err(InterpretError::invalid_constructor(
                "Integer",
                &format!("expected 0 or 1 arguments, got {n}"),
            )
            .into()),
        },
        "Real" | "Number" => match args.len() {
            0 => Ok(Value::Real(0.0)),
            1 => Ok(Value::Real(number_from_value(&eval_expr(cx, &args[0])?)?)),
            n => Err(InterpretError::invalid_constructor(
                type_name,
                &format!("expected 0 or 1 arguments, got {n}"),
            )
            .into()),
        },
        "Map" | "LegacyLeekArray" => {
            if !args.len().is_multiple_of(2) {
                return Err(InterpretError::invalid_constructor(
                    type_name,
                    "expected an even number of key/value arguments",
                )
                .into());
            }
            let mut m = super::map_store::MapStore::new();
            for i in (0..args.len()).step_by(2) {
                let k = eval_expr(cx, &args[i])?;
                let v = eval_expr(cx, &args[i + 1])?;
                if let Some(j) = m.find_key(&k) {
                    m[j].1 = v;
                } else {
                    m.push_kv(k, v);
                }
            }
            Ok(Value::map_from_store(m))
        }
        "LegacyLeekArrayList" => {
            let mut out = Vec::with_capacity(args.len());
            for a in args {
                out.push(eval_expr(cx, a)?);
            }
            Ok(Value::array_from(out))
        }
        "Set" | "SetLiteral" => {
            let mut s = Vec::new();
            for a in args {
                let v = eval_expr(cx, a)?;
                if !s
                    .iter()
                    .any(|x| super::util::values_equal_for_compare(x, &v))
                {
                    s.push(v);
                }
            }
            let mut out = if type_name == "SetLiteral" {
                // Java suite: literal sets iterate like a `HashSet` (stable type-first order).
                s.sort_by(super::java_export::cmp_java_set_export_order);
                Value::set_from_literal(s)
            } else {
                Value::set_from(s)
            };
            if type_name == "SetLiteral" {
                if let Value::Set(ss) = &mut out {
                    // Ensure set literals export with Java HashSet-like ordering.
                    ss.borrow_mut().java_hash_export = true;
                }
            }
            Ok(out)
        }
        "Interval" => match args.len() {
            0 => Ok(Value::Interval(IntervalValue::default())),
            4 => {
                let min_c = value_truthy(&eval_expr(cx, &args[0])?);
                let min_v = eval_expr(cx, &args[1])?;
                let max_c = value_truthy(&eval_expr(cx, &args[2])?);
                let max_v = eval_expr(cx, &args[3])?;
                let min_shorthand = matches!(
                    &args[1],
                    HirExpr::Real(x) if *x == f64::NEG_INFINITY
                );
                let max_shorthand = matches!(
                    &args[3],
                    HirExpr::Real(x) if x.is_infinite() && x.is_sign_positive()
                );
                Ok(Value::Interval(
                    eval_interval_endpoints(
                        min_c,
                        min_v,
                        max_c,
                        max_v,
                        min_shorthand,
                        max_shorthand,
                    )
                    .map_err(ExecAbort::Error)?,
                ))
            }
            n => Err(InterpretError::invalid_constructor(
                "Interval",
                &format!("expected 0 or 4 arguments, got {n}"),
            )
            .into()),
        },
        "Array" => {
            let mut out = Vec::with_capacity(args.len());
            for a in args {
                out.push(eval_expr(cx, a)?);
            }
            Ok(Value::array_from(out))
        }
        _ => Err(InterpretError::invalid_constructor(type_name, "unknown type for `new`").into()),
    }
}

/// `new Type(...)` from already-evaluated arguments (e.g. `arrayMap` with a class reference).
pub(super) fn eval_new_with_arg_values(
    cx: &mut InterpCx,
    type_name: &str,
    arg_vals: Vec<Value>,
) -> Result<Value, ExecAbort> {
    let Some(cd) = cx.classes.get(type_name) else {
        return Err(
            InterpretError::invalid_constructor(type_name, "unknown type for `new`").into(),
        );
    };
    let class_def = cd.clone();
    let extends = class_def.extends.clone();
    let array_backing = if class_def.extends.as_deref() == Some("Array") {
        Some(Rc::new(RefCell::new(Vec::new())))
    } else {
        None
    };
    let rc = Rc::new(RefCell::new(InstanceData {
        class_name: type_name.to_string(),
        extends,
        array_backing,
        string_override: None,
        fields: Default::default(),
    }));
    {
        let mut inst = rc.borrow_mut();
        for f in &class_def.instance_fields {
            let v = if let Some(expr) = class_def.field_inits.get(f) {
                let raw = eval_expr(cx, expr)?;
                let decl = class_def
                    .field_decl_tys
                    .get(f)
                    .map(std::string::String::as_str);
                super::util::coerce_var_init_value(raw, decl, cx.language_version)?
            } else {
                Value::Null
            };
            inst.fields.insert(f.clone(), v);
        }
    }
    let inst_val = Value::Instance(rc.clone());
    run_user_class_constructor_with_arg_values(cx, type_name, &rc, inst_val, arg_vals)?;
    // Cache `string()` override for export/coercion.
    if cx.language_version >= 2 {
        if let Ok(super::instance::InstanceMethodCallLookup::Resolved {
            callable: m,
            declaring_class: enc,
            bind_this: true,
        }) = super::instance::read_instance_callable_member_for_call(cx, &rc, "string", 0)
        {
            if matches!(m, Value::Function(_) | Value::Native(_)) {
                if let Ok(Value::String(s)) = super::call::invoke_value(
                    cx,
                    Some(Value::Instance(rc.clone())),
                    Some(enc.as_str()),
                    m,
                    Vec::new(),
                    InvokeOptions::strict(),
                ) {
                    rc.borrow_mut().string_override = Some(s);
                }
            }
        }
    }
    Ok(Value::Instance(rc))
}

fn eval_instanceof(cx: &mut InterpCx, left: &HirExpr, right: &HirExpr) -> Result<Value, ExecAbort> {
    let v = eval_expr(cx, left)?;
    let HirExpr::Ident { name, .. } = right else {
        return Err(InterpretError {
            reference: "WRONG_ARGUMENT_TYPE",
            message: "`instanceof` expects a type name on the right-hand side".into(),
        }
        .into());
    };
    Ok(Value::Bool(instanceof_leek_type(
        &v,
        name.as_str(),
        cx.language_version,
    )))
}

fn eval_cast(v: &Value, ty: &HirTypeExpr, language_version: u8) -> Result<Value, InterpretError> {
    match ty {
        HirTypeExpr::Nullable(inner) => {
            if matches!(v, Value::Null) {
                Ok(Value::Null)
            } else {
                eval_cast(v, inner, language_version)
            }
        }
        HirTypeExpr::Union(_tys) => Ok(v.clone()),
        HirTypeExpr::Generic { base, .. } => eval_cast_named(v, base.as_str(), language_version),
        HirTypeExpr::Named(name) => eval_cast_named(v, name.as_str(), language_version),
    }
}

fn eval_cast_named(v: &Value, name: &str, language_version: u8) -> Result<Value, InterpretError> {
    match name {
        "any" | "Object" | "Class" | "void" => Ok(v.clone()),
        "integer" => match v {
            Value::Integer(i) => Ok(Value::Integer(*i)),
            Value::Real(r) => Ok(Value::Integer(*r as i64)),
            Value::Null => Ok(Value::Null),
            _ => Err(InterpretError {
                reference: "IMPOSSIBLE_CAST",
                message: "impossible cast to integer".into(),
            }),
        },
        "real" => match v {
            Value::Integer(i) => Ok(Value::Real(*i as f64)),
            Value::Real(r) => Ok(Value::Real(*r)),
            Value::Null => Ok(Value::Null),
            _ => Err(InterpretError {
                reference: "IMPOSSIBLE_CAST",
                message: "impossible cast to real".into(),
            }),
        },
        // Matches Java `AI.string` / prefix `string expr` (parser treats `string([...])` as cast).
        "string" => Ok(Value::String(value_java_string_coerce(v, language_version))),
        "boolean" => match v {
            Value::Bool(b) => Ok(Value::Bool(*b)),
            Value::Integer(i) => Ok(Value::Bool(*i != 0)),
            Value::Real(r) => Ok(Value::Bool(*r != 0.0)),
            Value::Null => Ok(Value::Bool(false)),
            _ => Err(InterpretError {
                reference: "IMPOSSIBLE_CAST",
                message: "impossible cast to boolean".into(),
            }),
        },
        "Array" | "LegacyLeekArray" => match v {
            Value::Null => Ok(Value::Null),
            Value::Array(_) => Ok(v.clone()),
            _ => Err(InterpretError {
                reference: "IMPOSSIBLE_CAST",
                message: format!("impossible cast to `{name}`"),
            }),
        },
        class_name => match v {
            Value::Null => Ok(Value::Null),
            Value::Instance(rc) => {
                if rc.borrow().class_name == class_name {
                    Ok(Value::Instance(rc.clone()))
                } else {
                    Err(InterpretError {
                        reference: "IMPOSSIBLE_CAST",
                        message: format!("impossible cast to `{class_name}`"),
                    })
                }
            }
            _ => Err(InterpretError {
                reference: "IMPOSSIBLE_CAST",
                message: format!("impossible cast to `{class_name}`"),
            }),
        },
    }
}
