//! Instance field and method lookup.

use super::context::{is_subclass_of, InterpCx};
use super::error::InterpretError;
use super::value::{InstanceData, Value};
use leekscript_hir::HirFieldVisibility;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// Result of resolving an instance method for a **call** (arity match), including visibility.
pub(super) enum InstanceMethodCallLookup {
    Resolved {
        callable: Value,
        declaring_class: String,
        bind_this: bool,
    },
    /// No method with matching arity on the `extends` chain.
    NoMatch,
    /// A method name exists, but no overload accepts the call arity.
    ArityMismatch { expected: usize },
    /// A method matches arity but is not visible from [`InterpCx::enclosing_class_stack`].
    Inaccessible(HirFieldVisibility),
}

pub(super) fn callable_accepts_arg_count(func: &Value, n: usize) -> bool {
    match func {
        Value::Function(f) => {
            let params = &f.params;
            let param_defaults = &f.param_defaults;
            if n > params.len() {
                return false;
            }
            let mut min = 0usize;
            for i in 0..params.len() {
                let has_def = i < param_defaults.len() && param_defaults[i].is_some();
                if has_def {
                    break;
                }
                min += 1;
            }
            n >= min
        }
        Value::Native(_) => true,
        _ => false,
    }
}

pub(super) fn read_instance_member(
    cx: &InterpCx,
    rc: &Rc<RefCell<InstanceData>>,
    field: &str,
) -> Result<Value, InterpretError> {
    let inst = rc.borrow();
    if let Some(v) = inst.fields.get(field) {
        return Ok(v.clone());
    }
    let class_name = inst.class_name.clone();
    drop(inst);
    if field == "class" {
        if cx.language_version >= 2 {
            return Ok(Value::UserClass(class_name));
        }
        let fields: Vec<Value> = cx
            .classes
            .get(class_name.as_str())
            .map(|def| {
                def.instance_fields
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect()
            })
            .unwrap_or_default();
        return Ok(Value::map_from(vec![
            (Value::String("name".into()), Value::String(class_name)),
            (Value::String("fields".into()), Value::array_from(fields)),
        ]));
    }
    if let Some(class_def) = cx.classes.get(&class_name) {
        if let Some(f) = class_def.methods.get(field).and_then(|vs| vs.first()) {
            return Ok(f.clone());
        }
    }
    Ok(Value::Null)
}

/// Instance method on a **class** value (`A.m`), for unbound refs / parity with Java.
/// Walks `extends` like [`read_instance_callable_member`]; `None` if no such method exists.
pub(super) fn read_class_ref_instance_callable(
    cx: &InterpCx,
    start_class: &str,
    field: &str,
) -> Option<Value> {
    let mut seen = HashSet::<String>::new();
    let mut cursor: Option<String> = Some(start_class.to_string());
    for _ in 0..64 {
        let Some(cn) = cursor else {
            break;
        };
        if !seen.insert(cn.clone()) {
            break;
        }
        if let Some(class_def) = cx.classes.get(&cn) {
            if let Some(vs) = class_def.methods.get(field) {
                // Prefer a method variant that accepts 1 argument (unbound ref expects `this`).
                // Fallback to nullary for legacy fixtures.
                let f = vs.first()?.clone();
                let vis = class_def
                    .method_visibility
                    .get(field)
                    .copied()
                    .unwrap_or(HirFieldVisibility::Public);
                if static_field_visible_for_read(cx, cn.as_str(), vis) {
                    return Some(f);
                }
                return Some(Value::Null);
            }
            cursor = class_def.extends.clone();
        } else {
            break;
        }
    }
    None
}

/// Virtual dispatch: instance field, `.class`, then instance methods up the `extends` chain.
pub(super) fn read_instance_callable_member(
    cx: &InterpCx,
    rc: &Rc<RefCell<InstanceData>>,
    field: &str,
) -> Result<Value, InterpretError> {
    let inst = rc.borrow();
    if let Some(v) = inst.fields.get(field) {
        return Ok(v.clone());
    }
    let class_name = inst.class_name.clone();
    drop(inst);
    if field == "class" {
        return read_instance_member(cx, rc, field);
    }
    let mut seen = HashSet::<String>::new();
    let mut cursor: Option<String> = Some(class_name);
    for _ in 0..64 {
        let Some(cn) = cursor else {
            break;
        };
        if !seen.insert(cn.clone()) {
            break;
        }
        if let Some(class_def) = cx.classes.get(&cn) {
            if let Some(f) = class_def.methods.get(field).and_then(|vs| vs.first()) {
                let vis = class_def
                    .method_visibility
                    .get(field)
                    .copied()
                    .unwrap_or(HirFieldVisibility::Public);
                if static_field_visible_for_read(cx, cn.as_str(), vis) {
                    return Ok(f.clone());
                }
                return Ok(Value::Null);
            }
            cursor = class_def.extends.clone();
        } else {
            break;
        }
    }
    Ok(Value::Null)
}

/// Like [`read_instance_callable_member`], but for **calls**: pick the first method in the `extends`
/// chain whose parameter list accepts `arg_count` arguments (Java overload resolution on arity +
/// trailing defaults).
///
/// Returns `(callable, declaring_class)`; `declaring_class` is used as `enclosing_class` in
/// [`super::call::invoke_value`].
pub(super) fn read_instance_callable_member_for_call(
    cx: &InterpCx,
    rc: &Rc<RefCell<InstanceData>>,
    field: &str,
    arg_count: usize,
) -> Result<InstanceMethodCallLookup, InterpretError> {
    let inst = rc.borrow();
    // For calls, methods win over fields (Java/Leek semantics: `obj.a()` calls method `a` even if a
    // field `a` exists). Keep the field value as a fallback when no method matches.
    let fallback_field = inst.fields.get(field).cloned();
    let class_name = inst.class_name.clone();
    drop(inst);
    if field == "class" {
        let v = read_instance_member(cx, rc, field)?;
        return Ok(InstanceMethodCallLookup::Resolved {
            callable: v,
            declaring_class: class_name,
            bind_this: true,
        });
    }
    let mut seen = HashSet::<String>::new();
    let mut cursor: Option<String> = Some(class_name);
    let mut saw_method_name = false;
    let mut expected_min: Option<usize> = None;
    for _ in 0..64 {
        let Some(cn) = cursor else {
            break;
        };
        if !seen.insert(cn.clone()) {
            break;
        }
        if let Some(class_def) = cx.classes.get(&cn) {
            if let Some(vs) = class_def.methods.get(field) {
                let mut blocked_vis = None;
                saw_method_name = true;
                for f in vs {
                    if expected_min.is_none() {
                        expected_min = match f {
                            Value::Function(ff) => Some(ff.params.len()),
                            Value::Native(_) => Some(arg_count), // native arity is unknown here
                            _ => None,
                        };
                    }
                    if callable_accepts_arg_count(f, arg_count) {
                        let vis = class_def
                            .method_visibility
                            .get(field)
                            .copied()
                            .unwrap_or(HirFieldVisibility::Public);
                        if static_field_visible_for_read(cx, cn.as_str(), vis) {
                            return Ok(InstanceMethodCallLookup::Resolved {
                                callable: f.clone(),
                                declaring_class: cn,
                                bind_this: true,
                            });
                        }
                        blocked_vis = Some(vis);
                    }
                }
                if let Some(vis) = blocked_vis {
                    return Ok(InstanceMethodCallLookup::Inaccessible(vis));
                }
                // No arity match on this class; keep walking `extends` (a parent may have a different arity).
            }
            cursor = class_def.extends.clone();
        } else {
            break;
        }
    }
    if saw_method_name {
        return Ok(InstanceMethodCallLookup::ArityMismatch {
            expected: expected_min.unwrap_or(arg_count),
        });
    }
    if let Some(v) = fallback_field {
        // Callable stored in a field (e.g. `x = A.m`): do NOT bind `this` to the container instance.
        let cn = rc.borrow().class_name.clone();
        return Ok(InstanceMethodCallLookup::Resolved {
            callable: v,
            declaring_class: cn,
            bind_this: false,
        });
    }
    Ok(InstanceMethodCallLookup::NoMatch)
}

/// Member lookup for `super` in a user method: starts at the **parent of `from_class`** (the class
/// that lexically contains the `super` expression), then walks `extends` like JVM `invokespecial`.
///
/// `from_class` must be [`InterpCx::enclosing_class_stack`]’s innermost user class — **not** the
/// instance’s runtime class, or `super.m()` inside `B.m` on a `C` instance would resolve `m` on `C`’s
/// parent (`B`) and return `B.m` again (infinite recursion).
pub(super) fn read_super_instance_member(
    cx: &InterpCx,
    rc: &Rc<RefCell<InstanceData>>,
    field: &str,
    from_class: &str,
) -> Result<(Value, Option<String>), InterpretError> {
    let Some(def) = cx.classes.get(from_class) else {
        return Ok((Value::Null, None));
    };
    let mut cursor = def.extends.clone();
    let mut seen = HashSet::<String>::new();
    for _ in 0..64 {
        let Some(cn) = cursor else {
            break;
        };
        if !seen.insert(cn.clone()) {
            break;
        }
        if let Some(class_def) = cx.classes.get(&cn) {
            if let Some(v) = class_def.methods.get(field).and_then(|vs| vs.first()) {
                let vis = class_def
                    .method_visibility
                    .get(field)
                    .copied()
                    .unwrap_or(HirFieldVisibility::Public);
                if !static_field_visible_for_read(cx, cn.as_str(), vis) {
                    return Ok((Value::Null, None));
                }
                return Ok((v.clone(), Some(cn)));
            }
            if class_def.instance_fields.iter().any(|n| n == field) {
                let fv = rc
                    .borrow()
                    .fields
                    .get(field)
                    .cloned()
                    .unwrap_or(Value::Null);
                return Ok((fv, Some(cn)));
            }
            cursor = class_def.extends.clone();
        } else {
            break;
        }
    }
    Ok((Value::Null, None))
}

pub(super) fn enforce_instance_field_visibility(
    cx: &InterpCx,
    rc: &Rc<RefCell<InstanceData>>,
    field: &str,
) -> Result<(), InterpretError> {
    if field == "class" {
        return Ok(());
    }
    let cn = rc.borrow().class_name.clone();
    let Some(def) = cx.classes.get(&cn) else {
        return Ok(());
    };
    if !def.instance_fields.iter().any(|f| f == field) {
        return Ok(());
    }
    let Some(decl) = def.field_decl_class.get(field) else {
        return Ok(());
    };
    let Some(vis) = def.field_visibility.get(field) else {
        return Ok(());
    };
    match vis {
        HirFieldVisibility::Public => Ok(()),
        HirFieldVisibility::Private => {
            let enc = cx.enclosing_class_stack.last().map(|s| s.as_str());
            if enc != Some(decl.as_str()) {
                return Err(InterpretError::private_field());
            }
            Ok(())
        }
        HirFieldVisibility::Protected => {
            let enc = cx.enclosing_class_stack.last().map(|s| s.as_str());
            if enc == Some(decl.as_str()) {
                return Ok(());
            }
            if enc.is_some_and(|e| is_subclass_of(cx, e, decl)) {
                return Ok(());
            }
            if cx.strict == Some(true) {
                return Err(InterpretError::protected_field());
            }
            if enc.is_none() {
                return Ok(());
            }
            Err(InterpretError::protected_field())
        }
    }
}

fn static_field_visible_for_read(cx: &InterpCx, decl_class: &str, vis: HirFieldVisibility) -> bool {
    let enc = cx.enclosing_class_stack.last().map(|s| s.as_str());
    match vis {
        HirFieldVisibility::Public => true,
        HirFieldVisibility::Private => enc == Some(decl_class),
        HirFieldVisibility::Protected => {
            enc.is_some_and(|e| e == decl_class || is_subclass_of(cx, e, decl_class))
        }
    }
}

/// Class that declares `static_methods[method]`, walking `extends` from `start_class`.
/// Static method **calls** use Java-style errors when `protected` / `private` is not visible
/// (unlike static field reads, which yield `null` for inaccessible `protected` fields).
pub(super) fn enforce_static_method_call_visibility(
    cx: &InterpCx,
    owner_class: &str,
    method: &str,
) -> Result<(), InterpretError> {
    let Some(def) = cx.classes.get(owner_class) else {
        return Ok(());
    };
    if !def.static_methods.contains_key(method) {
        return Ok(());
    }
    let vis = def
        .static_method_visibility
        .get(method)
        .copied()
        .unwrap_or(HirFieldVisibility::Public);
    if static_field_visible_for_read(cx, owner_class, vis) {
        return Ok(());
    }
    Err(match vis {
        HirFieldVisibility::Protected => InterpretError::protected_static_method(),
        HirFieldVisibility::Private => InterpretError::private_static_method(),
        HirFieldVisibility::Public => InterpretError::not_callable(),
    })
}

pub(super) fn resolve_static_method_owner(
    cx: &InterpCx,
    start_class: &str,
    method: &str,
) -> Option<String> {
    let mut cur: Option<&str> = Some(start_class);
    let mut seen = HashSet::new();
    for _ in 0..64 {
        let cn = cur?;
        if !seen.insert(cn.to_string()) {
            break;
        }
        let def = cx.classes.get(cn)?;
        if def.static_methods.contains_key(method) {
            return Some(cn.to_string());
        }
        cur = def.extends.as_deref();
    }
    None
}

/// Class that owns `static_fields[field]`, walking `extends` from `start_class`.
pub(super) fn resolve_static_field_owner(
    cx: &InterpCx,
    start_class: &str,
    field: &str,
) -> Option<String> {
    let mut cur: Option<&str> = Some(start_class);
    let mut seen = HashSet::new();
    for _ in 0..64 {
        let cn = cur?;
        if !seen.insert(cn.to_string()) {
            break;
        }
        let def = cx.classes.get(cn)?;
        if def.static_fields.contains_key(field) {
            return Some(cn.to_string());
        }
        cur = def.extends.as_deref();
    }
    None
}

/// `None` if no static field in the hierarchy; `Some(Null)` if present but not visible.
pub(super) fn read_visible_class_static_field(
    cx: &InterpCx,
    start_class: &str,
    field: &str,
) -> Option<Value> {
    let owner = resolve_static_field_owner(cx, start_class, field)?;
    let def = cx.classes.get(owner.as_str())?;
    let decl = def
        .static_field_decl_class
        .get(field)
        .map(|s| s.as_str())
        .unwrap_or(owner.as_str());
    let vis = def
        .static_field_visibility
        .get(field)
        .copied()
        .unwrap_or(HirFieldVisibility::Public);
    if static_field_visible_for_read(cx, decl, vis) {
        return Some(def.static_fields.get(field)?.clone());
    }
    Some(Value::Null)
}

pub(super) fn enforce_constructor_callable(
    cx: &InterpCx,
    constructed_class: &str,
) -> Result<(), InterpretError> {
    let Some(def) = cx.classes.get(constructed_class) else {
        return Ok(());
    };
    let Some(vis) = def.method_visibility.get("constructor").copied() else {
        return Ok(());
    };
    if static_field_visible_for_read(cx, constructed_class, vis) {
        return Ok(());
    }
    Err(match vis {
        HirFieldVisibility::Protected => InterpretError::protected_constructor(),
        HirFieldVisibility::Private => InterpretError::private_constructor(),
        HirFieldVisibility::Public => InterpretError::invalid_constructor(
            constructed_class,
            "constructor is not accessible from this context",
        ),
    })
}

pub(super) fn enforce_static_field_visibility(
    cx: &InterpCx,
    owner_class: &str,
    field: &str,
) -> Result<(), InterpretError> {
    let Some(def) = cx.classes.get(owner_class) else {
        return Ok(());
    };
    if !def.static_fields.contains_key(field) {
        return Ok(());
    }
    let Some(decl) = def.static_field_decl_class.get(field) else {
        return Ok(());
    };
    let Some(vis) = def.static_field_visibility.get(field) else {
        return Ok(());
    };
    match vis {
        HirFieldVisibility::Public => Ok(()),
        HirFieldVisibility::Private => {
            let enc = cx.enclosing_class_stack.last().map(|s| s.as_str());
            if enc != Some(decl.as_str()) {
                return Err(InterpretError::private_field());
            }
            Ok(())
        }
        HirFieldVisibility::Protected => {
            let enc = cx.enclosing_class_stack.last().map(|s| s.as_str());
            if enc == Some(decl.as_str()) {
                return Ok(());
            }
            if enc.is_some_and(|e| is_subclass_of(cx, e, decl)) {
                return Ok(());
            }
            Err(InterpretError::protected_field())
        }
    }
}
