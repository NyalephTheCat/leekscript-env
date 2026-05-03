//! Binary/unary numeric operations, equality, ordering.

use super::error::InterpretError;
use super::java_export::value_java_string_coerce;
use super::native::number_from_value;
use super::util::{
    arithmetic_operand_as_f64, eval_in, integral_promotable_operand, java_longint, java_real,
    map_find_key, numeric_as_f64, value_truthy, values_equal_for_compare,
};
use super::value::{IntervalValue, Value};
use leekscript_hir::HirBinOp;

pub(super) fn eval_interval_endpoints(
    min_closed: bool,
    min_v: Value,
    max_closed: bool,
    max_v: Value,
    interval_min_neg_inf_from_shorthand: bool,
    interval_max_pos_inf_from_shorthand: bool,
) -> Result<IntervalValue, InterpretError> {
    let min = match min_v {
        Value::Integer(i) => i as f64,
        Value::Real(n) if n.is_finite() || n.is_infinite() => n,
        _ => {
            return Err(InterpretError::invalid_constructor(
                "Interval",
                "endpoints must be numbers",
            ));
        }
    };
    let max = match max_v {
        Value::Integer(i) => i as f64,
        Value::Real(n) if n.is_finite() || n.is_infinite() => n,
        _ => {
            return Err(InterpretError::invalid_constructor(
                "Interval",
                "endpoints must be numbers",
            ));
        }
    };
    // Integer lattice: no finite `real` endpoint (`[1.0..2]` is not a lattice; `[1..2]` and `[1..∞[` are).
    let integer_lattice = !matches!(min_v, Value::Real(r) if r.is_finite())
        && !matches!(max_v, Value::Real(r) if r.is_finite());
    Ok(IntervalValue {
        min_closed,
        min,
        max_closed,
        max,
        integer_lattice,
        export_endpoints_as_real: false,
        interval_min_neg_inf_from_shorthand,
        interval_max_pos_inf_from_shorthand,
    })
}

pub(super) fn instanceof_leek_type(v: &Value, t: &str, language_version: u8) -> bool {
    if matches!(v, Value::Null) {
        return language_version >= 3 && matches!(t, "Null");
    }
    match t {
        // Java v1–v3: bracket map/array literals use `LegacyLeekArray`; `instanceof` treats both maps and lists.
        "LegacyLeekArray" if language_version < 4 => {
            matches!(v, Value::Map(..) | Value::Array(_))
        }
        "LegacyLeekArray" => false,
        "Array" => {
            matches!(v, Value::Array(_))
                || matches!(v, Value::Instance(rc) if rc.borrow().extends.as_deref() == Some("Array"))
        }
        "string" | "String" => matches!(v, Value::String(_)),
        // Java `Class` metaclass values.
        "Class" => matches!(
            v,
            Value::Instance(_) | Value::Native(_) | Value::UserClass(_)
        ),
        "integer" | "int" | "Integer" => matches!(v, Value::Integer(_)),
        "real" | "float" | "double" | "Real" | "Number" => {
            matches!(v, Value::Real(_) | Value::Integer(_))
        }
        "boolean" | "Boolean" => matches!(v, Value::Bool(_)),
        "Function" => matches!(v, Value::Function(_) | Value::Native(_)),
        "Map" => matches!(v, Value::Map(..)),
        "Object" => matches!(v, Value::Object(..)),
        "Set" => matches!(v, Value::Set(_)),
        "Interval" => matches!(v, Value::Interval(_)),
        _ => {
            if let Value::Instance(rc) = v {
                rc.borrow().class_name == t
            } else {
                false
            }
        }
    }
}

fn peel_singleton_array(mut v: Value) -> Value {
    loop {
        let inner = if let Value::Array(a) = &v {
            let b = a.borrow();
            if b.len() == 1 {
                Some(b[0].clone())
            } else {
                None
            }
        } else {
            None
        };
        match inner {
            Some(next) => v = next,
            None => break,
        }
    }
    v
}

pub(super) fn eval_binary(
    op: HirBinOp,
    left: Value,
    right: Value,
    language_version: u8,
) -> Result<Value, InterpretError> {
    use HirBinOp::{
        Add, BitAnd, BitOr, BitXor, Div, Eq, Ge, Gt, In, Instanceof, IntDiv, Le, LogicalAnd,
        LogicalOr, Lt, Mul, Ne, NotIn, NullishCoalesce, Pow, Rem, Shl, Shr, StrictEq, StrictNe,
        Sub, UShr,
    };
    match op {
        Add | Sub | Mul | Div | Rem => eval_arithmetic(op, left, right, language_version),
        Pow => eval_pow(left, right, language_version),
        IntDiv => eval_intdiv(left, right),
        Eq | Ne | StrictEq | StrictNe => eval_equality(op, left, right, language_version),
        Lt | Le | Gt | Ge => eval_ordering(op, left, right),
        BitAnd | BitOr | BitXor => eval_bitwise_int(op, left, right),
        Shl | Shr | UShr => eval_shift(op, left, right),
        NotIn => eval_not_in(left, right),
        LogicalAnd | LogicalOr | NullishCoalesce => {
            unreachable!("evaluated via short-circuit in eval_expr")
        }
        Instanceof => unreachable!("evaluated via eval_instanceof"),
        In => unreachable!("evaluated in eval_expr"),
    }
}

fn eval_not_in(left: Value, right: Value) -> Result<Value, InterpretError> {
    match eval_in(left, right)? {
        Value::Bool(b) => Ok(Value::Bool(!b)),
        _ => unreachable!(),
    }
}

fn bitwise_operand_i64(v: &Value) -> Result<i64, InterpretError> {
    Ok(java_longint(v))
}

fn eval_bitwise_int(op: HirBinOp, left: Value, right: Value) -> Result<Value, InterpretError> {
    use HirBinOp::{BitAnd, BitOr, BitXor};
    let a = bitwise_operand_i64(&left)?;
    let b = bitwise_operand_i64(&right)?;
    let o = match op {
        BitAnd => a & b,
        BitOr => a | b,
        BitXor => a ^ b,
        _ => unreachable!(),
    };
    Ok(Value::Integer(o))
}

fn eval_shift(op: HirBinOp, left: Value, right: Value) -> Result<Value, InterpretError> {
    use HirBinOp::{Shl, Shr, UShr};
    let a = bitwise_operand_i64(&left)?;
    let b = (bitwise_operand_i64(&right)? as u32) & 0x3f;
    let o = match op {
        Shl => a.wrapping_shl(b),
        Shr => a.wrapping_shr(b),
        UShr => ((a as u64) >> b) as i64,
        _ => unreachable!(),
    };
    Ok(Value::Integer(o))
}

pub(super) fn eval_bitxor(left: Value, right: Value) -> Result<Value, InterpretError> {
    use Value::Bool;
    match (&left, &right) {
        (Bool(a), Bool(b)) => Ok(Bool(a ^ b)),
        _ => eval_bitwise_int(HirBinOp::BitXor, left, right),
    }
}

pub(super) fn eval_bitnot(v: Value) -> Result<Value, InterpretError> {
    let a = bitwise_operand_i64(&v)?;
    Ok(Value::Integer(!a))
}

/// Java `AI.add`: string concat uses `string()`; arrays/maps merge/concat; otherwise numeric.
pub(super) fn eval_add(
    left: Value,
    right: Value,
    language_version: u8,
) -> Result<Value, InterpretError> {
    use Value::{Array, Integer, Map, Real, RealDotZero, String};
    match (&left, &right) {
        (Integer(a), Integer(b)) => Ok(Integer(a.wrapping_add(*b))),
        _ if matches!(left, String(_)) || matches!(right, String(_)) => {
            let a = value_java_string_coerce(&left, language_version);
            let b = value_java_string_coerce(&right, language_version);
            Ok(Value::String(a + &b))
        }
        (Array(a), Array(b)) => {
            let mut v = a.borrow().clone();
            v.extend(b.borrow().iter().cloned());
            Ok(Value::array_from(v))
        }
        (Map(m1), Map(m2)) => {
            let mut out = m1.borrow().clone();
            for (k, v) in m2.borrow().iter() {
                if map_find_key(&out, k).is_none() {
                    out.push_kv(k.clone(), v.clone());
                }
            }
            Ok(Value::wrap_keyed_pairs(&left, out.to_vec()))
        }
        (Array(a), y) => {
            let mut v = a.borrow().clone();
            v.push(y.clone());
            Ok(Value::array_from(v))
        }
        _ => {
            // Java reference `AI.add`:
            // - if any operand is `real`, compute as `real(x) + real(y)`
            // - otherwise compute as `longint(x) + longint(y)`
            let left_is_real = matches!(left, Real(_) | RealDotZero(_));
            let right_is_real = matches!(right, Real(_) | RealDotZero(_));
            if left_is_real || right_is_real {
                let x = java_real(&left) + java_real(&right);
                Ok(normalize_add_sub_mul_result(
                    left,
                    right,
                    x,
                    language_version,
                ))
            } else {
                Ok(Integer(
                    java_longint(&left).wrapping_add(java_longint(&right)),
                ))
            }
        }
    }
}

fn normalize_add_sub_mul_result(left: Value, right: Value, x: f64, language_version: u8) -> Value {
    use Value::{Integer, Real};
    let integral =
        x.is_finite() && x.fract() == 0.0 && x >= i64::MIN as f64 && x <= i64::MAX as f64;
    if language_version == 1 && integral {
        // v1: keep results as `real` when a `real` participated so export can use `doubleToString`.
        if matches!(left, Real(_)) || matches!(right, Real(_)) {
            Real(x)
        } else {
            Integer(x as i64)
        }
    } else if integral_promotable_operand(&left) && integral_promotable_operand(&right) && integral
    {
        Integer(x as i64)
    } else {
        Real(x)
    }
}

/// Java `pow`: `null` operands are treated like `0` (see `LeekExpression` `POWER` codegen).
fn pow_operand_as_f64(v: &Value) -> Result<f64, InterpretError> {
    use Value::{Bool, Integer, Null, Real};
    match v {
        Null => Ok(0.0),
        Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Integer(i) => Ok(*i as f64),
        Real(r) if r.is_finite() || r.is_infinite() => Ok(*r),
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

pub(super) fn eval_pow(
    left: Value,
    right: Value,
    _language_version: u8,
) -> Result<Value, InterpretError> {
    use Value::{Integer, Real};
    let a = pow_operand_as_f64(&left)?;
    let b = pow_operand_as_f64(&right)?;
    let x = a.powf(b);
    if x.is_nan() {
        return Err(InterpretError::wrong_operand_types_binary());
    }
    if x.is_infinite() {
        return Ok(Real(x));
    }
    // `**` keeps exact integers as `Integer` for export in v2+; use `pow(...)` for real promotion.
    if x.fract() == 0.0 && x >= i64::MIN as f64 && x <= i64::MAX as f64 {
        Ok(Integer(x as i64))
    } else {
        Ok(Real(x))
    }
}

fn trunc_i64_for_intdiv(v: &Value) -> Result<i64, InterpretError> {
    Ok(java_longint(v))
}

pub(super) fn eval_intdiv(left: Value, right: Value) -> Result<Value, InterpretError> {
    let ai = trunc_i64_for_intdiv(&left)?;
    let bi = trunc_i64_for_intdiv(&right)?;
    if bi == 0 {
        return Err(InterpretError::division_by_zero());
    }
    Ok(Value::Integer(ai / bi))
}

fn eval_arithmetic(
    op: HirBinOp,
    left: Value,
    right: Value,
    language_version: u8,
) -> Result<Value, InterpretError> {
    use HirBinOp::{Add, Div, Mul, Rem, Sub};
    use Value::{Integer, Null, Real, RealDotZero};
    if matches!(op, Add) {
        return eval_add(left, right, language_version);
    }
    if matches!(op, HirBinOp::Div) && (1..=4).contains(&language_version) {
        if let Value::Array(arr) = &left {
            let mut s = 0.0;
            for x in arr.borrow().iter() {
                s += number_from_value(x)?;
            }
            let den = number_from_value(&right)?;
            if den == 0.0 {
                return Ok(Null);
            }
            let x = s / den;
            if language_version == 1
                && x.is_finite()
                && x.fract() == 0.0
                && x >= i64::MIN as f64
                && x <= i64::MAX as f64
            {
                return Ok(Integer(x as i64));
            }
            return Ok(normalize_add_sub_mul_result(
                left,
                right,
                x,
                language_version,
            ));
        }
    }
    let left = peel_singleton_array(left);
    let right = peel_singleton_array(right);
    match (op, &left, &right) {
        (Sub, Integer(a), Integer(b)) => Ok(Integer(a.wrapping_sub(*b))),
        (Mul, Integer(a), Integer(b)) => Ok(Integer(a.wrapping_mul(*b))),
        (Rem, Integer(a), Integer(b)) => {
            if *b == 0 {
                return Err(InterpretError::remainder_by_zero());
            }
            Ok(Integer(a % b))
        }
        (Sub | Mul | Div | Rem, _, _) => {
            // Match Java reference behavior:
            // - if either operand is a `real`, use `AI.real` on both and compute in f64
            // - otherwise use `AI.longint` on both and compute in i64 (except `/` which is always real in v2+)
            let left_is_real = matches!(left, Real(_) | RealDotZero(_));
            let right_is_real = matches!(right, Real(_) | RealDotZero(_));
            match op {
                Sub => {
                    if left_is_real || right_is_real {
                        let x = java_real(&left) - java_real(&right);
                        Ok(normalize_add_sub_mul_result(
                            left,
                            right,
                            x,
                            language_version,
                        ))
                    } else {
                        Ok(Integer(
                            java_longint(&left).wrapping_sub(java_longint(&right)),
                        ))
                    }
                }
                Mul => {
                    if left_is_real || right_is_real {
                        let x = java_real(&left) * java_real(&right);
                        Ok(normalize_add_sub_mul_result(
                            left,
                            right,
                            x,
                            language_version,
                        ))
                    } else {
                        Ok(Integer(
                            java_longint(&left).wrapping_mul(java_longint(&right)),
                        ))
                    }
                }
                Div => {
                    let a = java_real(&left);
                    let b = java_real(&right);
                    if b == 0.0 {
                        if language_version <= 1 {
                            return Ok(Null);
                        }
                        return Ok(Real(a / b));
                    }
                    let x = a / b;
                    // v1: exact integral division on int-like operands stays `integer`; v2+ `/` is always `real`.
                    if language_version >= 2 {
                        return Ok(Real(x));
                    }
                    let integral = x.fract() == 0.0
                        && x.is_finite()
                        && x >= i64::MIN as f64
                        && x <= i64::MAX as f64;
                    if integral {
                        Ok(Integer(x as i64))
                    } else {
                        Ok(Real(x))
                    }
                }
                Rem => {
                    if left_is_real || right_is_real {
                        let a = java_real(&left);
                        let b = java_real(&right);
                        if b == 0.0 {
                            return Err(InterpretError::remainder_by_zero());
                        }
                        let x = a % b;
                        Ok(normalize_add_sub_mul_result(
                            left,
                            right,
                            x,
                            language_version,
                        ))
                    } else {
                        let a = java_longint(&left);
                        let b = java_longint(&right);
                        if b == 0 {
                            // Java v1: logs + returns null; later versions throw DIVISION_BY_ZERO.
                            if language_version == 1 {
                                return Ok(Null);
                            }
                            return Err(InterpretError::remainder_by_zero());
                        }
                        Ok(Integer(a % b))
                    }
                }
                Add => unreachable!(),
                _ => unreachable!(),
            }
        }
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn eq_v1_3_coerce(left: &Value, right: &Value, language_version: u8) -> bool {
    // Observed Java v1–v3 behavior (parity suite):
    // - v1 only: `[x] == y` compares `x == y` (only when the LHS is a singleton array).
    // - v1–v3: `bool == other` performs some coercions (notably string `"true"`/`"false"`).
    match (left, right) {
        (Value::Array(a), other) if !matches!(other, Value::Array(_)) && language_version == 1 => {
            let ab = a.borrow();
            if ab.len() == 1 {
                // Prevent degenerate self-peel `[a]` where `a` is the array itself.
                if let Value::Array(inner) = &ab[0] {
                    if std::rc::Rc::ptr_eq(a, inner) {
                        // fall through
                    } else {
                        return values_equal_for_compare(&ab[0], other);
                    }
                } else {
                    return values_equal_for_compare(&ab[0], other);
                }
            }
        }
        _ => {}
    }

    let bool_from_string_v1_3 = |s: &str| -> bool {
        // Java v1–v3: string→boolean coercion treats numeric `"0"` as false, and `"false"` as false.
        match s {
            "" => false,
            "false" => false,
            "true" => true,
            _ => !s.parse::<f64>().is_ok_and(|n| n == 0.0),
        }
    };

    let numeric_from_string_v1_3 = |s: &str| -> Option<f64> {
        if language_version <= 3 {
            if s.is_empty() {
                Some(0.0)
            } else {
                s.parse::<f64>().ok()
            }
        } else {
            None
        }
    };

    let truthy_v1_3_peel_singleton_array = |v: &Value| -> bool {
        if language_version <= 3 {
            if let Value::Array(a) = v {
                let ab = a.borrow();
                if ab.len() == 1 {
                    return value_truthy(&ab[0]);
                }
            }
        }
        value_truthy(v)
    };

    // v1–v3: numeric/string weak coercion for `==` (e.g. `0 == ""`).
    //
    // Rule (parity suite): compare numerically if the string is numeric (or empty). Special-case `"true"`/`"false"`
    // to compare against numeric truthiness. Other non-numeric strings are not equal to numbers.
    // Exclude booleans here: `bool == "..."` is handled below.
    if language_version <= 3 {
        if !matches!(left, Value::Bool(_)) {
            if let (Some(ln), Value::String(rs)) = (arithmetic_operand_as_f64(left), right) {
                if let Some(rn) = numeric_from_string_v1_3(rs) {
                    return !ln.is_nan() && !rn.is_nan() && ln == rn;
                }
                if rs == "true" || rs == "false" {
                    return value_truthy(left) == bool_from_string_v1_3(rs);
                }
                return false;
            }
        }
        if !matches!(right, Value::Bool(_)) {
            if let (Value::String(ls), Some(rn)) = (left, arithmetic_operand_as_f64(right)) {
                if let Some(ln) = numeric_from_string_v1_3(ls) {
                    return !ln.is_nan() && !rn.is_nan() && ln == rn;
                }
                if ls == "true" || ls == "false" {
                    return bool_from_string_v1_3(ls) == value_truthy(right);
                }
                return false;
            }
        }
    }

    // v1: numeric/container weak coercion via truthiness (e.g. `0 == []`).
    // Exclude booleans here; boolean comparisons are handled below (and observe different rules).
    if language_version <= 3
        && !matches!(left, Value::Bool(_))
        && !matches!(right, Value::Bool(_))
        && !matches!(left, Value::Null)
        && !matches!(right, Value::Null)
    {
        let container_truthy_to_num = |v: &Value| -> Option<f64> {
            match v {
                Value::Array(a) => {
                    // v1–v3: singleton arrays can participate like their single element in weak coercions.
                    let ab = a.borrow();
                    if ab.len() == 1 {
                        if let Some(n) = numeric_as_f64(&ab[0]) {
                            return Some(n);
                        }
                    }
                    Some(if ab.is_empty() { 0.0 } else { 1.0 })
                }
                Value::Map(_) | Value::Object(_) | Value::Set(_) | Value::Interval(_) => {
                    Some(if value_truthy(v) { 1.0 } else { 0.0 })
                }
                _ => None,
            }
        };
        if let (Some(ln), Some(rn)) = (numeric_as_f64(left), container_truthy_to_num(right)) {
            return !ln.is_nan() && ln == rn;
        }
        if let (Some(ln), Some(rn)) = (container_truthy_to_num(left), numeric_as_f64(right)) {
            return !rn.is_nan() && ln == rn;
        }
    }

    match (left, right) {
        (Value::Bool(x), Value::String(s)) => *x == bool_from_string_v1_3(s),
        (Value::String(s), Value::Bool(x)) => bool_from_string_v1_3(s) == *x,
        (Value::Bool(x), other) => *x == truthy_v1_3_peel_singleton_array(other),
        (other, Value::Bool(x)) => truthy_v1_3_peel_singleton_array(other) == *x,
        _ => values_equal_for_compare(left, right),
    }
}

fn eval_equality(
    op: HirBinOp,
    left: Value,
    right: Value,
    language_version: u8,
) -> Result<Value, InterpretError> {
    use HirBinOp::{Eq, Ne, StrictEq, StrictNe};
    use Value::Bool;
    let eq = match op {
        // `===` / `!==` is strict (no v1–v3 weak coercions).
        StrictEq | StrictNe => values_equal_for_compare(&left, &right),
        _ => {
            if language_version <= 3 {
                eq_v1_3_coerce(&left, &right, language_version)
            } else {
                values_equal_for_compare(&left, &right)
            }
        }
    };
    let out = match op {
        Eq | StrictEq => eq,
        Ne | StrictNe => !eq,
        _ => unreachable!(),
    };
    Ok(Bool(out))
}

fn eval_ordering(op: HirBinOp, left: Value, right: Value) -> Result<Value, InterpretError> {
    use std::cmp::Ordering;
    use HirBinOp::{Ge, Gt, Le, Lt};
    use Value::{Bool, Integer, Null, String};
    // Java Leek: `null` orders like `0` for comparisons (`null < 3` is true; `null < 0` is false).
    let left = match left {
        Null => Integer(0),
        v => v,
    };
    let right = match right {
        Null => Integer(0),
        v => v,
    };
    let ord = match (&left, &right) {
        (Integer(a), Integer(b)) => a.cmp(b),
        (String(sa), String(sb)) => sa.cmp(sb),
        _ => {
            // Like Java Leek: bool/null promote for ordering (`true < 10`); string vs number is an error;
            // comparisons with arrays/maps/… that do not coerce yield `false`, not an error.
            let a_num = arithmetic_operand_as_f64(&left);
            let b_num = arithmetic_operand_as_f64(&right);
            match (a_num, b_num) {
                (Some(a), Some(b)) => {
                    if a.is_nan() || b.is_nan() {
                        return Ok(Bool(false));
                    }
                    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
                }
                (None, None) => return Ok(Bool(false)),
                _ => {
                    let left_is_string = matches!(&left, Value::String(_));
                    let right_is_string = matches!(&right, Value::String(_));
                    if (a_num.is_some() && right_is_string) || (b_num.is_some() && left_is_string) {
                        return Err(InterpretError::wrong_operand_types_compare());
                    }
                    return Ok(Bool(false));
                }
            }
        }
    };
    let ok = match op {
        Lt => ord == Ordering::Less,
        Le => ord != Ordering::Greater,
        Gt => ord == Ordering::Greater,
        Ge => ord != Ordering::Less,
        _ => unreachable!(),
    };
    Ok(Bool(ok))
}
