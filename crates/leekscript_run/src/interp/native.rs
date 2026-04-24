//! Java runner–style global natives (`LeekFunctions` / `LeekConstants` in the reference implementation).

use super::context::InterpCx;
use super::core_builtins::try_eval_builtin;
use super::env::Env;
use super::error::InterpretError;
use super::java_export::{
    value_java_string_coerce, ARRAY_CLASS_EXPORT_NATIVE, BOOLEAN_CLASS_EXPORT_NATIVE,
    CLASS_METACLASS_EXPORT_NATIVE, FUNCTION_CLASS_EXPORT_NATIVE, INTEGER_CLASS_EXPORT_NATIVE,
    INTERVAL_CLASS_EXPORT_NATIVE, NULL_CLASS_EXPORT_NATIVE, NUMBER_CLASS_EXPORT_NATIVE,
    OBJECT_CLASS_EXPORT_NATIVE, REAL_CLASS_EXPORT_NATIVE, STRING_CLASS_EXPORT_NATIVE,
};
use super::util::java_real;
use super::value::Value;

/// Re-exported from [`leekscript_resolve::STDLIB_GLOBAL_IDENTIFIERS`] (single source of truth).
pub use leekscript_resolve::STDLIB_GLOBAL_IDENTIFIERS;

pub(super) fn seed_stdlib(env: &mut Env, language_version: u8) {
    for &name in STDLIB_GLOBAL_IDENTIFIERS {
        env.insert(name.to_string(), Value::Native(name));
    }
    env.insert("Infinity".into(), Value::Real(f64::INFINITY));
    env.insert("PI".into(), Value::Real(std::f64::consts::PI));
    env.insert("E".into(), Value::Real(std::f64::consts::E));
    env.insert("NaN".into(), Value::Real(f64::NAN));
    env.insert("Integer".into(), Value::Native(INTEGER_CLASS_EXPORT_NATIVE));
    env.insert("Real".into(), Value::Native(REAL_CLASS_EXPORT_NATIVE));
    env.insert("Number".into(), Value::Native(NUMBER_CLASS_EXPORT_NATIVE));
    if language_version >= 3 {
        env.insert("Array".into(), Value::Native(ARRAY_CLASS_EXPORT_NATIVE));
        env.insert("Null".into(), Value::Native(NULL_CLASS_EXPORT_NATIVE));
        env.insert("String".into(), Value::Native(STRING_CLASS_EXPORT_NATIVE));
        env.insert("Boolean".into(), Value::Native(BOOLEAN_CLASS_EXPORT_NATIVE));
        env.insert("Object".into(), Value::Native(OBJECT_CLASS_EXPORT_NATIVE));
        env.insert(
            "Function".into(),
            Value::Native(FUNCTION_CLASS_EXPORT_NATIVE),
        );
        env.insert("Class".into(), Value::Native(CLASS_METACLASS_EXPORT_NATIVE));
        env.insert(
            "Interval".into(),
            Value::Native(INTERVAL_CLASS_EXPORT_NATIVE),
        );
        // Java exposes these metaclasses from v3+.
        env.insert("Value".into(), Value::UserClass("Value".into()));
        env.insert("JSON".into(), Value::UserClass("JSON".into()));
        env.insert("System".into(), Value::UserClass("System".into()));
    }
    env.insert("SORT_ASC".into(), Value::Integer(0));
    env.insert("SORT_DESC".into(), Value::Integer(1));
}

/// Java `LeekConstants` type codes returned by `typeOf` / unary `typeof` (see `LeekConstants.java`).
fn type_of_code(v: &Value) -> i64 {
    match v {
        Value::Null => 0,
        Value::Integer(_) | Value::Real(_) | Value::RealDotZero(_) => 1,
        Value::Bool(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Function(_) | Value::Native(_) => 5,
        Value::Instance(_) | Value::UserClass(_) | Value::Super => 7,
        Value::Map(..) | Value::Object(..) => 8,
        Value::Set(_) => 9,
        Value::Interval(_) => 10,
    }
}

/// Unary `typeof` / `typeOf(...)` numeric code.
pub(super) fn runtime_typeof_value(v: &Value) -> Value {
    Value::Integer(type_of_code(v))
}

pub(super) fn number_from_value(v: &Value) -> Result<f64, InterpretError> {
    Ok(java_real(v))
}

pub(super) fn f64_to_numeric_value(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
        Value::Integer(f as i64)
    } else {
        Value::Real(f)
    }
}

/// `string(x)` — Java `AI.string` (arrays/maps use the same rendering as `export` for elements).
pub(super) fn string_builtin(v: &Value, language_version: u8) -> String {
    value_java_string_coerce(v, language_version)
}

pub(super) fn eval_native(
    cx: &mut InterpCx,
    name: &str,
    args: &[Value],
    arg_idents: Option<&[Option<String>]>,
) -> Result<Value, InterpretError> {
    if name == ARRAY_CLASS_EXPORT_NATIVE {
        if cx.language_version >= 3 {
            return Ok(Value::array_from(args.to_vec()));
        }
        return Err(InterpretError::function_not_available());
    }
    if name == OBJECT_CLASS_EXPORT_NATIVE {
        if cx.language_version >= 3 {
            if !args.is_empty() {
                return Err(InterpretError::invalid_parameter_count(0, args.len()));
            }
            return Ok(Value::object_from(Vec::new()));
        }
        return Err(InterpretError::function_not_available());
    }
    match try_eval_builtin(cx, name, args, arg_idents)? {
        Some(v) => Ok(v),
        None => {
            let trace = cx.java_style_system_log_trace();
            if let Some(host) = cx.host.as_mut() {
                if let Some(v) = host.call_native(name, args, trace.as_deref())? {
                    let extra = host
                        .java_native_wrapper_ops(name)
                        .saturating_add(host.take_native_dispatch_extra_ops());
                    if extra > 0 {
                        cx.charge_ops(extra)?;
                    }
                    return Ok(v);
                }
            }
            Err(InterpretError::not_callable())
        }
    }
}
