//! Call expressions, including method calls with `this`.

use super::context::InterpCx;
use super::error::{ExecAbort, InterpretError};
use super::exec::exec_stmts;
use super::expr::{eval_expr, eval_new, eval_new_with_arg_values, eval_super_constructor};
use super::flow::StmtFlow;
use super::instance::{
    callable_accepts_arg_count, enforce_static_field_visibility,
    enforce_static_method_call_visibility, read_instance_callable_member_for_call,
    read_super_instance_member, resolve_static_field_owner, resolve_static_method_owner,
    InstanceMethodCallLookup,
};
use super::native::eval_native;
use super::util::{map_find_key, pass_parameter_value};
use super::value::{SharedArray, Value};
use leekscript_hir::{HirExpr, HirFieldVisibility};
use leekscript_resolve::STDLIB_GLOBAL_IDENTIFIERS;

pub(super) struct InvokeOptions<'a> {
    pub enforce_min_arity: bool,
    pub arg_array_cells: Option<&'a [Option<(SharedArray, usize)>]>,
    pub arg_idents: Option<&'a [Option<String>]>,
}

impl<'a> InvokeOptions<'a> {
    pub fn strict() -> Self {
        Self {
            enforce_min_arity: true,
            arg_array_cells: None,
            arg_idents: None,
        }
    }
}

/// Unqualified `name(args)` from a class lexical context: pick a **static** overload on the
/// enclosing class chain, or `None` to fall back to globals (`sqrt` inside `static sqrt()`).
/// `f()` callee fallback: globals in [`InterpCx::env`] must win over lexically enclosing
/// `static f()` when no static overload accepts the call arity (`sqrt(x)` inside `static sqrt()`).
fn resolve_call_callee_after_static_miss(
    cx: &mut InterpCx,
    callee: &HirExpr,
) -> Result<Value, ExecAbort> {
    let HirExpr::Ident { name, .. } = callee else {
        return eval_expr(cx, callee);
    };
    if let Some(v) = cx.env.get(name.as_str()) {
        return Ok(v.clone());
    }
    eval_expr(cx, callee)
}

fn try_static_method_callable(
    cx: &InterpCx,
    name: &str,
    arg_count: usize,
) -> Option<(Value, String)> {
    let enc = cx.enclosing_class_stack.last()?;
    let owner = resolve_static_method_owner(cx, enc.as_str(), name)?;
    let def = cx.classes.get(&owner)?;
    let m = def
        .static_methods
        .get(name)?
        .iter()
        .find(|f| callable_accepts_arg_count(f, arg_count))?
        .clone();
    Some((m, owner))
}

fn java_typed_instance_binding(cx: &InterpCx, base: &HirExpr) -> bool {
    let HirExpr::Ident { name, .. } = base else {
        return false;
    };
    let Some(Some(ty)) = cx.binding_decl_ty.get(name) else {
        return false;
    };
    // Typed locals lower the type spellings to ASCII lowercase (`A a` → `"a"`).
    cx.classes
        .keys()
        .any(|cn| cn.eq_ignore_ascii_case(ty.as_str()))
}

pub(super) fn invoke_value(
    cx: &mut InterpCx,
    this_obj: Option<Value>,
    enclosing_class: Option<&str>,
    func_val: Value,
    arg_vals: Vec<Value>,
    opts: InvokeOptions<'_>,
) -> Result<Value, ExecAbort> {
    let InvokeOptions {
        enforce_min_arity,
        arg_array_cells,
        arg_idents,
    } = opts;
    match func_val {
        Value::Native(name) => {
            if !enforce_min_arity && arg_vals.is_empty() {
                return Ok(Value::Null);
            }
            // Java suite: calling some natives through a variable is more permissive than a direct call.
            // Example: `var a = sqrt; a(25, 16, 9)` evaluates as `sqrt(25)`.
            let arg_vals = if !enforce_min_arity && arg_vals.len() > 1 {
                match name {
                    "sqrt" => vec![arg_vals[0].clone()],
                    _ => arg_vals,
                }
            } else {
                arg_vals
            };
            eval_native(cx, name, &arg_vals, arg_idents).map_err(Into::into)
        }
        Value::Function(f) => {
            let params = &f.params;
            let param_by_ref = &f.param_by_ref;
            let param_decl_tys = &f.param_decl_tys;
            let param_defaults = &f.param_defaults;
            let body = &f.body;
            let captured_locals = f.captured_locals.clone();
            let captured_aliases = f.captured_aliases.clone();
            let declared_return_ty = f.declared_return_ty.clone();

            // Java/Leek unbound method reference calls: `var r = {x: A.m}; r.x(new A())`
            // passes the receiver as the first argument (without binding `this` at lookup time).
            // If the call site didn't already provide `this_obj`, infer it from the first argument.
            let (this_obj, arg_vals, inferred_this) = if this_obj.is_none()
                && matches!(arg_vals.first(), Some(Value::Instance(_)))
                && (arg_vals.len() == params.len() + 1
                    || (f.unbound_method_ref && arg_vals.len() == params.len()))
            {
                let mut av = arg_vals;
                let thisv = av.remove(0);
                (Some(thisv), av, true)
            } else {
                (this_obj, arg_vals, false)
            };
            // Java suite: unbound method reference calls with a receiver but too few remaining args
            // evaluate to `null` (rather than running with missing parameters defaulted).
            if inferred_this && cx.strict != Some(true) && arg_vals.len() < params.len() {
                return Ok(Value::Null);
            }
            if let Some(n) = enclosing_class {
                cx.enclosing_class_stack.push(n.to_string());
            }
            cx.fn_return_ty_stack.push(declared_return_ty.clone());
            cx.final_field_assign_stack.push(false);
            let out = (|| {
                if arg_vals.len() > params.len() {
                    return Err(InterpretError::invalid_parameter_count(
                        params.len(),
                        arg_vals.len(),
                    )
                    .into());
                }
                let pushed_this = if let Some(ref t) = this_obj {
                    cx.this_stack.push(t.clone());
                    true
                } else {
                    false
                };
                let pushed_capture = if let Some(cap) = captured_locals {
                    cx.env.push_block();
                    for (k, v) in cap {
                        cx.env.insert(k, v);
                    }
                    if let Some(aliases) = captured_aliases {
                        for (k, v) in aliases {
                            cx.env.insert_var_alias(k, v);
                        }
                    }
                    true
                } else {
                    false
                };
                cx.env.begin_callable_frame();
                let body_result = (|| -> Result<StmtFlow, ExecAbort> {
                    for i in 0..params.len() {
                        let by_ref = i < param_by_ref.len() && param_by_ref[i];
                        let v = if i < arg_vals.len() {
                            if cx.language_version == 1 && by_ref {
                                if let Some(Some(argn)) = arg_idents.and_then(|a| a.get(i)) {
                                    cx.env.insert_var_alias(params[i].clone(), argn.clone());
                                    continue;
                                }
                            }
                            pass_parameter_value(cx.language_version, arg_vals[i].clone(), by_ref)
                        } else if let Some(expr) = param_defaults.get(i).and_then(|x| x.as_ref()) {
                            eval_expr(cx, expr)?
                        } else {
                            // Java VM suite: non-strict missing args default to `null`;
                            // strict mode requires an error.
                            if cx.strict == Some(true) || enforce_min_arity {
                                return Err(InterpretError::invalid_parameter_count(
                                    params.len(),
                                    arg_vals.len(),
                                )
                                .into());
                            }
                            Value::Null
                        };
                        // v2+: typed parameters coerce the argument (suite uses `integer` / `real`).
                        let v = if cx.language_version >= 2 {
                            if let Some(ty) = param_decl_tys.get(i).and_then(|x| x.as_ref()) {
                                super::util::coerce_var_init_value(
                                    v,
                                    Some(ty.as_str()),
                                    cx.language_version,
                                )?
                            } else {
                                v
                            }
                        } else {
                            v
                        };
                        // Typed function parameters: minimal parity for `Function<...>` in v2+.
                        if let Some(ty) = param_decl_tys.get(i).and_then(|x| x.as_ref()) {
                            if ty.starts_with("function<") && cx.language_version >= 2 {
                                match &v {
                                    Value::Null => {}
                                    Value::Function(fv) => {
                                        if let Some(exp_ret) = ty
                                            .split("=>")
                                            .nth(1)
                                            .map(|s| s.trim().trim_end_matches('>').trim())
                                        {
                                            if !exp_ret.is_empty()
                                                && exp_ret != "any"
                                                && fv.declared_return_ty.as_deref() != Some(exp_ret)
                                            {
                                                return Err(
                                                    InterpretError::impossible_cast().into()
                                                );
                                            }
                                        }
                                    }
                                    Value::Native(_) => {}
                                    _ => return Err(InterpretError::impossible_cast().into()),
                                }
                            }
                        }
                        let cell = arg_array_cells
                            .and_then(|r| r.get(i))
                            .and_then(|o| o.as_ref().map(|(a, u)| (a.clone(), *u)));
                        cx.env.insert_maybe_array_cell(params[i].clone(), v, cell);
                    }
                    exec_stmts(cx, body, false).map_err(Into::into)
                })();
                cx.env.end_callable_frame();
                if pushed_capture {
                    let dropped_cap = cx.env.pop_block();
                    super::ram::release_dropped_binding_values_ram(cx, dropped_cap);
                }
                if pushed_this {
                    cx.this_stack.pop();
                }
                let flow = body_result?;
                match flow {
                    StmtFlow::Continue => Ok(Value::Null),
                    StmtFlow::Return(Some(v)) => {
                        // Coerce numeric return types according to declared return type.
                        let head = declared_return_ty
                            .as_deref()
                            .and_then(|s| s.split_whitespace().next())
                            .unwrap_or("");
                        match head {
                            "real" | "double" | "float" => {
                                if let Value::Integer(ii) = v {
                                    if cx.language_version == 1 {
                                        Ok(Value::RealDotZero(ii as f64))
                                    } else {
                                        Ok(Value::Real(ii as f64))
                                    }
                                } else {
                                    Ok(v)
                                }
                            }
                            "integer" | "int" => {
                                if let Value::Real(r) = v {
                                    if r.is_finite() {
                                        return Ok(Value::Integer(r as i64));
                                    }
                                }
                                Ok(v)
                            }
                            _ => Ok(v),
                        }
                    }
                    StmtFlow::Return(None) => Ok(Value::Null),
                    StmtFlow::Break => Err(InterpretError::break_out_of_loop().into()),
                    StmtFlow::ContinueLoop => Err(InterpretError::continue_out_of_loop().into()),
                    StmtFlow::Throw(v) => Err(ExecAbort::Throw(v)),
                }
            })();
            cx.final_field_assign_stack.pop();
            cx.fn_return_ty_stack.pop();
            if enclosing_class.is_some() {
                cx.enclosing_class_stack.pop();
            }
            out
        }
        Value::UserClass(type_name) => eval_new_with_arg_values(cx, type_name.as_str(), arg_vals),
        Value::Super => Err(InterpretError::not_callable().into()),
        _ => Err(InterpretError::not_callable().into()),
    }
}

pub(super) fn eval_call(
    cx: &mut InterpCx,
    callee: &HirExpr,
    args: &[HirExpr],
) -> Result<Value, ExecAbort> {
    if let HirExpr::Ident { name, .. } = callee {
        if name == "super" && cx.language_version >= 2 {
            return eval_super_constructor(cx, args);
        }
    }

    if let HirExpr::Member { base, field, .. } = callee {
        let mut arg_vals = Vec::with_capacity(args.len());
        let mut arg_idents: Vec<Option<String>> = Vec::with_capacity(args.len());
        for a in args {
            arg_vals.push(eval_expr(cx, a)?);
            arg_idents.push(match a {
                HirExpr::Ident { name, .. } => Some(name.clone()),
                _ => None,
            });
        }
        let base_v = eval_expr(cx, base)?;
        match base_v {
            Value::Map(m) | Value::Object(m) => {
                if field == "values" {
                    if !arg_vals.is_empty() {
                        return Err(
                            InterpretError::invalid_parameter_count(0, arg_vals.len()).into()
                        );
                    }
                    let b = m.borrow();
                    cx.charge_ram_quads(b.len() as u64)?;
                    let vals: Vec<Value> = b.as_slice().iter().map(|(_, v)| v.clone()).collect();
                    return Ok(Value::array_from(vals));
                }
                if field == "keys" {
                    if !arg_vals.is_empty() {
                        return Err(
                            InterpretError::invalid_parameter_count(0, arg_vals.len()).into()
                        );
                    }
                    let b = m.borrow();
                    cx.charge_ram_quads(b.len() as u64)?;
                    let keys: Vec<Value> = b.as_slice().iter().map(|(k, _)| k.clone()).collect();
                    return Ok(Value::array_from(keys));
                }
                let key = Value::String(field.clone());
                let fv = {
                    let b = m.borrow();
                    map_find_key(&b, &key).map(|p| b[p].1.clone())
                };
                let Some(fv) = fv else {
                    return Err(InterpretError::not_callable().into());
                };
                match fv {
                    Value::UserClass(ref tn) => eval_new(cx, tn, args),
                    Value::Function(_) | Value::Native(_) => invoke_value(
                        cx,
                        None,
                        None,
                        fv,
                        arg_vals,
                        InvokeOptions {
                            enforce_min_arity: true,
                            arg_array_cells: None,
                            arg_idents: Some(arg_idents.as_slice()),
                        },
                    ),
                    _ => Err(InterpretError::not_callable().into()),
                }
            }
            Value::UserClass(class_name) => {
                if !cx.classes.contains_key(&class_name) {
                    return Err(InterpretError::not_callable().into());
                }
                let enc = class_name.clone();
                if let Some(owner) =
                    resolve_static_method_owner(cx, class_name.as_str(), field.as_str())
                {
                    enforce_static_method_call_visibility(cx, owner.as_str(), field.as_str())
                        .map_err(ExecAbort::Error)?;
                    let m = cx
                        .classes
                        .get(owner.as_str())
                        .and_then(|d| d.static_methods.get(field.as_str()))
                        .and_then(|vs| {
                            vs.iter()
                                .find(|f| callable_accepts_arg_count(f, arg_vals.len()))
                        })
                        .cloned()
                        .unwrap_or(Value::Null);
                    match &m {
                        Value::Function(_) | Value::Native(_) => {}
                        Value::Null if cx.language_version >= 2 => {
                            return Err(InterpretError::class_static_member_does_not_exist(
                                class_name.as_str(),
                                field.as_str(),
                            )
                            .into());
                        }
                        _ => return Err(InterpretError::not_callable().into()),
                    }
                    return invoke_value(
                        cx,
                        None,
                        Some(enc.as_str()),
                        m,
                        arg_vals,
                        InvokeOptions {
                            enforce_min_arity: true,
                            arg_array_cells: None,
                            arg_idents: Some(arg_idents.as_slice()),
                        },
                    );
                }
                if let Some(owner) =
                    resolve_static_field_owner(cx, class_name.as_str(), field.as_str())
                {
                    enforce_static_field_visibility(cx, owner.as_str(), field.as_str())
                        .map_err(ExecAbort::Error)?;
                    let v = cx
                        .classes
                        .get(owner.as_str())
                        .and_then(|d| d.static_fields.get(field.as_str()))
                        .cloned()
                        .unwrap_or(Value::Null);
                    return match v {
                        Value::UserClass(ref tn) => eval_new(cx, tn, args),
                        Value::Function(_) | Value::Native(_) => invoke_value(
                            cx,
                            None,
                            Some(enc.as_str()),
                            v,
                            arg_vals,
                            InvokeOptions {
                                enforce_min_arity: true,
                                arg_array_cells: None,
                                arg_idents: Some(arg_idents.as_slice()),
                            },
                        ),
                        _ => Err(InterpretError::not_callable().into()),
                    };
                }
                if cx.language_version >= 2 {
                    return Err(InterpretError::class_static_member_does_not_exist(
                        class_name.as_str(),
                        field.as_str(),
                    )
                    .into());
                }
                Err(InterpretError::not_callable().into())
            }
            Value::Instance(rc) => {
                if field == "keys" {
                    if !arg_vals.is_empty() {
                        return Err(
                            InterpretError::invalid_parameter_count(0, arg_vals.len()).into()
                        );
                    }
                    let cn = rc.borrow().class_name.clone();
                    let names: Vec<Value> = cx
                        .classes
                        .get(cn.as_str())
                        .map(|d| {
                            d.instance_fields
                                .iter()
                                .cloned()
                                .map(Value::String)
                                .collect()
                        })
                        .unwrap_or_default();
                    return Ok(Value::array_from(names));
                }
                match read_instance_callable_member_for_call(cx, &rc, field, arg_vals.len())? {
                    InstanceMethodCallLookup::Resolved {
                        callable: m,
                        declaring_class: enc,
                        bind_this,
                    } => {
                        match &m {
                            Value::Function(_) | Value::Native(_) => {}
                            _ => return Err(InterpretError::not_callable().into()),
                        }
                        invoke_value(
                            cx,
                            if bind_this {
                                Some(Value::Instance(rc))
                            } else {
                                None
                            },
                            Some(enc.as_str()),
                            m,
                            arg_vals,
                            InvokeOptions {
                                enforce_min_arity: true,
                                arg_array_cells: None,
                                arg_idents: Some(arg_idents.as_slice()),
                            },
                        )
                    }
                    InstanceMethodCallLookup::ArityMismatch { expected } => Err(
                        InterpretError::invalid_parameter_count(expected, arg_vals.len()).into(),
                    ),
                    InstanceMethodCallLookup::NoMatch => Ok(Value::Null),
                    InstanceMethodCallLookup::Inaccessible(vis) => {
                        if java_typed_instance_binding(cx, base.as_ref()) {
                            return Err(match vis {
                                HirFieldVisibility::Protected => {
                                    InterpretError::protected_method().into()
                                }
                                HirFieldVisibility::Private => {
                                    InterpretError::private_method().into()
                                }
                                HirFieldVisibility::Public => InterpretError::not_callable().into(),
                            });
                        }
                        Ok(Value::Null)
                    }
                }
            }
            Value::Super => {
                let Value::Instance(rc) = cx
                    .this_stack
                    .last()
                    .cloned()
                    .ok_or_else(InterpretError::this_not_allowed_here)?
                else {
                    return Err(InterpretError::this_not_allowed_here().into());
                };
                let enc = cx
                    .enclosing_class_stack
                    .last()
                    .ok_or_else(InterpretError::this_not_allowed_here)?;
                let (m, decl_class) = read_super_instance_member(cx, &rc, field, enc.as_str())?;
                match &m {
                    Value::Function(_) | Value::Native(_) => {}
                    _ => return Err(InterpretError::not_callable().into()),
                }
                let Some(decl) = decl_class else {
                    return Err(InterpretError::super_not_available_parent().into());
                };
                invoke_value(
                    cx,
                    Some(Value::Instance(rc.clone())),
                    Some(decl.as_str()),
                    m,
                    arg_vals,
                    InvokeOptions {
                        enforce_min_arity: true,
                        arg_array_cells: None,
                        arg_idents: Some(arg_idents.as_slice()),
                    },
                )
            }
            _ => Err(InterpretError::member_requires_instance().into()),
        }
    } else {
        let (func_val, mut this_for_call, decl_override) = match callee {
            HirExpr::Ident { name, .. } => {
                if let Some(Value::Instance(rc)) = cx.this_stack.last() {
                    match read_instance_callable_member_for_call(cx, rc, name, args.len()) {
                        Ok(InstanceMethodCallLookup::Resolved {
                            callable: m,
                            declaring_class: decl,
                            bind_this,
                        }) if matches!(m, Value::Function(_) | Value::Native(_)) => (
                            m,
                            if bind_this {
                                Some(Value::Instance(rc.clone()))
                            } else {
                                None
                            },
                            Some(decl),
                        ),
                        Ok(InstanceMethodCallLookup::Inaccessible(_)) => {
                            // `m()` with implicit `this`: never Java-typed receiver binding.
                            (
                                resolve_call_callee_after_static_miss(cx, callee)?,
                                None,
                                None,
                            )
                        }
                        Ok(InstanceMethodCallLookup::ArityMismatch { .. }) => {
                            // Implicit `this` call: if no overload matches the arity, fall back to
                            // static/global resolution (e.g. `sqrt(25)` inside `sqrt()`).
                            (
                                resolve_call_callee_after_static_miss(cx, callee)?,
                                None,
                                None,
                            )
                        }
                        _ => {
                            if let Some((m, owner)) =
                                try_static_method_callable(cx, name.as_str(), args.len())
                            {
                                enforce_static_method_call_visibility(
                                    cx,
                                    owner.as_str(),
                                    name.as_str(),
                                )
                                .map_err(ExecAbort::Error)?;
                                (m, None, Some(owner))
                            } else {
                                (
                                    resolve_call_callee_after_static_miss(cx, callee)?,
                                    None,
                                    None,
                                )
                            }
                        }
                    }
                } else if let Some((m, owner)) =
                    try_static_method_callable(cx, name.as_str(), args.len())
                {
                    enforce_static_method_call_visibility(cx, owner.as_str(), name.as_str())
                        .map_err(ExecAbort::Error)?;
                    (m, None, Some(owner))
                } else {
                    (
                        resolve_call_callee_after_static_miss(cx, callee)?,
                        None,
                        None,
                    )
                }
            }
            _ => (eval_expr(cx, callee)?, None, None),
        };

        if let Value::UserClass(ref type_name) = func_val {
            if this_for_call.is_none() {
                return eval_new(cx, type_name, args);
            }
        }

        let mut arg_vals = Vec::with_capacity(args.len());
        let mut arg_idents: Vec<Option<String>> = Vec::with_capacity(args.len());
        for a in args {
            arg_vals.push(eval_expr(cx, a)?);
            arg_idents.push(match a {
                HirExpr::Ident { name, .. } => Some(name.clone()),
                _ => None,
            });
        }

        if this_for_call.is_none() {
            if let Value::Function(f) = &func_val {
                if arg_vals.len() == f.params.len() + 1
                    && matches!(arg_vals.first(), Some(Value::Instance(_)))
                {
                    this_for_call = Some(arg_vals[0].clone());
                    arg_vals.remove(0);
                }
            }
        }

        let enclosing_name: Option<String> = decl_override.or_else(|| match &this_for_call {
            Some(Value::Instance(rc)) => Some(rc.borrow().class_name.clone()),
            _ => None,
        });

        let enforce_min_arity = match (callee, &func_val) {
            (HirExpr::Ident { name, .. }, Value::Native(native_name)) => {
                // Direct stdlib calls (`sqrt(...)`, `Array(...)`) enforce arity.
                // Calls through user variables (`var a = sqrt; a(...)`) are more permissive.
                //
                // Embedding natives (e.g. Leek Wars fight) are not in `STDLIB_GLOBAL_IDENTIFIERS`,
                // but a **direct** reference `getNearestEnemy()` must still invoke the native with
                // zero args — otherwise `invoke_value` treats empty-arg indirect-style calls as `null`.
                name == native_name
                    || STDLIB_GLOBAL_IDENTIFIERS.contains(&name.as_str())
                    || (cx.language_version >= 3
                        && matches!(
                            name.as_str(),
                            "Array"
                                | "Object"
                                | "Null"
                                | "String"
                                | "Boolean"
                                | "Function"
                                | "Class"
                                | "Interval"
                                | "Integer"
                                | "Real"
                                | "Number"
                        ))
            }
            (HirExpr::Ident { .. }, _) => true,
            _ => false,
        };
        invoke_value(
            cx,
            this_for_call,
            enclosing_name.as_deref(),
            func_val,
            arg_vals,
            InvokeOptions {
                enforce_min_arity,
                arg_array_cells: None,
                arg_idents: Some(arg_idents.as_slice()),
            },
        )
    }
}

/// Call a top-level function or native by name (used for repeated `turn()` in fight simulation).
pub(super) fn invoke_global_by_name(
    cx: &mut InterpCx,
    name: &str,
    arg_vals: Vec<Value>,
) -> Result<Value, ExecAbort> {
    let func_val = cx
        .env
        .get(name)
        .ok_or_else(|| ExecAbort::Error(InterpretError::variable_not_exists(name)))?;
    invoke_value(cx, None, None, func_val, arg_vals, InvokeOptions::strict())
}
