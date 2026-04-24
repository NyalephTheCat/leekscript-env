//! Shared helpers: intervals, `for`-`in`, truthiness, map key lookup.

use super::error::{ExecAbort, InterpretError};
use super::value::{IntervalValue, Value};
use leekscript_hir::HirAssignOp;
use std::collections::HashSet;
use std::rc::Rc;

/// Leek **v1** passes array / map / set arguments **by copy** unless the parameter is `@`.
/// From **v2** onward, containers are shared like Java reference types.
/// Coerce a value for a declared type (`integer`, `real`, …).
///
/// Nullable types (`real?`, `integer?`, …) preserve `null` (do not coerce it to `0`).
pub(super) fn coerce_var_init_value(
    v: Value,
    decl_ty: Option<&str>,
    language_version: u8,
) -> Result<Value, InterpretError> {
    let Some(raw) = decl_ty else {
        return Ok(v);
    };
    let trimmed = raw.trim();
    if trimmed.ends_with('?') && matches!(v, Value::Null) {
        return Ok(Value::Null);
    }
    let ty = trimmed.trim_end_matches('?');
    match ty {
        "integer" | "int" | "long" => match v {
            Value::Integer(i) => Ok(Value::Integer(i)),
            Value::Real(r) => Ok(Value::Integer(r as i64)),
            Value::Bool(b) => Ok(Value::Integer(if b { 1 } else { 0 })),
            Value::Null if language_version == 1 => Err(InterpretError {
                reference: "IMPOSSIBLE_CAST",
                message: "impossible cast to integer".into(),
            }),
            _ => Ok(Value::Integer(0)),
        },
        "real" | "double" => match v {
            // Java v1: typed `real` bindings can still hold integral values without forcing `.0` everywhere.
            Value::Integer(i) if language_version == 1 => Ok(Value::Integer(i)),
            Value::Integer(i) => Ok(Value::Real(i as f64)),
            Value::Real(r) => Ok(Value::Real(r)),
            Value::Bool(b) => Ok(Value::Real(if b { 1.0 } else { 0.0 })),
            Value::Null if language_version == 1 => Err(InterpretError {
                reference: "IMPOSSIBLE_CAST",
                message: "impossible cast to real".into(),
            }),
            _ => Ok(Value::Real(0.0)),
        },
        "boolean" | "bool" => Ok(Value::Bool(value_truthy(&v))),
        _ => Ok(v),
    }
}

pub(super) fn pass_parameter_value(language_version: u8, arg: Value, by_ref: bool) -> Value {
    if by_ref || language_version >= 2 {
        return arg;
    }
    match arg {
        Value::Array(a) => Value::array_from(a.borrow().clone()),
        Value::Map(m) => Value::map_from(m.borrow().to_vec()),
        Value::Object(m) => Value::object_from(m.borrow().to_vec()),
        Value::Set(s) => Value::set_from(s.borrow().elems.clone()),
        other => other,
    }
}

pub(super) fn numeric_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Real(r) | Value::RealDotZero(r) => Some(*r),
        _ => None,
    }
}

fn interval_size_java(iv: &IntervalValue) -> f64 {
    // Java reference:
    // - IntegerIntervalLeekValue.intervalSize(): unbounded → Long.MAX_VALUE; bounded → from - to
    // - RealIntervalLeekValue.intervalSize(): unbounded → +Infinity; bounded → from - to
    //
    // Our runtime stores endpoints as f64 with infinities for unbounded sugar.
    if !iv.min.is_finite() || !iv.max.is_finite() {
        return f64::INFINITY;
    }
    iv.min - iv.max
}

pub(super) fn java_longint(v: &Value) -> i64 {
    match v {
        Value::Integer(i) => *i,
        Value::Real(r) | Value::RealDotZero(r) => *r as i64,
        Value::Bool(b) => {
            if *b {
                1
            } else {
                0
            }
        }
        Value::Null => 0,
        Value::String(s) => {
            if s == "true" {
                return 1;
            }
            if s == "false" {
                return 0;
            }
            if s.is_empty() {
                return 0;
            }
            if let Ok(i) = s.parse::<i64>() {
                return i;
            }
            s.len() as i64
        }
        Value::Array(a) => a.borrow().len() as i64,
        Value::Map(m) | Value::Object(m) => m.borrow().len() as i64,
        Value::Set(s) => s.borrow().elems.len() as i64,
        Value::Interval(iv) => {
            let sz = interval_size_java(iv);
            if sz.is_infinite() {
                i64::MAX
            } else {
                sz as i64
            }
        }
        Value::Function(_) | Value::Native(_) | Value::UserClass(_) => 0,
        Value::Instance(rc) => rc.borrow().fields.len() as i64,
        Value::Super => 0,
    }
}

pub(super) fn java_real(v: &Value) -> f64 {
    match v {
        Value::Integer(i) => *i as f64,
        Value::Real(r) | Value::RealDotZero(r) => *r,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Null => 0.0,
        Value::String(s) => {
            if s == "true" {
                return 1.0;
            }
            if s == "false" {
                return 0.0;
            }
            if s.is_empty() {
                return 0.0;
            }
            if let Ok(f) = s.parse::<f64>() {
                return f;
            }
            s.len() as f64
        }
        Value::Array(a) => a.borrow().len() as f64,
        Value::Map(m) | Value::Object(m) => m.borrow().len() as f64,
        Value::Set(s) => s.borrow().elems.len() as f64,
        Value::Interval(iv) => interval_size_java(iv),
        Value::Function(_) | Value::Native(_) | Value::UserClass(_) => 0.0,
        Value::Instance(rc) => rc.borrow().fields.len() as f64,
        Value::Super => 0.0,
    }
}

/// Java weak numeric coercion for operand checks (`+ - * / %` fast-path / legacy equality rules):
/// `true`→1, `false`→0, `null`→0. (Notably does **not** coerce strings/containers.)
pub(super) fn arithmetic_operand_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Real(r) | Value::RealDotZero(r) => Some(*r),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Null => Some(0.0),
        _ => None,
    }
}

/// Operands that are not [`Value::Real`]; used to pick Java `integer` vs `real` division/export.
pub(super) fn integral_promotable_operand(v: &Value) -> bool {
    matches!(v, Value::Integer(_) | Value::Bool(_) | Value::Null)
}

pub(super) fn map_find_key(m: &super::map_store::MapStore, k: &Value) -> Option<usize> {
    m.find_key(k)
}

/// v1–v3 map index / `removeKey`: `m[5.7]` reads key `5`; `removeKey(m, 12.12)` removes `12`.
pub(super) fn map_find_key_legacy(m: &super::map_store::MapStore, k: &Value) -> Option<usize> {
    m.find_key_legacy(k)
}

pub(super) fn map_stored_key_matches_legacy_query(stored: &Value, query: &Value) -> bool {
    values_equal_for_compare(stored, query)
        || matches!(
            (stored, query),
            (Value::Integer(i), Value::Real(r)) if r.is_finite() && *i == r.trunc() as i64
        )
        || matches!(
            (stored, query),
            (Value::Real(r), Value::Integer(i)) if r.is_finite() && r.fract() == 0.0 && *i == *r as i64
        )
}

pub(super) fn interval_is_empty(iv: &IntervalValue) -> bool {
    if iv.max < iv.min {
        return true;
    }
    if iv.min == iv.max {
        return !(iv.min_closed && iv.max_closed);
    }
    false
}

fn interval_is_bounded(iv: &IntervalValue) -> bool {
    iv.min.is_finite() && iv.max.is_finite()
}

fn interval_discrete_value(iv: &IntervalValue, step: f64, x: f64) -> Value {
    let unit = (step - 1.0).abs() < f64::EPSILON;
    // Non-unit *fractional* step (e.g. `0.8`) keeps `real` elements even on an integer lattice.
    let fractional_step = iv.integer_lattice && !unit && step.fract() != 0.0;
    if iv.integer_lattice
        && !fractional_step
        && x.is_finite()
        && x.fract() == 0.0
        && x >= i64::MIN as f64
        && x <= i64::MAX as f64
    {
        Value::Integer(x as i64)
    } else {
        Value::Real(x)
    }
}

/// Java `IntervalIterator`: index + value, step `+1` from first included point.
pub(super) fn interval_kv_pairs(iv: &IntervalValue) -> Result<Vec<(Value, Value)>, InterpretError> {
    if !interval_is_bounded(iv) {
        return Err(InterpretError::cannot_iterate_unbounded_interval());
    }
    if interval_is_empty(iv) {
        return Ok(Vec::new());
    }
    let mut x = if iv.min_closed { iv.min } else { iv.min + 1.0 };
    let mut i = 0.0f64;
    let mut out = Vec::new();
    loop {
        let in_range = if iv.max_closed {
            x <= iv.max
        } else {
            x < iv.max
        };
        if !in_range {
            break;
        }
        let elem = interval_discrete_value(iv, 1.0, x);
        out.push((Value::Integer(i as i64), elem));
        i += 1.0;
        x += 1.0;
    }
    Ok(out)
}

fn interval_stepped_values(iv: &IntervalValue, step: f64) -> Result<Vec<Value>, InterpretError> {
    debug_assert!(step > 0.0);
    let first = if iv.min_closed { iv.min } else { iv.min + step };
    let mut x = first;
    let mut out = Vec::new();
    loop {
        let in_range = if iv.max_closed {
            x <= iv.max
        } else {
            x < iv.max
        };
        if !in_range {
            break;
        }
        out.push(interval_discrete_value(iv, step, x));
        x += step;
    }
    Ok(out)
}

fn interval_stepped_values_rev(
    iv: &IntervalValue,
    step: f64,
) -> Result<Vec<Value>, InterpretError> {
    debug_assert!(step < 0.0);
    let s = -step;
    let first = if iv.max_closed { iv.max } else { iv.max - s };
    let mut x = first;
    let mut out = Vec::new();
    loop {
        let in_range = if iv.min_closed {
            x >= iv.min
        } else {
            x > iv.min
        };
        if !in_range {
            break;
        }
        out.push(interval_discrete_value(iv, step, x));
        x += step;
    }
    Ok(out)
}

/// `intervalToArray` / `intervalToSet`: `None` if unbounded (`null` in Java), `Some([])` if empty.
pub(super) fn interval_array_values(
    iv: &IntervalValue,
    step: Option<f64>,
) -> Result<Option<Vec<Value>>, InterpretError> {
    let step_f = match step {
        None => 1.0,
        Some(s) => {
            if !s.is_finite() || s == 0.0 {
                return Err(InterpretError::wrong_operand_types_binary());
            }
            s
        }
    };
    if !interval_is_bounded(iv) {
        return Ok(None);
    }
    if interval_is_empty(iv) {
        return Ok(Some(Vec::new()));
    }
    if (step_f - 1.0).abs() < f64::EPSILON {
        let pairs = interval_kv_pairs(iv)?;
        return Ok(Some(pairs.into_iter().map(|(_, v)| v).collect()));
    }
    if step_f < 0.0 {
        return Ok(Some(interval_stepped_values_rev(iv, step_f)?));
    }
    Ok(Some(interval_stepped_values(iv, step_f)?))
}

pub(super) fn value_to_for_in_sequence(v: Value) -> Result<Vec<Value>, InterpretError> {
    match v {
        Value::Array(a) => Ok(a.borrow().clone()),
        Value::Map(m) | Value::Object(m) => Ok(m.borrow().iter().map(|(_, v)| v.clone()).collect()),
        Value::Set(s) => Ok(s.borrow().elems.clone()),
        Value::Interval(iv) => Ok(interval_kv_pairs(&iv)?
            .into_iter()
            .map(|(_, x)| x)
            .collect()),
        _ => Err(InterpretError::not_iterable()),
    }
}

pub(super) fn value_to_for_in_key_value_pairs(
    v: Value,
) -> Result<Vec<(Value, Value)>, InterpretError> {
    match v {
        Value::Array(a) => Ok(a
            .borrow()
            .iter()
            .enumerate()
            .map(|(i, x)| (Value::Integer(i as i64), x.clone()))
            .collect()),
        Value::Map(m) | Value::Object(m) => Ok(m.borrow().to_vec()),
        Value::Set(s) => Ok(s
            .borrow()
            .elems
            .iter()
            .enumerate()
            .map(|(i, x)| (Value::Integer(i as i64), x.clone()))
            .collect()),
        Value::Interval(iv) => interval_kv_pairs(&iv),
        _ => Err(InterpretError::not_iterable()),
    }
}

pub(super) fn value_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Integer(i) => *i != 0,
        Value::Real(n) | Value::RealDotZero(n) => *n != 0.0 && !n.is_nan(),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.borrow().is_empty(),
        Value::Map(m) | Value::Object(m) => !m.borrow().is_empty(),
        Value::Set(s) => !s.borrow().elems.is_empty(),
        Value::Interval(iv) => !interval_is_empty(iv),
        Value::Function(_) | Value::Native(_) => true,
        Value::Instance(_) | Value::UserClass(_) | Value::Super => true,
    }
}

fn interval_contains_infinite(x: f64, iv: &IntervalValue) -> bool {
    if x.is_sign_positive() {
        iv.max.is_infinite() && iv.max.is_sign_positive()
    } else {
        iv.min.is_infinite() && iv.min.is_sign_negative()
    }
}

fn interval_contains_scalar(x: f64, iv: &IntervalValue) -> bool {
    if !x.is_finite() || interval_is_empty(iv) {
        return false;
    }
    let ge_min = if iv.min.is_finite() {
        if iv.min_closed {
            x >= iv.min
        } else {
            x > iv.min
        }
    } else if iv.min.is_sign_negative() {
        true
    } else {
        false
    };
    let le_max = if iv.max.is_finite() {
        if iv.max_closed {
            x <= iv.max
        } else {
            x < iv.max
        }
    } else if iv.max.is_sign_positive() {
        true
    } else {
        false
    };
    ge_min && le_max
}

pub(super) fn eval_in(left: Value, right: Value) -> Result<Value, InterpretError> {
    match right {
        Value::Array(arr) => Ok(Value::Bool(
            arr.borrow()
                .iter()
                .any(|x| values_equal_for_compare(x, &left)),
        )),
        Value::Map(m) | Value::Object(m) => {
            Ok(Value::Bool(map_find_key(&m.borrow(), &left).is_some()))
        }
        Value::Set(s) => Ok(Value::Bool(
            s.borrow()
                .elems
                .iter()
                .any(|x| values_equal_for_compare(x, &left)),
        )),
        Value::Interval(iv) => {
            if let Value::Real(x) = &left {
                if x.is_infinite() {
                    return Ok(Value::Bool(interval_contains_infinite(*x, &iv)));
                }
            }
            let Some(x) = numeric_as_f64(&left) else {
                return Ok(Value::Bool(false));
            };
            Ok(Value::Bool(interval_contains_scalar(x, &iv)))
        }
        _ => Err(InterpretError::in_operator_requires_container()),
    }
}

pub(super) fn values_equal_for_compare(a: &Value, b: &Value) -> bool {
    values_equal_for_compare_inner(a, b, &mut HashSet::new())
}

/// Deep `==` compatible with Java Leek (numeric coercion for `integer`/`real`, structural containers).
/// `active` breaks Rc cycles (and deep paths that would otherwise blow the stack).
fn values_equal_for_compare_inner(
    a: &Value,
    b: &Value,
    active: &mut HashSet<(usize, usize)>,
) -> bool {
    use Value::*;
    match (a, b) {
        (Integer(x), Integer(y)) => x == y,
        (Real(x), Real(y)) => x == y,
        (String(s), String(t)) => s == t,
        (Bool(x), Bool(y)) => x == y,
        (Null, Null) => true,
        (Native(na), Native(nb)) => na == nb,
        (Instance(ia), Instance(ib)) => Rc::ptr_eq(ia, ib),
        (Array(a), Array(b)) => {
            if Rc::ptr_eq(a, b) {
                return true;
            }
            let pa = Rc::as_ptr(a) as usize;
            let pb = Rc::as_ptr(b) as usize;
            if !active.insert((pa, pb)) {
                return true;
            }
            let ab = a.borrow();
            let bb = b.borrow();
            if ab.len() != bb.len() {
                return false;
            }
            for (av, bv) in ab.iter().zip(bb.iter()) {
                if !values_equal_for_compare_inner(av, bv, active) {
                    return false;
                }
            }
            true
        }
        (Map(a), Map(b)) => {
            if Rc::ptr_eq(a, b) {
                return true;
            }
            let pa = Rc::as_ptr(a) as usize;
            let pb = Rc::as_ptr(b) as usize;
            if !active.insert((pa, pb)) {
                // Already comparing these two nodes; assume equal to break cycles.
                return true;
            }
            let ab = a.borrow();
            let bb = b.borrow();
            if ab.len() != bb.len() {
                return false;
            }
            for (k, av) in ab.iter() {
                let Some((_, bv)) = bb
                    .iter()
                    .find(|(kk, _)| values_equal_for_compare_inner(kk, k, active))
                else {
                    return false;
                };
                if !values_equal_for_compare_inner(av, bv, active) {
                    return false;
                }
            }
            true
        }
        (Object(a), Object(b)) => Rc::ptr_eq(a, b),
        (Set(a), Set(b)) => {
            if Rc::ptr_eq(a, b) {
                return true;
            }
            let pa = Rc::as_ptr(a) as usize;
            let pb = Rc::as_ptr(b) as usize;
            if !active.insert((pa, pb)) {
                return true;
            }
            let ab = a.borrow();
            let bb = b.borrow();
            if ab.elems.len() != bb.elems.len() {
                return false;
            }
            // Order-insensitive structural equality.
            for x in ab.elems.iter() {
                if !bb
                    .elems
                    .iter()
                    .any(|y| values_equal_for_compare_inner(x, y, active))
                {
                    return false;
                }
            }
            true
        }
        (Interval(ia), Interval(ib)) => ia == ib,
        (Function(a), Function(b)) => Rc::ptr_eq(a, b),
        _ => {
            let (Some(af), Some(bf)) = (numeric_as_f64(a), numeric_as_f64(b)) else {
                return false;
            };
            !af.is_nan() && !bf.is_nan() && af == bf
        }
    }
}

pub(super) fn value_as_array_index_i64(v: &Value) -> Result<i64, ExecAbort> {
    match v {
        Value::Integer(n) => Ok(*n),
        Value::Real(r) if r.is_finite() && r.fract() == 0.0 => Ok(*r as i64),
        _ => Err(InterpretError::array_index_out_of_bounds().into()),
    }
}

fn index_i64_for_read(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(n) => Some(*n),
        Value::Real(r) if r.is_finite() => Some(r.trunc() as i64),
        Value::Bool(b) => Some(if *b { 1 } else { 0 }),
        Value::Null => Some(0),
        _ => None,
    }
}

/// Java `ArrayLeekValue.get`: OOB / empty / non-numeric index → `null` (unlike strict assign indexing).
/// v1–v3: index `== len` reads the last element **only** when that element is an `Array` (`TestArray` `misc`);
/// otherwise OOB stays `null` (`TestArray` `testOperator_on_unknown_arrays`, `testOut_of_bounds_exception`).
pub(super) fn array_index_for_read(
    key: &Value,
    language_version: u8,
    buf: &[Value],
) -> Option<usize> {
    let len = buf.len();
    if len == 0 {
        return None;
    }
    let i = index_i64_for_read(key)?;
    let len_i = len as i64;
    let mut j = if i < 0 { i + len_i } else { i };
    if j < 0 {
        return None;
    }
    if j >= len_i {
        if language_version <= 4 && j == len_i && matches!(buf.last(), Some(Value::Array(_))) {
            j = len_i - 1;
        } else {
            return None;
        }
    }
    Some(j as usize)
}

/// Single-element index: matches `ArrayLeekValue.get(long)` — negative indices count from the end,
/// then the index must lie in `[0, len)`.
pub(super) fn array_index_at(key: &Value, len: usize) -> Result<usize, ExecAbort> {
    if len == 0 {
        return Err(InterpretError::array_index_out_of_bounds().into());
    }
    let i = value_as_array_index_i64(key)?;
    let len_i = len as i64;
    let j = if i < 0 { i + len_i } else { i };
    if j < 0 || j >= len_i {
        return Err(InterpretError::array_index_out_of_bounds().into());
    }
    Ok(j as usize)
}

/// Index for assigning into an array cell. v1–v3: indices `>= len` grow the array with `null`.
/// v4+: plain `=` past the end errors; compound ops (`+=`, …) past the end are a silent no-op
/// ([`None`]).
pub(super) fn array_cell_index_for_assign(
    buf: &mut Vec<Value>,
    key: &Value,
    _op: HirAssignOp,
    language_version: u8,
    strict: Option<bool>,
) -> Result<Option<usize>, ExecAbort> {
    let i = value_as_array_index_i64(key)?;
    let len = buf.len();
    let len_i = len as i64;
    let j = if i < 0 { i + len_i } else { i };
    if j < 0 {
        if language_version >= 4 {
            if strict == Some(true) {
                return Err(InterpretError::array_out_of_bound_strict().into());
            }
            return Ok(None);
        }
        return Err(InterpretError::array_index_out_of_bounds().into());
    }
    let j = j as usize;
    if j >= len {
        if language_version >= 4 {
            if strict == Some(true) {
                return Err(InterpretError::array_out_of_bound_strict().into());
            }
            return Ok(None);
        }
        buf.resize(j + 1, Value::Null);
    }
    Ok(Some(j))
}

/// JVM `ArrayLeekValue.arraySlice` start when `stride > 0`: optional bound, `+len` if negative, then `max(0, x)` only.
pub(super) fn pos_slice_start(key: Option<&Value>, len: usize) -> Result<i64, ExecAbort> {
    let len_i = len as i64;
    Ok(match key {
        None => 0,
        Some(Value::Null) => 0,
        Some(v) => {
            let raw = value_as_array_index_i64(v)?;
            let x = if raw < 0 { raw + len_i } else { raw };
            x.max(0)
        }
    })
}

/// JVM `arraySlice` end when `stride > 0`: default `len`, `+len` if negative, then `min(len, x)` only (may stay `<0`).
pub(super) fn pos_slice_end(key: Option<&Value>, len: usize) -> Result<i64, ExecAbort> {
    let len_i = len as i64;
    Ok(match key {
        None => len_i,
        Some(Value::Null) => len_i,
        Some(v) => {
            let raw = value_as_array_index_i64(v)?;
            let x = if raw < 0 { raw + len_i } else { raw };
            x.min(len_i)
        }
    })
}

/// JVM `arraySlice` start when `stride < 0`: default `len - 1`, else normalize then `min(len - 1, x)`.
pub(super) fn neg_slice_start(key: Option<&Value>, len: usize) -> Result<i64, ExecAbort> {
    if len == 0 {
        return Ok(-1);
    }
    let len_i = len as i64;
    Ok(match key {
        None => len_i - 1,
        Some(v) => {
            let raw = value_as_array_index_i64(v)?;
            let j = if raw < 0 { raw + len_i } else { raw };
            j.clamp(0, len_i - 1)
        }
    })
}

/// JVM `arraySlice` end when `stride < 0`: default `-1`, else normalize then `max(-1, x)`.
pub(super) fn neg_slice_end(key: Option<&Value>, len: usize) -> Result<i64, ExecAbort> {
    let len_i = len as i64;
    Ok(match key {
        None => -1,
        Some(v) => {
            let raw = value_as_array_index_i64(v)?;
            let j = if raw < 0 { raw + len_i } else { raw };
            j.clamp(-1, len_i - 1)
        }
    })
}

/// JVM `String.hashCode` for ASCII / UTF-8 BMP text (matches Java for Leek string literals in the suite).
pub(super) fn java_string_hash_code(s: &str) -> i32 {
    let mut h: i32 = 0;
    for &b in s.as_bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as i32);
    }
    h
}

/// Java `Long.hashCode` / `Double.hashCode` (boxed numeric keys in Leek).
fn leek_java_hash_code(v: &Value) -> i32 {
    match v {
        Value::Integer(i) => {
            let x = *i as i64;
            (x ^ (x >> 32)) as i32
        }
        Value::Real(r) => {
            let bits = r.to_bits() as i64;
            (bits ^ (bits >> 32)) as i32
        }
        Value::String(s) => java_string_hash_code(s),
        _ => 0,
    }
}

#[inline]
fn java_hash_spread(h: i32) -> usize {
    let u = h as u32;
    (u ^ (u >> 16)) as usize
}

/// JDK8-style `HashSet` iteration order: insert with separate chaining + 0.75 load factor resize,
/// then walk bins `0..cap-1` in list insertion order (matches `java_vm_suite` `intervalToSet` v4).
pub(super) fn java_hashset_iteration_order(insertion_order: Vec<Value>) -> Vec<Value> {
    let mut cap = 16usize;
    let mut buckets: Vec<Vec<Value>> = vec![Vec::new(); cap];
    let mut size = 0usize;

    for k in insertion_order {
        let h = leek_java_hash_code(&k);
        let idx = java_hash_spread(h) & (cap - 1);
        if buckets[idx].iter().any(|x| values_equal_for_compare(x, &k)) {
            continue;
        }
        buckets[idx].push(k);
        size += 1;
        let threshold = ((cap as f64) * 0.75).floor() as usize;
        if size > threshold {
            let old = std::mem::replace(&mut buckets, vec![Vec::new(); cap * 2]);
            cap *= 2;
            size = 0;
            for bin in old {
                for k2 in bin {
                    let h2 = leek_java_hash_code(&k2);
                    let j = java_hash_spread(h2) & (cap - 1);
                    buckets[j].push(k2);
                    size += 1;
                }
            }
        }
    }

    let mut out = Vec::with_capacity(size);
    for bin in buckets {
        for k in bin {
            out.push(k);
        }
    }
    out
}
