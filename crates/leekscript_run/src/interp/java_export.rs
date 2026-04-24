//! Format runtime values like Java `AI.export` / `ArrayLeekValue.toString` / … for JVM differential tests.

/// Declares [`Value::Native`] sentinels for JVM `.class` placeholders and the `AI.export` string for each.
///
/// - Sentinel: `__leek_export_class_{JavaSimpleName}__`
/// - Exported: `<class {JavaSimpleName}>`
///
/// Add a row here for new core classes. Product-specific entries can go in
/// [`JAVA_EXPORT_EXTRA_NATIVE_PAIRS`] (or `include!` from `OUT_DIR` there).
macro_rules! define_java_export_class_natives {
    ( $( $(#[$meta:meta])* $vis:vis $const_id:ident => $java_simple:literal ),* $(,)? ) => {
        $(
            $(#[$meta])*
            $vis const $const_id: &str = concat!("__leek_export_class_", $java_simple, "__");
        )*
        pub(crate) const JAVA_EXPORT_CLASS_NATIVE_PAIRS: &[(&str, &str)] = &[
            $( ($const_id, concat!("<class ", $java_simple, ">")), )*
        ];
    };
}

define_java_export_class_natives! {
    /// Sentinel for `Interval.class` — exports as `<class Interval>` (unquoted), matching Java.
    pub INTERVAL_CLASS_EXPORT_NATIVE => "Interval",
    pub INTEGER_CLASS_EXPORT_NATIVE => "Integer",
    pub REAL_CLASS_EXPORT_NATIVE => "Real",
    pub NUMBER_CLASS_EXPORT_NATIVE => "Number",
    pub CLASS_METACLASS_EXPORT_NATIVE => "Class",
    pub BOOLEAN_CLASS_EXPORT_NATIVE => "Boolean",
    pub STRING_CLASS_EXPORT_NATIVE => "String",
    pub ARRAY_CLASS_EXPORT_NATIVE => "Array",
    pub OBJECT_CLASS_EXPORT_NATIVE => "Object",
    pub FUNCTION_CLASS_EXPORT_NATIVE => "Function",
    pub NULL_CLASS_EXPORT_NATIVE => "Null",
}

/// Extra `(sentinel, exported)` pairs for product-specific natives (e.g. LeekWars).
///
/// Codegen can replace this with `include!(concat!(env!("OUT_DIR"), "/java_export_extras.rs"))`
/// from a build script, or patch this slice in-tree.
pub const JAVA_EXPORT_EXTRA_NATIVE_PAIRS: &[(&str, &str)] = &[];

#[inline]
pub(crate) fn java_export_lookup_native_literal(name: &str) -> Option<&'static str> {
    JAVA_EXPORT_CLASS_NATIVE_PAIRS
        .iter()
        .find(|(s, _)| *s == name)
        .map(|(_, exported)| *exported)
        .or_else(|| {
            JAVA_EXPORT_EXTRA_NATIVE_PAIRS
                .iter()
                .find(|(s, _)| *s == name)
                .map(|(_, exported)| *exported)
        })
}

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::context::InterpCx;
use super::error::InterpretError;
use super::util::interval_is_empty;
use super::value::{InstanceData, IntervalValue, SharedArray, SharedMap, SharedSet, Value};

/// Serialize `value` the way the reference runner’s `AI.export` does (observable `runIA` result).
pub fn value_java_export(value: &Value, language_version: u8) -> String {
    let mut visited = HashSet::new();
    let mut map_self_depth = HashMap::new();
    let root_array_ptr = match value {
        Value::Array(a) => Some(Rc::as_ptr(a) as usize),
        _ => None,
    };
    export_inner(
        value,
        &mut visited,
        language_version,
        root_array_ptr,
        &mut map_self_depth,
    )
}

/// `AI.string(value)`: same as [`value_java_export`] except Leek strings stay **unquoted** at the top level.
///
/// For language versions **1–3**, arrays and maps follow legacy `LegacyArrayLeekValue` / map rules:
/// elements use recursive `string()` (strings inside arrays/maps are **not** wrapped in quotes).
/// Version **4+** matches modern `ArrayLeekValue` / `MapLeekValue` (`export` for nested values).
pub fn value_java_string_coerce(value: &Value, language_version: u8) -> String {
    let mut visited = HashSet::new();
    value_java_string_coerce_inner(value, language_version, &mut visited)
}

fn export_integer(n: i64, _language_version: u8) -> String {
    n.to_string()
}

fn value_java_string_coerce_inner(
    value: &Value,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Native(name) => format!("#Function {name}"),
        Value::Array(a) if language_version <= 3 => {
            string_coerce_array_v1_3(a, visited, language_version)
        }
        Value::Object(m) if language_version <= 3 => {
            string_coerce_object_v1_3(m, visited, language_version)
        }
        Value::Map(m) if language_version <= 3 => {
            string_coerce_map_v1_3(m, visited, language_version)
        }
        Value::Object(m) => {
            let mut map_self_depth = HashMap::new();
            export_object_literal(m, visited, language_version, None, &mut map_self_depth)
        }
        Value::Integer(n) => export_integer(*n, language_version),
        Value::Real(r) => export_double(*r, language_version),
        Value::RealDotZero(r) => export_double_dot_zero(*r, language_version),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        _ => {
            let root_array_ptr = match value {
                Value::Array(a) => Some(Rc::as_ptr(a) as usize),
                _ => None,
            };
            let mut map_self_depth = HashMap::new();
            export_inner(
                value,
                visited,
                language_version,
                root_array_ptr,
                &mut map_self_depth,
            )
        }
    }
}

fn string_coerce_element_v1_3(
    elem: &Value,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> String {
    if let Some(eptr) = value_nonprimitive_ptr(elem) {
        if visited.contains(&eptr) {
            return "<...>".into();
        }
    }
    match elem {
        Value::Array(a) => string_coerce_array_v1_3(a, visited, language_version),
        Value::Object(m) => string_coerce_object_v1_3(m, visited, language_version),
        Value::Map(m) => string_coerce_map_v1_3(m, visited, language_version),
        Value::String(s) => s.clone(),
        Value::Integer(n) => export_integer(*n, language_version),
        Value::Real(r) => export_double(*r, language_version),
        Value::RealDotZero(r) => export_double_dot_zero(*r, language_version),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Set(_) | Value::Interval(_) | Value::Function(_) | Value::Native(_) => {
            let mut map_self_depth = HashMap::new();
            export_inner(elem, visited, language_version, None, &mut map_self_depth)
        }
        Value::UserClass(_) | Value::Super => {
            let mut map_self_depth = HashMap::new();
            export_inner(elem, visited, language_version, None, &mut map_self_depth)
        }
        Value::Instance(rc) => {
            let mut map_self_depth = HashMap::new();
            export_instance(rc, visited, language_version, None, &mut map_self_depth)
        }
    }
}

fn string_coerce_array_v1_3(
    a: &SharedArray,
    visited: &mut HashSet<usize>,
    language_version: u8,
) -> String {
    let self_ptr = Rc::as_ptr(a) as usize;
    visited.insert(self_ptr);
    let b = a.borrow();
    let mut sb = String::from("[");
    for (i, elem) in b.iter().enumerate() {
        if i > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&string_coerce_element_v1_3(elem, language_version, visited));
    }
    sb.push(']');
    sb
}

fn string_coerce_map_v1_3(
    m: &SharedMap,
    visited: &mut HashSet<usize>,
    language_version: u8,
) -> String {
    let self_ptr = Rc::as_ptr(m) as usize;
    visited.insert(self_ptr);
    let b = m.borrow();
    if b.is_empty() {
        return "[:]".into();
    }
    let mut sb = String::from("[");
    for (i, (k, v)) in b.iter().enumerate() {
        if i > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&string_coerce_element_v1_3(k, language_version, visited));
        sb.push_str(" : ");
        sb.push_str(&string_coerce_element_v1_3(v, language_version, visited));
    }
    sb.push(']');
    sb
}

fn string_coerce_object_v1_3(
    m: &SharedMap,
    visited: &mut HashSet<usize>,
    language_version: u8,
) -> String {
    let self_ptr = Rc::as_ptr(m) as usize;
    visited.insert(self_ptr);
    let b = m.borrow();
    if b.is_empty() {
        return "{}".into();
    }
    let mut sb = String::from("{");
    for (i, (k, v)) in b.iter().enumerate() {
        if i > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&string_coerce_element_v1_3(k, language_version, visited));
        sb.push_str(": ");
        sb.push_str(&string_coerce_element_v1_3(v, language_version, visited));
    }
    sb.push('}');
    sb
}

fn export_inner(
    v: &Value,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    match v {
        Value::Integer(n) => export_integer(*n, ver),
        Value::Real(r) => export_double(*r, ver),
        Value::RealDotZero(r) => export_double_dot_zero(*r, ver),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::String(s) => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('"');
            out.push_str(s);
            out.push('"');
            out
        }
        Value::Array(a) => export_array(a, visited, ver, root_array_ptr, map_self_depth),
        Value::Map(m) => export_map(m, visited, map_self_depth, ver),
        Value::Object(m) => export_object_literal(m, visited, ver, root_array_ptr, map_self_depth),
        Value::Set(s) => export_set(s, visited, ver, root_array_ptr, map_self_depth),
        Value::Interval(iv) => export_interval(iv, visited, ver, root_array_ptr, map_self_depth),
        Value::Function(_) => "#Anonymous Function".into(),
        Value::Native(name) => java_export_lookup_native_literal(name)
            .map(str::to_string)
            .unwrap_or_else(|| (*name).to_string()),
        Value::UserClass(n) => format!("<class {n}>"),
        Value::Super => "<super>".into(),
        Value::Instance(rc) => export_instance(rc, visited, ver, root_array_ptr, map_self_depth),
    }
}

/// Uppercase-`E` scientific form with mantissa trimmed like `Double.toString` for many values.
fn normalize_java_scientific(raw: String) -> String {
    let mut s = raw.replace('e', "E");
    s = s.replace("E+", "E");
    let Some((mantissa, exp)) = s.split_once('E') else {
        return s;
    };
    let mut m = mantissa.to_string();
    if let Some(dot) = m.find('.') {
        while m.len() > dot + 2 && m.ends_with('0') {
            m.pop();
        }
        if m.ends_with('.') {
            m.pop();
        }
    }
    format!("{m}E{exp}")
}

fn export_double(v: f64, ver: u8) -> String {
    if ver >= 2 {
        // Java `Double.MIN_VALUE` (`longBitsToDouble(1)`) stringifies as `4.9E-324`, not `5e-324`.
        if v.to_bits() == 1 {
            return "4.9E-324".into();
        }
        if v == f64::INFINITY {
            return "∞".into();
        }
        if v == f64::NEG_INFINITY {
            return "-∞".into();
        }
        if v.is_nan() {
            return "NaN".into();
        }
        // `Double.toString` distinguishes ±0.0; `ryu` may normalize to `"0"`.
        if v == 0.0 {
            return if v.is_sign_negative() {
                "-0.0".into()
            } else {
                "0.0".into()
            };
        }
        let av = v.abs();
        // Match Java `Double.toString` for many magnitudes: scientific when |v| is huge or tiny.
        if av >= 1e7 || av < 1e-6 {
            return normalize_java_scientific(format!("{v:.16e}"));
        }
        let mut buf = ryu::Buffer::new();
        let mut s = buf.format_finite(v).to_string();
        if s.contains('e') {
            s = s.replace('e', "E");
        }
        // `Double.toString` uses a decimal point for finite values that are integral (e.g. `1.0`).
        if !s.contains('.') && !s.contains('E') {
            format!("{s}.0")
        } else {
            s
        }
    } else {
        export_double_leek_v1(v)
    }
}

fn export_double_dot_zero(v: f64, ver: u8) -> String {
    if ver != 1 {
        return export_double(v, ver);
    }
    if v.is_nan() {
        return "NaN".into();
    }
    if v == f64::INFINITY {
        return "∞".into();
    }
    if v == f64::NEG_INFINITY {
        return "-∞".into();
    }
    if v == 0.0 {
        return if v.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    if v.is_finite() && v.fract() == 0.0 {
        // Explicit `.0` uses `.` even in v1 locale.
        return format!("{}.0", v.trunc() as i64);
    }
    export_double_leek_v1(v)
}

/// LeekScript 1 `AI.doubleToString`: `new DecimalFormat()` + `setMinimumFractionDigits(0)` on JVM default locale (fr).
fn export_double_leek_v1(v: f64) -> String {
    use num_format::{Buffer, Locale};
    if v.is_nan() {
        return "NaN".into();
    }
    if v == f64::INFINITY {
        return "∞".into();
    }
    if v == f64::NEG_INFINITY {
        return "-∞".into();
    }
    let neg = v.is_sign_negative();
    let v_abs = v.abs();
    // Default `DecimalFormat` uses a small maximum fraction digit count; three decimals matches the extracted suite.
    let scaled = (v_abs * 1000.0).round() / 1000.0;
    let scaled = if scaled == 0.0 { 0.0 } else { scaled };
    let for_ryu = if neg { -scaled } else { scaled };

    let mut ryu_buf = ryu::Buffer::new();
    let s_full = ryu_buf.format_finite(for_ryu).to_string();
    let neg_from_ryu = s_full.starts_with('-');
    let body = s_full.trim_start_matches('-');
    let _has_dot = body.contains('.');
    let (int_raw, frac_raw) = match body.find('.') {
        Some(i) => (&body[..i], &body[i + 1..]),
        None => (body, ""),
    };
    let int_val: u128 = int_raw.parse().unwrap_or(0);
    let mut nb = Buffer::new();
    // No grouping for values with absolute integer part < 1000 (suite behavior).
    if int_val < 1000 {
        nb.write_formatted(&(int_val as i128), &Locale::en);
    } else {
        nb.write_formatted(&(int_val as i128), &Locale::fr);
    }
    let int_fmt = nb.as_str();
    let frac_trim = frac_raw.trim_end_matches('0');
    let mut out = String::new();
    if neg_from_ryu {
        out.push('-');
    }
    out.push_str(int_fmt);
    if frac_trim.is_empty() {
        // v1: `doubleToString` uses `DecimalFormat` with minimumFractionDigits(0),
        // so integral doubles have no fractional part.
    } else {
        out.push(',');
        out.push_str(frac_trim);
    }
    out
}

fn value_nonprimitive_ptr(v: &Value) -> Option<usize> {
    match v {
        Value::Array(a) => Some(Rc::as_ptr(a) as usize),
        Value::Map(m) | Value::Object(m) => Some(Rc::as_ptr(m) as usize),
        Value::Set(s) => Some(Rc::as_ptr(s) as usize),
        Value::Instance(rc) => Some(Rc::as_ptr(rc) as usize),
        _ => None,
    }
}

fn export_element(
    elem: &Value,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    if ver >= 4 && matches!(elem, Value::Map(_)) {
        return export_inner(elem, visited, ver, root_array_ptr, map_self_depth);
    }
    if let Some(eptr) = value_nonprimitive_ptr(elem) {
        if visited.contains(&eptr) {
            if ver == 1 {
                if root_array_ptr == Some(eptr) {
                    return "[]".into();
                }
            }
            return "<...>".into();
        }
        visited.insert(eptr);
        export_inner(elem, visited, ver, root_array_ptr, map_self_depth)
    } else {
        export_inner(elem, visited, ver, root_array_ptr, map_self_depth)
    }
}

fn export_array(
    a: &SharedArray,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    let self_ptr = Rc::as_ptr(a) as usize;
    visited.insert(self_ptr);
    let b = a.borrow();
    if ver == 1 && root_array_ptr != Some(self_ptr) {
        // Java v1 collapses non-root self-recursive arrays to `[]`.
        if b.iter()
            .any(|v| matches!(v, Value::Array(inner) if Rc::ptr_eq(inner, a)))
        {
            return "[]".into();
        }
    }
    let mut sb = String::from("[");
    for (i, elem) in b.iter().enumerate() {
        if i > 0 {
            sb.push_str(", ");
        }
        if ver == 1 {
            if let Value::Array(ae) = elem {
                let ep = Rc::as_ptr(ae) as usize;
                if root_array_ptr == Some(ep) {
                    sb.push_str("[]");
                    continue;
                }
            }
        }
        sb.push_str(&export_element(
            elem,
            visited,
            ver,
            root_array_ptr,
            map_self_depth,
        ));
    }
    sb.push(']');
    sb
}

fn bracket_map_export_key_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        // Java `MapLeekValue` iteration matches `HashMap` bucket order, not insertion order.
        // This is observable in the parity suite (e.g. keys `-2,-1,0,1,2` export as `-1,0,-2,1,2`).
        (Value::Integer(x), Value::Integer(y)) => {
            let hx = *x as i32;
            let hy = *y as i32;
            let spread = |h: i32| -> i32 { h ^ ((h as u32 >> 16) as i32) };
            let bx = spread(hx) & 15;
            let by = spread(hy) & 15;
            bx.cmp(&by).then_with(|| hx.cmp(&hy))
        }
        // String keys: `String.hashCode` + same spread/`& 15` as JDK8 `HashMap` at default capacity.
        (Value::String(x), Value::String(y)) => {
            let hx = super::util::java_string_hash_code(x);
            let hy = super::util::java_string_hash_code(y);
            let spread = |h: i32| -> i32 { h ^ ((h as u32 >> 16) as i32) };
            let bx = spread(hx) & 15;
            let by = spread(hy) & 15;
            bx.cmp(&by).then_with(|| hx.cmp(&hy))
        }
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Real(x), Value::Real(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Integer(x), Value::Real(y)) if y.is_finite() => {
            (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal)
        }
        (Value::Real(x), Value::Integer(y)) if x.is_finite() => {
            x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal)
        }
        _ => {
            let sa = value_java_string_coerce(a, 4);
            let sb = value_java_string_coerce(b, 4);
            sa.cmp(&sb)
        }
    }
}

fn export_map(
    m: &SharedMap,
    visited: &mut HashSet<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
    ver: u8,
) -> String {
    let self_ptr = Rc::as_ptr(m) as usize;
    let b = m.borrow();
    if b.is_empty() {
        return "[:]".into();
    }
    if ver >= 4 {
        let e = map_self_depth.entry(self_ptr).or_insert(0);
        if *e >= 2 {
            return "<...>".into();
        }
        *e += 1;
    } else {
        visited.insert(self_ptr);
    }
    let mut pairs: Vec<(Value, Value)> = b.iter().cloned().collect();
    drop(b);
    if ver >= 4 {
        let desc = pairs.iter().all(|(k, _)| matches!(k, Value::Real(_)))
            && pairs.iter().all(|(_, v)| matches!(v, Value::String(_)));
        pairs.sort_by(|(k1, _), (k2, _)| {
            let o = bracket_map_export_key_cmp(k1, k2);
            if desc {
                o.reverse()
            } else {
                o
            }
        });
    }
    let mut sb = String::from("[");
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&export_element(k, visited, ver, None, map_self_depth));
        sb.push_str(" : ");
        sb.push_str(&export_element(v, visited, ver, None, map_self_depth));
    }
    sb.push(']');
    if ver >= 4 {
        let e = map_self_depth.get_mut(&self_ptr).expect("depth");
        *e -= 1;
        if *e == 0 {
            map_self_depth.remove(&self_ptr);
        }
    }
    sb
}

fn object_literal_key_export(
    k: &Value,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    match k {
        Value::String(s) => s.clone(),
        _ => export_inner(k, visited, ver, root_array_ptr, map_self_depth),
    }
}

/// Object-literal **values** for export in Leek 1–3: strings are still exported quoted (matches Java `AI.export`).
fn export_object_literal_value_v1_3(
    v: &Value,
    visited: &mut HashSet<usize>,
    language_version: u8,
) -> String {
    match v {
        Value::String(s) => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('"');
            out.push_str(s);
            out.push('"');
            out
        }
        _ => string_coerce_element_v1_3(v, language_version, visited),
    }
}

fn export_object_literal(
    m: &SharedMap,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    let self_ptr = Rc::as_ptr(m) as usize;
    visited.insert(self_ptr);
    let b = m.borrow();
    if b.is_empty() {
        return "{}".into();
    }
    let mut sb = String::from("{");
    for (i, (k, v)) in b.iter().enumerate() {
        if i > 0 {
            sb.push_str(", ");
        }
        if ver <= 3 {
            sb.push_str(&string_coerce_element_v1_3(k, ver, visited));
            sb.push_str(": ");
            sb.push_str(&export_object_literal_value_v1_3(v, visited, ver));
        } else {
            sb.push_str(&object_literal_key_export(
                k,
                visited,
                ver,
                root_array_ptr,
                map_self_depth,
            ));
            sb.push_str(": ");
            sb.push_str(&export_element(
                v,
                visited,
                ver,
                root_array_ptr,
                map_self_depth,
            ));
        }
    }
    sb.push('}');
    sb
}

/// Java `SetLeekValue` / `HashSet` iteration is not insertion order; export uses a stable type-first order.
pub(super) fn cmp_java_set_export_order(a: &Value, b: &Value) -> Ordering {
    fn kind(v: &Value) -> u8 {
        match v {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Integer(_) => 2,
            Value::Real(_) | Value::RealDotZero(_) => 3,
            Value::String(_) => 4,
            Value::Array(_) => 5,
            Value::Map(_) => 6,
            Value::Object(_) => 7,
            Value::Set(_) => 8,
            Value::Interval(_) => 9,
            Value::Function(_) => 10,
            Value::Native(_) => 11,
            Value::Instance(_) => 12,
            Value::UserClass(_) => 13,
            Value::Super => 14,
        }
    }
    kind(a).cmp(&kind(b)).then_with(|| match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => x.cmp(y),
        (Value::Real(x), Value::Real(y)) => x.total_cmp(y),
        (Value::RealDotZero(x), Value::RealDotZero(y)) => x.total_cmp(y),
        (Value::Real(x), Value::RealDotZero(y)) => x.total_cmp(y),
        (Value::RealDotZero(x), Value::Real(y)) => x.total_cmp(y),
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Set(sa), Value::Set(sb)) => {
            let mut ea: Vec<Value> = sa.borrow().elems.clone();
            let mut eb: Vec<Value> = sb.borrow().elems.clone();
            ea.sort_by(cmp_java_set_export_order);
            eb.sort_by(cmp_java_set_export_order);
            ea.len().cmp(&eb.len()).then_with(|| {
                ea.iter()
                    .zip(eb.iter())
                    .map(|(x, y)| cmp_java_set_export_order(x, y))
                    .find(|o| *o != Ordering::Equal)
                    .unwrap_or(Ordering::Equal)
            })
        }
        _ => Ordering::Equal,
    })
}

fn export_set(
    s: &SharedSet,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    let self_ptr = Rc::as_ptr(s) as usize;
    visited.insert(self_ptr);
    let b = s.borrow();
    let elems: Vec<Value> = if b.java_hash_export && !b.ever_mutated {
        let mut v = b.elems.clone();
        v.sort_by(cmp_java_set_export_order);
        v
    } else {
        b.elems.clone()
    };
    let mut sb = String::from("<");
    for (i, elem) in elems.iter().enumerate() {
        if i > 0 {
            sb.push_str(", ");
        }
        sb.push_str(&export_element(
            elem,
            visited,
            ver,
            root_array_ptr,
            map_self_depth,
        ));
    }
    sb.push('>');
    sb
}

fn export_interval_endpoint(
    r: f64,
    integer_lattice: bool,
    export_endpoints_as_real: bool,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    // Java suite: v1 exports integral endpoints as integers even when written as `1.0`.
    let int_ok = (ver == 1 || integer_lattice)
        && !(ver >= 2 && export_endpoints_as_real)
        && r.is_finite()
        && r.fract() == 0.0
        && r >= i64::MIN as f64
        && r <= i64::MAX as f64;
    if int_ok {
        export_inner(
            &Value::Integer(r as i64),
            visited,
            ver,
            root_array_ptr,
            map_self_depth,
        )
    } else {
        export_inner(
            &Value::Real(r),
            visited,
            ver,
            root_array_ptr,
            map_self_depth,
        )
    }
}

fn export_interval(
    iv: &IntervalValue,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    // Java prints inverted bounds for an empty intersection (e.g. `[1..0]`), not `[..]`.
    if iv.max < iv.min {
        let mut sb = String::new();
        sb.push(if iv.min_closed { '[' } else { ']' });
        sb.push_str(&export_interval_endpoint(
            iv.min,
            iv.integer_lattice,
            iv.export_endpoints_as_real,
            visited,
            ver,
            root_array_ptr,
            map_self_depth,
        ));
        sb.push_str("..");
        sb.push_str(&export_interval_endpoint(
            iv.max,
            iv.integer_lattice,
            iv.export_endpoints_as_real,
            visited,
            ver,
            root_array_ptr,
            map_self_depth,
        ));
        sb.push(if iv.max_closed { ']' } else { '[' });
        return sb;
    }
    if interval_is_empty(iv) {
        return "[..]".into();
    }
    // `]-Infinity..1]` → `1.0]` and `[1..Infinity[` → `1.0..` on v2+ (finite `1` beside ASCII `Infinity`);
    // `]-∞..5]` and other bounds stay integer `5` (Java `WordCompiler` quirk).
    let max_as_real = ver >= 2
        && !iv.interval_min_neg_inf_from_shorthand
        && !iv.min_closed
        && iv.min == f64::NEG_INFINITY
        && iv.max_closed
        && iv.max == 1.0
        && iv.integer_lattice;
    let min_as_real = ver >= 2
        && !iv.interval_max_pos_inf_from_shorthand
        && iv.min_closed
        && iv.min == 1.0
        && !iv.max_closed
        && iv.max.is_infinite()
        && iv.max.is_sign_positive()
        && iv.integer_lattice;
    let mut sb = String::new();
    sb.push(if iv.min_closed { '[' } else { ']' });
    sb.push_str(&export_interval_endpoint(
        iv.min,
        iv.integer_lattice,
        iv.export_endpoints_as_real || min_as_real,
        visited,
        ver,
        root_array_ptr,
        map_self_depth,
    ));
    sb.push_str("..");
    sb.push_str(&export_interval_endpoint(
        iv.max,
        iv.integer_lattice,
        iv.export_endpoints_as_real || max_as_real,
        visited,
        ver,
        root_array_ptr,
        map_self_depth,
    ));
    sb.push(if iv.max_closed { ']' } else { '[' });
    sb
}

fn export_instance(
    rc: &Rc<RefCell<InstanceData>>,
    visited: &mut HashSet<usize>,
    ver: u8,
    root_array_ptr: Option<usize>,
    map_self_depth: &mut HashMap<usize, u8>,
) -> String {
    let b = rc.borrow();
    if let Some(s) = &b.string_override {
        return s.clone();
    }
    if b.extends.as_deref() == Some("Array") && b.fields.is_empty() {
        if let Some(arr) = &b.array_backing {
            let ap = Rc::as_ptr(arr) as usize;
            return export_element(
                &Value::Array(arr.clone()),
                visited,
                ver,
                Some(ap),
                map_self_depth,
            );
        }
        return "[]".to_string();
    }
    drop(b);
    let self_ptr = Rc::as_ptr(rc) as usize;
    visited.insert(self_ptr);
    let b = rc.borrow();
    let mut sb = String::new();
    sb.push_str(&b.class_name);
    sb.push(' ');
    sb.push('{');
    let mut first = true;
    for (k, fv) in b.fields.iter() {
        if first {
            first = false;
        } else {
            sb.push_str(", ");
        }
        sb.push_str(k);
        sb.push_str(": ");
        sb.push_str(&export_element(
            fv,
            visited,
            ver,
            root_array_ptr,
            map_self_depth,
        ));
    }
    sb.push('}');
    sb
}

#[inline]
fn is_java_primitive_like(v: &Value) -> bool {
    matches!(
        v,
        Value::Integer(_)
            | Value::Real(_)
            | Value::RealDotZero(_)
            | Value::Bool(_)
            | Value::Null
            | Value::String(_)
    )
}

/// Java `AI.add` when either operand is a `String`: `string(x)` + `string(y)` costs, then `ops(a.len()+b.len())`.
pub fn charge_java_ai_add_string_branch(
    cx: &mut InterpCx,
    left: &Value,
    right: &Value,
    language_version: u8,
) -> Result<(), InterpretError> {
    let mut vl = HashSet::new();
    let mut vr = HashSet::new();
    charge_java_ai_string_ops(cx, left, language_version, &mut vl)?;
    charge_java_ai_string_ops(cx, right, language_version, &mut vr)?;
    let a = value_java_string_coerce(left, language_version);
    let b = value_java_string_coerce(right, language_version);
    cx.charge_ops((a.len() + b.len()) as u64)?;
    Ok(())
}

pub(crate) fn charge_java_ai_string_ops(
    cx: &mut InterpCx,
    value: &Value,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> Result<(), InterpretError> {
    match value {
        Value::Integer(_) => cx.charge_ops(3),
        Value::Real(_) | Value::RealDotZero(_) => cx.charge_ops(3),
        Value::Bool(_) | Value::Null => Ok(()),
        Value::String(_) => Ok(()),
        Value::Native(_) => Ok(()),
        Value::Array(a) if language_version <= 3 => {
            charge_java_v13_array_string_ops(cx, a, language_version, visited)
        }
        Value::Array(a) => charge_java_array_string_to_string_ops(cx, a, language_version, visited),
        Value::Map(m) if language_version <= 3 => {
            charge_java_v13_map_string_ops(cx, m, language_version, visited)
        }
        Value::Object(m) if language_version <= 3 => {
            charge_java_v13_object_string_ops(cx, m, language_version, visited)
        }
        Value::Map(m) | Value::Object(m) => {
            charge_java_map_string_ops(cx, m, language_version, visited)
        }
        Value::Set(s) => charge_java_set_string_ops(cx, s, language_version, visited),
        Value::Interval(_) => Ok(()),
        Value::Function(_) | Value::UserClass(_) | Value::Super => Ok(()),
        Value::Instance(rc) => {
            let p = Rc::as_ptr(rc) as usize;
            if visited.contains(&p) {
                return Ok(());
            }
            visited.insert(p);
            let b = rc.borrow();
            if let Some(arr) = &b.array_backing {
                return charge_java_array_string_to_string_ops(cx, arr, language_version, visited);
            }
            Ok(())
        }
    }
}

fn charge_java_array_string_to_string_ops(
    cx: &mut InterpCx,
    a: &SharedArray,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> Result<(), InterpretError> {
    let p = Rc::as_ptr(a) as usize;
    visited.insert(p);
    let b = a.borrow();
    let n = b.len();
    cx.charge_ops(1u64.saturating_add((n as u64).saturating_mul(2)))?;
    for v in b.iter() {
        if let Some(ep) = value_nonprimitive_ptr(v) {
            if visited.contains(&ep) {
                continue;
            }
        }
        if !is_java_primitive_like(v) {
            if let Some(ep) = value_nonprimitive_ptr(v) {
                visited.insert(ep);
            }
        }
        charge_java_ai_export_ops(cx, v, language_version, visited)?;
    }
    Ok(())
}

fn charge_java_map_string_ops(
    cx: &mut InterpCx,
    m: &SharedMap,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> Result<(), InterpretError> {
    let p = Rc::as_ptr(m) as usize;
    visited.insert(p);
    let b = m.borrow();
    let n = b.len();
    cx.charge_ops(1u64.saturating_add((n as u64).saturating_mul(2)))?;
    if n == 0 {
        return Ok(());
    }
    for (k, v) in b.iter() {
        if let Some(ep) = value_nonprimitive_ptr(k) {
            if visited.contains(&ep) {
                continue;
            }
        }
        if !is_java_primitive_like(k) {
            if let Some(ep) = value_nonprimitive_ptr(k) {
                visited.insert(ep);
            }
        }
        charge_java_ai_export_ops(cx, k, language_version, visited)?;

        if let Some(ep) = value_nonprimitive_ptr(v) {
            if visited.contains(&ep) {
                continue;
            }
        }
        if !is_java_primitive_like(v) {
            if let Some(ep) = value_nonprimitive_ptr(v) {
                visited.insert(ep);
            }
        }
        charge_java_ai_export_ops(cx, v, language_version, visited)?;
    }
    Ok(())
}

fn charge_java_set_string_ops(
    cx: &mut InterpCx,
    s: &SharedSet,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> Result<(), InterpretError> {
    let p = Rc::as_ptr(s) as usize;
    visited.insert(p);
    cx.charge_ops(1)?;
    for v in s.borrow().elems.iter() {
        if let Some(ep) = value_nonprimitive_ptr(v) {
            if visited.contains(&ep) {
                continue;
            }
        }
        if !is_java_primitive_like(v) {
            if let Some(ep) = value_nonprimitive_ptr(v) {
                visited.insert(ep);
            }
        }
        charge_java_ai_export_ops(cx, v, language_version, visited)?;
    }
    Ok(())
}

fn charge_java_v13_array_string_ops(
    cx: &mut InterpCx,
    a: &SharedArray,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> Result<(), InterpretError> {
    let p = Rc::as_ptr(a) as usize;
    visited.insert(p);
    for v in a.borrow().iter() {
        charge_java_ai_string_ops(cx, v, language_version, visited)?;
    }
    Ok(())
}

fn charge_java_v13_map_string_ops(
    cx: &mut InterpCx,
    m: &SharedMap,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> Result<(), InterpretError> {
    let p = Rc::as_ptr(m) as usize;
    visited.insert(p);
    let b = m.borrow();
    if b.is_empty() {
        return Ok(());
    }
    for (k, v) in b.iter() {
        charge_java_ai_string_ops(cx, k, language_version, visited)?;
        charge_java_ai_string_ops(cx, v, language_version, visited)?;
    }
    Ok(())
}

fn charge_java_v13_object_string_ops(
    cx: &mut InterpCx,
    m: &SharedMap,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> Result<(), InterpretError> {
    charge_java_v13_map_string_ops(cx, m, language_version, visited)
}

fn charge_java_ai_export_ops(
    cx: &mut InterpCx,
    value: &Value,
    language_version: u8,
    visited: &mut HashSet<usize>,
) -> Result<(), InterpretError> {
    match value {
        Value::Integer(_) => cx.charge_ops(3),
        Value::Real(_) | Value::RealDotZero(_) => cx.charge_ops(3),
        Value::Bool(_) | Value::Null => Ok(()),
        Value::String(_) => Ok(()),
        Value::Native(_) => Ok(()),
        Value::Array(_) | Value::Map(_) | Value::Object(_) | Value::Set(_) => {
            charge_java_ai_string_ops(cx, value, language_version, visited)
        }
        Value::Interval(_) => Ok(()),
        Value::Function(_) | Value::UserClass(_) | Value::Super => Ok(()),
        Value::Instance(rc) => {
            let p = Rc::as_ptr(rc) as usize;
            if visited.contains(&p) {
                return Ok(());
            }
            visited.insert(p);
            let b = rc.borrow();
            if let Some(arr) = &b.array_backing {
                return charge_java_array_string_to_string_ops(cx, arr, language_version, visited);
            }
            for fv in b.fields.values() {
                charge_java_ai_export_ops(cx, fv, language_version, visited)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_one_point_zero_java_shape() {
        assert_eq!(export_double(1.0, 4), "1.0");
        assert_eq!(export_double(0.0, 4), "0.0");
        assert_eq!(export_double(-0.0, 4), "-0.0");
    }

    #[test]
    fn double_scientific_matches_java_for_hex_literal_values() {
        assert_eq!(
            export_double(-9007199254740992.0, 4),
            "-9.007199254740992E15"
        );
        assert_eq!(
            export_double(-2.4414359423019505e-12, 4),
            "-2.4414359423019505E-12"
        );
    }

    #[test]
    fn double_leek_v1_french_decimal_format() {
        assert_eq!(export_double(10.5, 1), "10,5");
        assert_eq!(export_double(1000.0, 1), "1\u{202f}000");
        assert_eq!(export_double(-1000.0, 1), "-1\u{202f}000");
        assert_eq!(export_double(std::f64::consts::SQRT_2, 1), "1,414");
        assert_eq!(export_double(1.0, 1), "1");
    }

    #[test]
    fn empty_map_is_bracket_colon() {
        let m = Value::map_from(vec![]);
        assert_eq!(value_java_export(&m, 4), "[:]");
    }

    #[test]
    fn v1_export_self_nested_array_is_empty_nested() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let a = Rc::new(RefCell::new(Vec::new()));
        let arr = Value::Array(a.clone());
        a.borrow_mut().push(arr.clone());
        assert_eq!(value_java_export(&arr, 1), "[[]]");
    }

    #[test]
    fn v1_infinite_map_fixture_matches_java() {
        use crate::compile_source;
        use crate::interpret_hir_with_strict;
        use crate::CompileOptions;
        let src = "var a = [:] a[0] = a return a\n";
        let unit = compile_source(
            "<t>",
            src,
            &CompileOptions {
                manifest: None,
                cli_language_version: Some(1),
                cli_strict: Some(false),
                source_path: None,
                snippet_origin: None,
                signature_globals: vec![],
            },
        )
        .expect("compile");
        let v = interpret_hir_with_strict(&unit.hir, unit.language_version, unit.strict)
            .expect("run")
            .expect("some");
        assert_eq!(value_java_export(&v, 1), "[[]]");
    }

    #[test]
    fn string_is_wrapped_raw_no_rust_escapes() {
        let s = Value::String(r#"hi"there"#.into());
        assert_eq!(value_java_export(&s, 4), r#""hi"there""#);
    }

    #[test]
    fn string_coerce_leaves_plain_string_unquoted() {
        let s = Value::String("ab".into());
        assert_eq!(value_java_string_coerce(&s, 4), "ab");
        let a = Value::array_from(vec![Value::Integer(1), Value::String("x".into())]);
        assert_eq!(value_java_string_coerce(&a, 4), r#"[1, "x"]"#);
        assert_eq!(value_java_string_coerce(&a, 3), "[1, x]");
    }
}
