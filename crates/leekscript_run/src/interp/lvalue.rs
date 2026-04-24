//! Assignment to identifiers, indices, members, and `this`.

use super::context::InterpCx;
use super::error::{ExecAbort, InterpretError};
use super::expr::{eval_expr, eval_new};
use super::instance::{
    enforce_instance_field_visibility, enforce_static_field_visibility, resolve_static_field_owner,
};
use super::java_export::charge_java_ai_add_string_branch;
use super::ops::{eval_add, eval_binary};
use super::ram::{self, MAP_RAM_QUADS_PER_ENTRY};
use super::util::{
    arithmetic_operand_as_f64, array_cell_index_for_assign, array_index_at, coerce_var_init_value,
    map_find_key, value_as_array_index_i64,
};
use super::value::{InstanceData, Value};
use leekscript_hir::{HirAssignOp, HirBinOp, HirExpr};
use std::cell::RefCell;
use std::rc::Rc;

/// Java VM parity: in Leek **v1–v3**, inserting a new map/object slot is much more expensive than in v4.
#[inline]
fn charge_legacy_keyed_insert_ops(cx: &mut InterpCx) -> Result<(), ExecAbort> {
    if cx.language_version < 4 {
        cx.charge_ops(20).map_err(ExecAbort::Error)?;
    }
    Ok(())
}

fn strict_type_tag(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
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
        Value::Super => "super",
    }
}

fn is_integer_or_real_constant_member_place(expr: &HirExpr) -> bool {
    let HirExpr::Member { base, field, .. } = expr else {
        return false;
    };
    let HirExpr::Ident { name, .. } = base.as_ref() else {
        return false;
    };
    match name.as_str() {
        "Integer" => matches!(field.as_str(), "MIN_VALUE" | "MAX_VALUE"),
        "Real" => matches!(field.as_str(), "MIN_VALUE" | "MAX_VALUE"),
        _ => false,
    }
}

fn maybe_strict_integer_coerce(
    cx: &InterpCx,
    name_hint: Option<&str>,
    old: &Value,
    newv: Value,
    _op: HirAssignOp,
) -> Value {
    if cx.strict != Some(true) || cx.language_version < 2 {
        return newv;
    }
    if let Some(name) = name_hint {
        if matches!(cx.binding_decl_ty.get(name), Some(Some(ty)) if ty == "any") {
            return newv;
        }
    }
    if matches!(old, Value::Integer(_)) {
        if let Value::Real(r) = newv {
            if r.is_finite() {
                return Value::Integer(r as i64);
            }
        }
    }
    newv
}

#[derive(Debug)]
enum LvalueRoot {
    Var(String),
    This,
}

#[derive(Debug)]
enum LvaluePeelRoot {
    Var(String),
    This,
    ClassSelf,
    New {
        type_name: String,
        args: Vec<HirExpr>,
    },
}

#[derive(Debug)]
enum PathSeg<'a> {
    Index(&'a HirExpr),
    Field(String),
}

pub(super) fn combine_assign_old_rhs(
    cx: &mut InterpCx,
    op: HirAssignOp,
    old: Value,
    rhs: Value,
) -> Result<Value, InterpretError> {
    let language_version = cx.language_version;
    use HirAssignOp::*;
    match op {
        Assign => Ok(rhs),
        AddAssign => {
            if matches!(&old, Value::String(_)) || matches!(&rhs, Value::String(_)) {
                charge_java_ai_add_string_branch(cx, &old, &rhs, language_version)?;
            }
            match eval_add(old.clone(), rhs.clone(), language_version) {
                Ok(v) => Ok(v),
                Err(e)
                    if language_version >= 2
                        && e == InterpretError::wrong_operand_types_binary() =>
                {
                    // Java v2+: compound `+=` on non-numeric operands behaves like numeric coercion,
                    // treating non-numeric values as `0` rather than throwing.
                    let a = arithmetic_operand_as_f64(&old).unwrap_or(0.0);
                    let b = arithmetic_operand_as_f64(&rhs).unwrap_or(0.0);
                    let x = a + b;
                    if x.is_finite()
                        && x.fract() == 0.0
                        && x >= i64::MIN as f64
                        && x <= i64::MAX as f64
                    {
                        Ok(Value::Integer(x as i64))
                    } else {
                        Ok(Value::Real(x))
                    }
                }
                Err(e) => Err(e),
            }
        }
        SubAssign => eval_binary(HirBinOp::Sub, old, rhs, language_version),
        MulAssign => eval_binary(HirBinOp::Mul, old, rhs, language_version),
        DivAssign => eval_binary(HirBinOp::Div, old, rhs, language_version),
        RemAssign => eval_binary(HirBinOp::Rem, old, rhs, language_version),
        PowAssign => eval_binary(HirBinOp::Pow, old, rhs, language_version),
        IntDivAssign => eval_binary(HirBinOp::IntDiv, old, rhs, language_version),
        BitAndAssign => eval_binary(HirBinOp::BitAnd, old, rhs, language_version),
        BitOrAssign => eval_binary(HirBinOp::BitOr, old, rhs, language_version),
        // v1: `^=` is power assignment; v2+: bitwise XOR (see `TestArray` / `TestNumber`).
        BitXorAssign => {
            if language_version <= 1 {
                eval_binary(HirBinOp::Pow, old, rhs, language_version)
            } else {
                eval_binary(HirBinOp::BitXor, old, rhs, language_version)
            }
        }
        ShlAssign => eval_binary(HirBinOp::Shl, old, rhs, language_version),
        ShrAssign => eval_binary(HirBinOp::Shr, old, rhs, language_version),
        UShrAssign => eval_binary(HirBinOp::UShr, old, rhs, language_version),
    }
}

pub(super) fn instance_has_field_named(
    cx: &InterpCx,
    rc: &Rc<RefCell<InstanceData>>,
    name: &str,
) -> bool {
    let d = rc.borrow();
    if d.fields.contains_key(name) {
        return true;
    }
    cx.classes
        .get(d.class_name.as_str())
        .is_some_and(|c| c.instance_fields.iter().any(|f| f == name))
}

fn peel_lvalue_root<'a>(
    mut e: &'a HirExpr,
    acc: &mut Vec<PathSeg<'a>>,
) -> Result<LvaluePeelRoot, ExecAbort> {
    loop {
        match e {
            HirExpr::Ident { name, .. } => return Ok(LvaluePeelRoot::Var(name.clone())),
            HirExpr::This => return Ok(LvaluePeelRoot::This),
            HirExpr::ClassSelf { .. } => return Ok(LvaluePeelRoot::ClassSelf),
            HirExpr::New {
                type_name, args, ..
            } => {
                return Ok(LvaluePeelRoot::New {
                    type_name: type_name.clone(),
                    args: args.clone(),
                });
            }
            HirExpr::Index { base, index, .. } => {
                acc.push(PathSeg::Index(index.as_ref()));
                e = base.as_ref();
            }
            HirExpr::Member { base, field, .. } => {
                acc.push(PathSeg::Field(field.clone()));
                e = base.as_ref();
            }
            _ => return Err(InterpretError::invalid_assign_target().into()),
        }
    }
}

pub(super) fn assign_place(
    cx: &mut InterpCx,
    place: &HirExpr,
    op: HirAssignOp,
    value_expr: &HirExpr,
) -> Result<Value, ExecAbort> {
    cx.last_assign_value = None;
    // Java suite: assigning to a class binding is forbidden (`class A {} A = 12`).
    if cx.language_version >= 2 {
        if let HirExpr::Ident { name, .. } = place {
            if let Some(Value::UserClass(_)) = cx.env.get(name.as_str()) {
                return Err(InterpretError::cant_assign_value().into());
            }
        }
    }
    if cx.language_version >= 3 && is_integer_or_real_constant_member_place(place) {
        return Err(InterpretError::cannot_assign_final_field().into());
    }
    let rhs = eval_expr(cx, value_expr)?;
    if let HirExpr::Ident { name, .. } = place {
        // Java suite: numeric constants are not assignable.
        if matches!(name.as_str(), "Infinity" | "PI" | "E" | "NaN") {
            return Err(InterpretError::cant_assign_value().into());
        }
        // Leek v4+: built-in functions (stdlib natives) are not rebindable.
        if cx.language_version >= 4 && matches!(cx.env.get(name.as_str()), Some(Value::Native(_))) {
            return Err(InterpretError::cannot_redefine_function(name.as_str()).into());
        }
        // Prefer an instance field over an outer/script `var` with the same name when assigning
        // from inside an instance method (`class A { a = 10 m() { return a-- } } var a = A()`).
        if let Some(Value::Instance(rc)) = cx.this_stack.last() {
            if instance_has_field_named(cx, rc, name.as_str()) {
                let segs = [PathSeg::Field(name.clone())];
                assign_walk(cx, LvalueRoot::This, &segs, 0, op, rhs)?;
                return Ok(cx.last_assign_value.clone().unwrap_or(Value::Null));
            }
        }
        if cx.env.in_user_callable() {
            if let Some(enc) = cx.enclosing_class_stack.last() {
                if let Some(owner) = resolve_static_field_owner(cx, enc, name.as_str()) {
                    enforce_static_field_visibility(cx, &owner, name.as_str())?;
                    let old = cx
                        .classes
                        .get(&owner)
                        .and_then(|d| d.static_fields.get(name).cloned())
                        .unwrap_or(Value::Null);
                    let newv = if matches!(op, HirAssignOp::Assign) {
                        rhs
                    } else {
                        let c = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                        maybe_strict_integer_coerce(cx, None, &old, c, op)
                    };
                    let decl = cx
                        .classes
                        .get(&owner)
                        .and_then(|d| d.static_field_decl_tys.get(name).map(|s| s.as_str()));
                    let newv = coerce_var_init_value(newv, decl, cx.language_version)?;
                    let Some(def) = cx.classes.get_mut(&owner) else {
                        return Err(InterpretError::variable_not_exists(name.as_str()).into());
                    };
                    if def
                        .static_field_final
                        .get(name.as_str())
                        .copied()
                        .unwrap_or(false)
                    {
                        return Err(InterpretError::cannot_assign_final_field().into());
                    }
                    def.static_fields.insert(name.clone(), newv.clone());
                    cx.last_assign_value = Some(newv.clone());
                    return Ok(newv);
                }
            }
        }
        if cx.env.get(name).is_some() {
            let newv = if matches!(op, HirAssignOp::Assign) {
                rhs
            } else {
                let old = cx
                    .env
                    .get(name)
                    .ok_or_else(|| InterpretError::variable_not_exists(name.as_str()))?;
                if cx.strict == Some(true)
                    && matches!(old, Value::Null)
                    && !matches!(cx.binding_decl_ty.get(name), Some(Some(ty)) if ty == "any")
                {
                    return Err(InterpretError::assignment_incompatible_type().into());
                }
                let combined = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                maybe_strict_integer_coerce(cx, Some(name.as_str()), &old, combined, op)
            };
            let decl = cx.binding_decl_ty.get(name).and_then(|o| o.as_deref());
            let mut newv = coerce_var_init_value(newv, decl, cx.language_version)?;

            // Strict mode: `var` binds a stable inferred type after its first non-null assignment.
            // Reassigning with an incompatible runtime type is an error (unless declared `any`).
            if cx.strict == Some(true) {
                let cur = cx
                    .env
                    .get(name)
                    .ok_or_else(|| InterpretError::variable_not_exists(name.as_str()))?;
                let new_tag = strict_type_tag(&newv);
                match cx.binding_decl_ty.get(name).and_then(|o| o.as_deref()) {
                    Some("any") => {}
                    Some(t) if t.starts_with("#infer:") => {
                        let expect = &t["#infer:".len()..];
                        // Numeric widening/narrowing: `integer`/`real` can flow both ways.
                        if matches!((expect, new_tag), ("integer" | "real", "integer" | "real")) {
                            if cx.language_version >= 2 {
                                if expect == "real" && new_tag == "integer" {
                                    if let Value::Integer(i) = newv {
                                        newv = Value::Real(i as f64);
                                    }
                                } else if expect == "integer" && new_tag == "real" {
                                    if let Value::Real(r) = newv {
                                        if r.is_finite() {
                                            newv = Value::Integer(r as i64);
                                        }
                                    }
                                }
                            }
                        } else if new_tag != "null" && new_tag != expect {
                            return Err(InterpretError::assignment_incompatible_type().into());
                        }
                    }
                    Some(_decl) => {
                        // Typed locals are coerced via `coerce_var_init_value`; don't enforce here.
                    }
                    None => {
                        let cur_tag = strict_type_tag(&cur);
                        if cur_tag == "null" {
                            if new_tag != "null" {
                                cx.binding_decl_ty
                                    .insert(name.to_string(), Some(format!("#infer:{new_tag}")));
                            }
                        } else if new_tag != "null" && new_tag != cur_tag {
                            return Err(InterpretError::assignment_incompatible_type().into());
                        }
                    }
                };
            }
            cx.last_assign_value = Some(newv.clone());
            // Leek v1: assigning to a captured outer variable inside a `function(){...}` creates a
            // new local binding instead of mutating the outer one (parity: `for (var @e in a) {
            // (function(){ e = 5 })() }` must not mutate `a`).
            if cx.language_version == 1
                && cx.env.in_user_callable()
                && cx.env.get_callable_local(name).is_none()
                && cx.env.callable_outer_lexical_is_array_ref(name.as_str())
                && !cx.env.is_aliased(name.as_str())
            {
                cx.env.insert(name.to_string(), newv);
            } else {
                cx.assign_with_ram(name, newv)?;
            }
            return Ok(cx.last_assign_value.clone().unwrap_or(Value::Null));
        }
        return Err(InterpretError::variable_not_exists(name.as_str()).into());
    }
    let mut segs = Vec::new();
    let root = peel_lvalue_root(place, &mut segs)?;
    if segs.is_empty() {
        return Err(InterpretError::invalid_assign_target().into());
    }
    segs.reverse();
    match root {
        LvaluePeelRoot::Var(name) => {
            assign_walk(cx, LvalueRoot::Var(name), &segs, 0, op, rhs)?;
            Ok(cx.last_assign_value.clone().unwrap_or(Value::Null))
        }
        LvaluePeelRoot::This => {
            assign_walk(cx, LvalueRoot::This, &segs, 0, op, rhs)?;
            Ok(cx.last_assign_value.clone().unwrap_or(Value::Null))
        }
        LvaluePeelRoot::ClassSelf => {
            let Some(enc) = cx.enclosing_class_stack.last() else {
                return Err(InterpretError::class_self_not_allowed_here().into());
            };
            let mut cur = Value::UserClass(enc.clone());
            assign_walk_value(cx, &mut cur, &segs, 0, op, rhs)?;
            Ok(cx.last_assign_value.clone().unwrap_or(Value::Null))
        }
        LvaluePeelRoot::New { type_name, args } => {
            let mut cur = eval_new(cx, type_name.as_str(), &args)?;
            assign_walk_value(cx, &mut cur, &segs, 0, op, rhs)?;
            Ok(cx.last_assign_value.clone().unwrap_or(Value::Null))
        }
    }
}

fn assign_walk(
    cx: &mut InterpCx,
    root: LvalueRoot,
    segs: &[PathSeg<'_>],
    idx: usize,
    op: HirAssignOp,
    rhs: Value,
) -> Result<(), ExecAbort> {
    if idx == segs.len() {
        return Err(InterpretError::invalid_assign_target().into());
    }
    match &root {
        LvalueRoot::Var(name) => {
            let mut cur = cx
                .env
                .get(name)
                .ok_or_else(|| InterpretError::variable_not_exists(name.as_str()))?;
            assign_walk_value(cx, &mut cur, segs, idx, op, rhs)?;
            // Ensure RAM quota is enforced even if the inner path mutates through shared Rcs.
            // This catches map/object growth patterns used by the Java parity suite RAM tests.
            if let Some(limit) = cx.ram_quads_limit {
                match &cur {
                    Value::Map(m) | Value::Object(m) => {
                        let q = MAP_RAM_QUADS_PER_ENTRY * m.borrow().len() as u64;
                        cx.ram_quads_used = cx.ram_quads_used.max(q);
                        if q > limit {
                            return Err(ExecAbort::Error(InterpretError::out_of_memory()));
                        }
                    }
                    Value::Array(a) => {
                        let len = a.borrow().len() as u64;
                        cx.ram_quads_used = cx.ram_quads_used.max(len);
                        if len > limit {
                            return Err(ExecAbort::Error(InterpretError::out_of_memory()));
                        }
                    }
                    _ => {}
                }
            }
            let _ = cx.env.assign(name, cur)?;
            Ok(())
        }
        LvalueRoot::This => {
            let Some(Value::Instance(rc)) = cx.this_stack.last() else {
                return Err(InterpretError::this_not_allowed_here().into());
            };
            let mut wrap = Value::Instance(rc.clone());
            assign_walk_value(cx, &mut wrap, segs, idx, op, rhs)?;
            Ok(())
        }
    }
}

fn assign_walk_value(
    cx: &mut InterpCx,
    cur: &mut Value,
    segs: &[PathSeg<'_>],
    idx: usize,
    op: HirAssignOp,
    rhs: Value,
) -> Result<(), ExecAbort> {
    if idx + 1 == segs.len() {
        match &segs[idx] {
            PathSeg::Index(ix_expr) => {
                let key = eval_expr(cx, ix_expr)?;
                index_assign_on_value(cx, cur, &key, op, rhs)?;
            }
            PathSeg::Field(field) => match cur {
                Value::Instance(rc) => {
                    enforce_instance_field_visibility(cx, rc, field.as_str())?;
                    let cn = rc.borrow().class_name.clone();
                    let allow_final_init =
                        cx.final_field_assign_stack.last().copied().unwrap_or(false);
                    if !allow_final_init
                        && cx
                            .classes
                            .get(cn.as_str())
                            .and_then(|d| d.field_final.get(field.as_str()))
                            .copied()
                            .unwrap_or(false)
                    {
                        return Err(InterpretError::cannot_assign_final_field().into());
                    }
                    let mut data = rc.borrow_mut();
                    let old = data.fields.get(field).cloned().unwrap_or(Value::Null);
                    let newv = if matches!(op, HirAssignOp::Assign) {
                        rhs
                    } else {
                        let c = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                        maybe_strict_integer_coerce(cx, None, &old, c, op)
                    };
                    let decl = cx
                        .classes
                        .get(data.class_name.as_str())
                        .and_then(|def| def.field_decl_tys.get(field).map(|s| s.as_str()));
                    let newv = coerce_var_init_value(newv, decl, cx.language_version)?;
                    let new_slot = !data.fields.contains_key(field.as_str());
                    data.fields.insert(field.clone(), newv.clone());
                    if new_slot {
                        cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY)
                            .map_err(ExecAbort::Error)?;
                    }
                    cx.last_assign_value = Some(newv);
                }
                Value::UserClass(cn) => {
                    let cn = cn.clone();
                    let owner = resolve_static_field_owner(cx, &cn, field.as_str())
                        .unwrap_or_else(|| cn.clone());
                    enforce_static_field_visibility(cx, &owner, field.as_str())?;
                    // Java v2+: assigning to class members is only allowed for existing static fields.
                    // Overwriting methods (e.g. `A.m = 12`) must fail.
                    if cx.language_version >= 2 {
                        let Some(def_ro) = cx.classes.get(&owner) else {
                            return Err(InterpretError::cant_assign_value().into());
                        };
                        let field_exists = def_ro.static_fields.contains_key(field.as_str());
                        let method_exists = def_ro.static_methods.contains_key(field.as_str());
                        if !field_exists || method_exists {
                            return Err(InterpretError::cant_assign_value().into());
                        }
                    }
                    if cx
                        .classes
                        .get(&owner)
                        .and_then(|d| d.static_field_final.get(field.as_str()))
                        .copied()
                        .unwrap_or(false)
                    {
                        return Err(InterpretError::cannot_assign_final_field().into());
                    }
                    let old = cx
                        .classes
                        .get(&owner)
                        .and_then(|d| d.static_fields.get(field).cloned())
                        .unwrap_or(Value::Null);
                    let newv = if matches!(op, HirAssignOp::Assign) {
                        rhs
                    } else {
                        let c = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                        maybe_strict_integer_coerce(cx, None, &old, c, op)
                    };
                    let decl = cx
                        .classes
                        .get(&owner)
                        .and_then(|d| d.static_field_decl_tys.get(field).map(|s| s.as_str()));
                    let newv = coerce_var_init_value(newv, decl, cx.language_version)?;
                    let Some(def) = cx.classes.get_mut(&owner) else {
                        return Err(InterpretError::member_requires_instance().into());
                    };
                    def.static_fields.insert(field.clone(), newv.clone());
                    cx.last_assign_value = Some(newv);
                }
                Value::Map(m) | Value::Object(m) => {
                    let key = Value::String(field.clone());
                    let mut bm = m.borrow_mut();
                    let old = map_find_key(&bm, &key)
                        .map(|p| bm[p].1.clone())
                        .unwrap_or(Value::Null);
                    let newv = if matches!(op, HirAssignOp::Assign) {
                        rhs
                    } else {
                        let c = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                        maybe_strict_integer_coerce(cx, None, &old, c, op)
                    };
                    if let Some(p) = map_find_key(&bm, &key) {
                        bm[p].1 = newv;
                        cx.last_assign_value = Some(bm[p].1.clone());
                    } else {
                        cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY)
                            .map_err(ExecAbort::Error)?;
                        charge_legacy_keyed_insert_ops(cx)?;
                        bm.push_kv(key, newv);
                        cx.last_assign_value = bm.last_pair().map(|p| p.1.clone());
                    }
                    ram::note_keyed_container_ram_peak(cx, bm.len()).map_err(ExecAbort::Error)?;
                }
                _ => return Err(InterpretError::member_requires_instance().into()),
            },
        }
        return Ok(());
    }
    match &segs[idx] {
        PathSeg::Index(ix_expr) => {
            let key = eval_expr(cx, ix_expr)?;
            match cur {
                Value::Null => return Ok(()),
                Value::Array(arr) => {
                    let i = {
                        let b = arr.borrow();
                        array_index_at(&key, b.len())?
                    };
                    if idx + 1 >= segs.len() {
                        let old = { arr.borrow()[i].clone() };
                        let newv = if matches!(op, HirAssignOp::Assign) {
                            rhs
                        } else {
                            combine_assign_old_rhs(cx, op, old, rhs)?
                        };
                        arr.borrow_mut()[i] = newv;
                    } else {
                        // Avoid holding a mutable borrow across recursion: nested paths can read the
                        // same array again (self-referential structures / recursion), which would
                        // panic with "already mutably borrowed".
                        let mut child = { arr.borrow()[i].clone() };
                        assign_walk_value(cx, &mut child, segs, idx + 1, op, rhs)?;
                        arr.borrow_mut()[i] = child;
                    }
                }
                Value::Map(m) | Value::Object(m) => {
                    let pos = {
                        let b = m.borrow();
                        map_find_key(&b, &key)
                    };
                    if idx + 1 >= segs.len() {
                        let mut b = m.borrow_mut();
                        if let Some(p) = pos {
                            let old = b[p].1.clone();
                            let newv = if matches!(op, HirAssignOp::Assign) {
                                rhs
                            } else {
                                combine_assign_old_rhs(cx, op, old, rhs)?
                            };
                            b[p].1 = newv;
                        } else {
                            cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY)
                                .map_err(ExecAbort::Error)?;
                            charge_legacy_keyed_insert_ops(cx)?;
                            b.push_kv(key.clone(), rhs);
                        }
                        ram::note_keyed_container_ram_peak(cx, b.len())
                            .map_err(ExecAbort::Error)?;
                    } else {
                        // Same borrow-avoidance as arrays.
                        let (p, mut child) = {
                            let b = m.borrow();
                            if let Some(p) = pos {
                                (p, b[p].1.clone())
                            } else {
                                // We'll append a new slot and recurse into it.
                                (usize::MAX, Value::Null)
                            }
                        };
                        assign_walk_value(cx, &mut child, segs, idx + 1, op, rhs)?;
                        let mut b = m.borrow_mut();
                        if p != usize::MAX {
                            b[p].1 = child;
                        } else {
                            cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY)
                                .map_err(ExecAbort::Error)?;
                            charge_legacy_keyed_insert_ops(cx)?;
                            b.push_kv(key.clone(), child);
                        }
                        ram::note_keyed_container_ram_peak(cx, b.len())
                            .map_err(ExecAbort::Error)?;
                    }
                }
                Value::UserClass(cn) => {
                    let field = match key {
                        Value::String(s) => s.clone(),
                        _ => return Err(InterpretError::not_indexable().into()),
                    };
                    let cn = cn.clone();
                    let owner =
                        resolve_static_field_owner(cx, &cn, &field).unwrap_or_else(|| cn.clone());
                    enforce_static_field_visibility(cx, &owner, &field)?;
                    if cx
                        .classes
                        .get(&owner)
                        .and_then(|d| d.static_field_final.get(field.as_str()))
                        .copied()
                        .unwrap_or(false)
                    {
                        return Err(InterpretError::cannot_assign_final_field().into());
                    }
                    let mut inner = cx
                        .classes
                        .get(&owner)
                        .and_then(|d| d.static_fields.get(&field).cloned())
                        .unwrap_or(Value::Null);
                    assign_walk_value(cx, &mut inner, segs, idx + 1, op, rhs)?;
                    let Some(def) = cx.classes.get_mut(&owner) else {
                        return Err(InterpretError::not_indexable().into());
                    };
                    def.static_fields.insert(field, inner);
                }
                _ => return Err(InterpretError::not_indexable().into()),
            }
        }
        PathSeg::Field(field) => match cur {
            Value::Instance(rc) => {
                let mut bm = rc.borrow_mut();
                let slot = bm.fields.entry(field.clone()).or_insert(Value::Null);
                assign_walk_value(cx, slot, segs, idx + 1, op, rhs)?;
            }
            Value::Map(m) | Value::Object(m) => {
                let key = Value::String(field.clone());
                // Avoid holding a mutable borrow across recursion (same reason as index paths).
                let (p, mut child) = {
                    let b = m.borrow();
                    if let Some(p) = map_find_key(&b, &key) {
                        (p, b[p].1.clone())
                    } else {
                        (usize::MAX, Value::Null)
                    }
                };
                assign_walk_value(cx, &mut child, segs, idx + 1, op, rhs)?;
                let mut b = m.borrow_mut();
                if p != usize::MAX {
                    b[p].1 = child;
                } else {
                    cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY)
                        .map_err(ExecAbort::Error)?;
                    charge_legacy_keyed_insert_ops(cx)?;
                    b.push_kv(key, child);
                }
                ram::note_keyed_container_ram_peak(cx, b.len()).map_err(ExecAbort::Error)?;
            }
            Value::UserClass(cn) => {
                let cn = cn.clone();
                let owner = resolve_static_field_owner(cx, &cn, field.as_str())
                    .unwrap_or_else(|| cn.clone());
                enforce_static_field_visibility(cx, &owner, field.as_str())?;
                if cx
                    .classes
                    .get(&owner)
                    .and_then(|d| d.static_field_final.get(field.as_str()))
                    .copied()
                    .unwrap_or(false)
                {
                    return Err(InterpretError::cannot_assign_final_field().into());
                }
                let mut inner = cx
                    .classes
                    .get(&owner)
                    .and_then(|d| d.static_fields.get(field).cloned())
                    .unwrap_or(Value::Null);
                assign_walk_value(cx, &mut inner, segs, idx + 1, op, rhs)?;
                let Some(def) = cx.classes.get_mut(&owner) else {
                    return Err(InterpretError::member_requires_instance().into());
                };
                def.static_fields.insert(field.clone(), inner);
            }
            _ => return Err(InterpretError::member_requires_instance().into()),
        },
    }
    Ok(())
}

fn index_assign_on_value(
    cx: &mut InterpCx,
    container: &mut Value,
    key: &Value,
    op: HirAssignOp,
    rhs: Value,
) -> Result<(), ExecAbort> {
    match container {
        Value::Null => {
            if cx.strict == Some(true) {
                return Err(InterpretError::assignment_incompatible_type().into());
            }
            Ok(())
        }
        Value::Array(arr) => {
            if cx.language_version < 4 {
                // Java v1–v3: `a[a] = rhs` uses the container as its own subscript → write slot 0
                // (empty array grows to `[rhs]`; matches `[:]` then `a[a] = 1` → export `[1]`).
                if let Value::Array(ka) = key {
                    if Rc::ptr_eq(ka, arr) {
                        let mut b = arr.borrow_mut();
                        if b.is_empty() {
                            b.push(rhs);
                        } else {
                            b[0] = rhs;
                        }
                        return Ok(());
                    }
                }
                if value_as_array_index_i64(key).is_err() {
                    let map_val = {
                        let b = arr.borrow();
                        let pairs: Vec<(Value, Value)> = b
                            .iter()
                            .enumerate()
                            .map(|(i, v)| (Value::Integer(i as i64), v.clone()))
                            .collect();
                        Value::map_from(pairs)
                    };
                    *container = map_val;
                    return index_assign_on_value(cx, container, key, op, rhs);
                }
                let raw = value_as_array_index_i64(key)?;
                let len = arr.borrow().len();
                let len_i = len as i64;
                let j = if raw < 0 { raw + len_i } else { raw };
                let in_dense = j >= 0 && (j as usize) < len;
                if !in_dense {
                    let promote = if j < 0 {
                        true
                    } else if len == 0 {
                        j != 0
                    } else {
                        j > len_i
                    };
                    if promote {
                        let map_val = {
                            let b = arr.borrow();
                            let pairs: Vec<(Value, Value)> = b
                                .iter()
                                .enumerate()
                                .map(|(i, v)| (Value::Integer(i as i64), v.clone()))
                                .collect();
                            Value::map_from(pairs)
                        };
                        *container = map_val;
                        return index_assign_on_value(cx, container, &Value::Integer(raw), op, rhs);
                    }
                }
            }
            if cx.language_version >= 4 && value_as_array_index_i64(key).is_err() {
                if cx.strict == Some(true) {
                    return Err(InterpretError::assignment_incompatible_type().into());
                }
                return Ok(());
            }
            let mut b = arr.borrow_mut();
            let Some(i) =
                array_cell_index_for_assign(&mut b, key, op, cx.language_version, cx.strict)?
            else {
                return Ok(());
            };
            let mut old = b[i].clone();
            if cx.language_version < 4
                && !matches!(op, HirAssignOp::Assign)
                && matches!(old, Value::Null)
            {
                old = Value::Integer(0);
            }
            let newv = if matches!(op, HirAssignOp::Assign) {
                rhs
            } else {
                let c = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                maybe_strict_integer_coerce(cx, None, &old, c, op)
            };
            b[i] = newv;
            cx.last_assign_value = Some(b[i].clone());
            Ok(())
        }
        Value::Map(m) | Value::Object(m) => {
            let eff_key = if cx.language_version < 4 {
                if let Value::Map(km) | Value::Object(km) = key {
                    if Rc::ptr_eq(km, m) {
                        Value::Integer(1)
                    } else {
                        key.clone()
                    }
                } else {
                    key.clone()
                }
            } else {
                key.clone()
            };
            let mut bm = m.borrow_mut();
            let old = map_find_key(&bm, &eff_key)
                .map(|p| bm[p].1.clone())
                .unwrap_or(Value::Null);
            if cx.language_version >= 4
                && cx.strict == Some(true)
                && !matches!(op, HirAssignOp::Assign)
                && matches!(old, Value::Null)
            {
                return Err(InterpretError::assignment_incompatible_type().into());
            }
            let newv = if matches!(op, HirAssignOp::Assign) {
                rhs
            } else {
                let c = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                maybe_strict_integer_coerce(cx, None, &old, c, op)
            };
            if let Some(p) = map_find_key(&bm, &eff_key) {
                bm[p].1 = newv;
                cx.last_assign_value = Some(bm[p].1.clone());
            } else {
                cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY)
                    .map_err(ExecAbort::Error)?;
                charge_legacy_keyed_insert_ops(cx)?;
                bm.push_kv(eff_key, newv);
                cx.last_assign_value = bm.last_pair().map(|p| p.1.clone());
            }
            ram::note_keyed_container_ram_peak(cx, bm.len()).map_err(ExecAbort::Error)?;
            Ok(())
        }
        Value::Instance(rc) => {
            let field = match key {
                Value::String(s) => s.clone(),
                _ => return Err(InterpretError::not_indexable().into()),
            };
            enforce_instance_field_visibility(cx, rc, field.as_str())?;
            let cn = rc.borrow().class_name.clone();
            if cx
                .classes
                .get(cn.as_str())
                .and_then(|d| d.field_final.get(field.as_str()))
                .copied()
                .unwrap_or(false)
            {
                let allow_final_init = cx.final_field_assign_stack.last().copied().unwrap_or(false);
                if !allow_final_init {
                    if cx.strict == Some(true) {
                        return Err(InterpretError::cannot_assign_final_field().into());
                    }
                    return Ok(());
                }
            }
            let mut data = rc.borrow_mut();
            let old = data.fields.get(&field).cloned().unwrap_or(Value::Null);
            let newv = if matches!(op, HirAssignOp::Assign) {
                rhs
            } else {
                let c = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                maybe_strict_integer_coerce(cx, None, &old, c, op)
            };
            let decl = cx
                .classes
                .get(data.class_name.as_str())
                .and_then(|def| def.field_decl_tys.get(&field).map(|s| s.as_str()));
            let newv = coerce_var_init_value(newv, decl, cx.language_version)?;
            let new_slot = !data.fields.contains_key(field.as_str());
            data.fields.insert(field, newv.clone());
            if new_slot {
                cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY)
                    .map_err(ExecAbort::Error)?;
            }
            cx.last_assign_value = Some(newv);
            Ok(())
        }
        Value::UserClass(cn) => {
            let field = match key {
                Value::String(s) => s.clone(),
                _ => return Err(InterpretError::not_indexable().into()),
            };
            let cn = cn.clone();
            let owner = resolve_static_field_owner(cx, &cn, &field).unwrap_or_else(|| cn.clone());
            enforce_static_field_visibility(cx, &owner, &field)?;
            if cx
                .classes
                .get(&owner)
                .and_then(|d| d.static_field_final.get(field.as_str()))
                .copied()
                .unwrap_or(false)
            {
                return Err(InterpretError::cannot_assign_final_field().into());
            }
            let old = cx
                .classes
                .get(&owner)
                .and_then(|d| d.static_fields.get(&field).cloned())
                .unwrap_or(Value::Null);
            let newv = if matches!(op, HirAssignOp::Assign) {
                rhs
            } else {
                let c = combine_assign_old_rhs(cx, op, old.clone(), rhs)?;
                maybe_strict_integer_coerce(cx, None, &old, c, op)
            };
            let decl = cx
                .classes
                .get(&owner)
                .and_then(|d| d.static_field_decl_tys.get(&field).map(|s| s.as_str()));
            let newv = coerce_var_init_value(newv, decl, cx.language_version)?;
            let Some(def) = cx.classes.get_mut(&owner) else {
                return Err(InterpretError::not_indexable().into());
            };
            def.static_fields.insert(field, newv.clone());
            cx.last_assign_value = Some(newv);
            Ok(())
        }
        _ => Err(InterpretError::not_indexable().into()),
    }
}
