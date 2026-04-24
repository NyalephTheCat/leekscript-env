//! Core globals from `data/signatures/core.sig.leek` (tree interpreter).

use super::call::invoke_value;
use super::context::InterpCx;
use super::error::{ExecAbort, InterpretError};
use super::host::{DebugLogHandled, DebugLogKind};
use super::java_export::charge_java_ai_string_ops;
use super::map_store::MapStore;
use super::native::{
    f64_to_numeric_value, number_from_value, runtime_typeof_value, string_builtin,
};
use super::ram::{self, MAP_RAM_QUADS_PER_ENTRY};
use super::util::{
    array_index_at, eval_in, interval_array_values, interval_is_empty, map_find_key,
    map_find_key_legacy, pass_parameter_value, values_equal_for_compare,
};
use super::value::{IntervalValue, SharedArray, Value};
use serde_json::Value as JsonValue;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;

pub(super) fn try_eval_builtin(
    cx: &mut InterpCx,
    name: &str,
    args: &[Value],
    arg_idents: Option<&[Option<String>]>,
) -> Result<Option<Value>, InterpretError> {
    match dispatch(cx, name, args, arg_idents) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.reference == "NOT_CALLABLE" => Ok(None),
        Err(e) => Err(e),
    }
}

#[inline]
fn require_leek_v4(cx: &InterpCx) -> Result<(), InterpretError> {
    if cx.language_version >= 4 {
        Ok(())
    } else {
        Err(InterpretError::function_not_available())
    }
}

#[inline]
fn require_leek_v1_to_v3(cx: &InterpCx) -> Result<(), InterpretError> {
    if cx.language_version < 4 {
        Ok(())
    } else {
        Err(InterpretError::function_not_available())
    }
}

fn builtin_rand_range_real(name: &str, args: &[Value]) -> Result<Value, InterpretError> {
    expect_arity(name, 2, args.len())?;
    let lo = number_from_value(&args[0])?;
    let hi = number_from_value(&args[1])?;
    let t = fastrand::f64();
    Ok(Value::Real(lo + t * (hi - lo)))
}

fn dispatch_map_extrema(name: &str, args: &[Value]) -> Result<Value, InterpretError> {
    expect_arity(name, 1, args.len())?;
    let m = expect_map(&args[0])?;
    let b = m.borrow();
    if b.is_empty() {
        return Ok(Value::Null);
    }
    let want_max = name == "mapMax";
    let mut cur = b[0].1.clone();
    for (_, v) in b.iter().skip(1) {
        let ord = cmp_sort_values(v, &cur)?;
        let better = if want_max {
            ord == Ordering::Greater
        } else {
            ord == Ordering::Less
        };
        if better {
            cur = v.clone();
        }
    }
    Ok(cur)
}

fn dispatch_interval_combine(args: &[Value]) -> Result<Value, InterpretError> {
    expect_arity("intervalCombine", 2, args.len())?;
    let a = expect_interval(&args[0])?;
    let b = expect_interval(&args[1])?;
    let min = a.min.min(b.min);
    let max = a.max.max(b.max);
    let min_closed = if (a.min - min).abs() < f64::EPSILON {
        a.min_closed
    } else if (b.min - min).abs() < f64::EPSILON {
        b.min_closed
    } else {
        a.min_closed && b.min_closed
    };
    let max_closed = if (a.max - max).abs() < f64::EPSILON {
        a.max_closed
    } else if (b.max - max).abs() < f64::EPSILON {
        b.max_closed
    } else {
        a.max_closed && b.max_closed
    };
    let integer_lattice = a.integer_lattice && b.integer_lattice;
    let export_endpoints_as_real = a.export_endpoints_as_real || b.export_endpoints_as_real;
    Ok(Value::Interval(IntervalValue {
        min_closed,
        min,
        max_closed,
        max,
        integer_lattice,
        export_endpoints_as_real,
        interval_min_neg_inf_from_shorthand: false,
        interval_max_pos_inf_from_shorthand: false,
    }))
}

fn dispatch_interval_intersection(args: &[Value]) -> Result<Value, InterpretError> {
    expect_arity("intervalIntersection", 2, args.len())?;
    let a = expect_interval(&args[0])?;
    let b = expect_interval(&args[1])?;
    let min = a.min.max(b.min);
    let max = a.max.min(b.max);
    let min_closed = if min == a.min {
        a.min_closed
    } else {
        b.min_closed
    };
    let max_closed = if max == a.max {
        a.max_closed
    } else {
        b.max_closed
    };
    let integer_lattice = a.integer_lattice && b.integer_lattice;
    let export_endpoints_as_real = a.export_endpoints_as_real
        || b.export_endpoints_as_real
        || (!a.min.is_finite() && !a.max.is_finite())
        || (!b.min.is_finite() && !b.max.is_finite());
    if min > max {
        return Ok(Value::Interval(IntervalValue {
            min_closed: true,
            min,
            max_closed: true,
            max,
            integer_lattice,
            export_endpoints_as_real,
            interval_min_neg_inf_from_shorthand: false,
            interval_max_pos_inf_from_shorthand: false,
        }));
    }
    if min == max && !(min_closed && max_closed) {
        return Ok(Value::Interval(IntervalValue::default()));
    }
    Ok(Value::Interval(IntervalValue {
        min_closed,
        min,
        max_closed,
        max,
        integer_lattice,
        export_endpoints_as_real,
        interval_min_neg_inf_from_shorthand: false,
        interval_max_pos_inf_from_shorthand: false,
    }))
}

fn emit_leek_debug(
    cx: &mut InterpCx,
    name: &str,
    kind: DebugLogKind,
    args: &[Value],
) -> Result<Value, InterpretError> {
    let (msg_src, color_rgb24) = if kind == DebugLogKind::Colored {
        expect_arity(name, 2, args.len())?;
        let rgb = (int_operand(&args[1])? as u64) as u32 & 0xFF_FF_FF;
        (&args[0], Some(rgb))
    } else {
        expect_arity(name, 1, args.len())?;
        (&args[0], None)
    };
    let mut dbg_visited = HashSet::new();
    charge_java_ai_string_ops(cx, msg_src, cx.language_version, &mut dbg_visited)?;
    let msg = string_builtin(msg_src, cx.language_version);
    // Java `SystemClass.debug` / `debugW` / …: `ai.ops(message.length())` after `ai.string(...)`.
    cx.charge_ops(msg.len() as u64)?;
    let mut print_stderr = true;
    let position = cx
        .pending_call_span
        .and_then(|sp| cx.debug_log_position(sp));
    if let Some(host) = cx.host.as_mut() {
        match host.emit_debug_log(kind, &msg, color_rgb24, position)? {
            DebugLogHandled::Handled => print_stderr = false,
            DebugLogHandled::NotHandled => {}
        }
    }
    if print_stderr {
        match kind {
            DebugLogKind::Info => eprintln!("[leek:debug] {msg}"),
            DebugLogKind::Colored => {
                let c = color_rgb24.unwrap_or(0);
                eprintln!("[leek:debug] #{c:06x} {msg}");
            }
            DebugLogKind::Error => eprintln!("[leek:error] {msg}"),
            DebugLogKind::Warning => eprintln!("[leek:warn] {msg}"),
        }
    }
    Ok(Value::Null)
}

fn dispatch(
    cx: &mut InterpCx,
    name: &str,
    args: &[Value],
    arg_idents: Option<&[Option<String>]>,
) -> Result<Value, InterpretError> {
    // Registry costs are folded into Java `ops(expr, getOperations())` statement wrappers
    // (`java_ops_budget`); do not charge again here.
    match name {
        "getOperations" => {
            expect_arity(name, 0, args.len())?;
            Ok(Value::Integer(cx.operations_used as i64))
        }
        "getInstructionCount" => {
            expect_arity(name, 0, args.len())?;
            let n = cx.operations_used.saturating_sub(cx.turn_operations_start);
            Ok(Value::Integer(n as i64))
        }
        "getMaxOperations" => {
            expect_arity(name, 0, args.len())?;
            let v = cx.operations_limit.map(|x| x as i64).unwrap_or(i64::MAX);
            Ok(Value::Integer(v))
        }
        "getRam" => {
            expect_arity(name, 0, args.len())?;
            Ok(Value::Integer(cx.ram_quads_used as i64))
        }
        "Object" => {
            if cx.language_version < 3 {
                return Err(InterpretError::function_not_available());
            }
            expect_arity(name, 0, args.len())?;
            Ok(Value::object_from(vec![]))
        }
        "abs" => {
            expect_arity(name, 1, args.len())?;
            if cx.strict == Some(true) && matches!(&args[0], Value::Null) {
                return Err(InterpretError::wrong_operand_types_binary());
            }
            match &args[0] {
                Value::Null if cx.language_version == 1 => Ok(Value::Integer(0)),
                Value::Integer(i) => Ok(Value::Integer(i.wrapping_abs())),
                _ => {
                    let r = number_from_value(&args[0])?.abs();
                    if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                        Ok(Value::Integer(r as i64))
                    } else {
                        Ok(Value::Real(r))
                    }
                }
            }
        }
        "acos" => {
            let x = if cx.language_version <= 2 && args.is_empty() {
                0.0
            } else {
                expect_arity(name, 1, args.len())?;
                number_from_value(&args[0])?
            };
            let r = x.acos();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "assocReverse" => {
            require_leek_v1_to_v3(cx)?;
            expect_arity(name, 1, args.len())?;
            let id0 = arg_idents.and_then(|a| a.first()).and_then(|o| o.as_ref());
            match &args[0] {
                Value::Map(m) | Value::Object(m) => {
                    m.borrow_mut().reverse_in_place();
                    Ok(Value::Null)
                }
                Value::Array(a) => {
                    let src = a.borrow().clone();
                    let mut pairs: Vec<(Value, Value)> = src
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| (Value::Integer(i as i64), v))
                        .collect();
                    pairs.reverse();
                    if let Some(var) = id0 {
                        cx.assign_with_ram(var, Value::map_from(pairs))?;
                    } else {
                        let vals: Vec<Value> = pairs.into_iter().map(|(_, v)| v).collect();
                        *a.borrow_mut() = vals;
                    }
                    Ok(Value::Null)
                }
                _ => Err(InterpretError::wrong_operand_types_binary()),
            }
        }
        "assocSort" => {
            require_leek_v1_to_v3(cx)?;
            if args.is_empty() {
                return Err(InterpretError::invalid_parameter_count(1, 0));
            }
            if args.len() > 2 {
                return Err(InterpretError::invalid_parameter_count(2, args.len()));
            }
            let desc = args.len() == 2 && matches!(&args[1], Value::Integer(1));
            let id0 = arg_idents.and_then(|a| a.first()).and_then(|o| o.as_ref());
            match &args[0] {
                Value::Map(m) | Value::Object(m) => {
                    let mut pairs: Vec<(Value, Value)> = m.borrow().to_vec();
                    let string_keys_only = pairs.iter().all(|(k, _)| matches!(k, Value::String(_)));
                    if string_keys_only {
                        pairs.sort_by(|(k1, _), (k2, _)| {
                            let o = cmp_sort_values(k1, k2).unwrap_or(Ordering::Equal);
                            if desc {
                                o.reverse()
                            } else {
                                o
                            }
                        });
                    } else {
                        let mut tagged: Vec<(usize, (Value, Value))> =
                            pairs.into_iter().enumerate().collect();
                        tagged.sort_by(|(t1, (_, v1)), (t2, (_, v2))| {
                            let c = if cx.language_version == 1 {
                                if desc {
                                    cmp_sort_values_v1_nulls_first_desc(v1, v2)
                                } else {
                                    cmp_sort_values_v1_nulls_last(v1, v2)
                                }
                            } else {
                                let mut c = cmp_sort_values(v1, v2).unwrap_or(Ordering::Equal);
                                if desc {
                                    c = c.reverse();
                                }
                                c
                            };
                            c.then_with(|| t1.cmp(t2))
                        });
                        pairs = tagged.into_iter().map(|(_, p)| p).collect();
                    }
                    m.borrow_mut().replace_all(pairs);
                    Ok(Value::Null)
                }
                Value::Array(a) => {
                    let Some(var) = id0 else {
                        return Err(InterpretError::wrong_operand_types_binary());
                    };
                    let src = a.borrow().clone();
                    let mut tagged: Vec<(usize, Value)> = src.into_iter().enumerate().collect();
                    tagged.sort_by(|(i1, v1), (i2, v2)| {
                        let c = if cx.language_version == 1 {
                            if desc {
                                cmp_sort_values_v1_nulls_first_desc(v1, v2)
                            } else {
                                cmp_sort_values_v1_nulls_last(v1, v2)
                            }
                        } else {
                            let mut c = cmp_sort_values(v1, v2).unwrap_or(Ordering::Equal);
                            if desc {
                                c = c.reverse();
                            }
                            c
                        };
                        c.then_with(|| i1.cmp(i2))
                    });
                    let pairs: Vec<(Value, Value)> = tagged
                        .into_iter()
                        .map(|(i, v)| (Value::Integer(i as i64), v))
                        .collect();
                    cx.assign_with_ram(var, Value::map_from(pairs))?;
                    Ok(Value::Null)
                }
                _ => Err(InterpretError::wrong_operand_types_binary()),
            }
        }
        "asin" => {
            let x = if cx.language_version <= 2 && args.is_empty() {
                0.0
            } else {
                expect_arity(name, 1, args.len())?;
                number_from_value(&args[0])?
            };
            let r = x.asin();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "atan" => {
            let x = if cx.language_version <= 2 && args.is_empty() {
                0.0
            } else {
                expect_arity(name, 1, args.len())?;
                number_from_value(&args[0])?
            };
            let r = x.atan();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "atan2" => {
            expect_arity(name, 2, args.len())?;
            let r = number_from_value(&args[0])?.atan2(number_from_value(&args[1])?);
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "average" => {
            expect_arity(name, 1, args.len())?;
            match &args[0] {
                Value::Array(a) => {
                    let b = a.borrow();
                    if b.is_empty() {
                        return Ok(if cx.language_version == 1 {
                            Value::Integer(0)
                        } else {
                            Value::Real(0.0)
                        });
                    }
                    let s: f64 = b
                        .iter()
                        .map(|v| number_from_value(v))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .sum();
                    let avg = s / b.len() as f64;
                    if cx.language_version == 1
                        && avg.is_finite()
                        && avg.fract() == 0.0
                        && avg >= i64::MIN as f64
                        && avg <= i64::MAX as f64
                    {
                        Ok(Value::Integer(avg as i64))
                    } else {
                        Ok(Value::Real(avg))
                    }
                }
                Value::Map(m) | Value::Object(m) => {
                    let b = m.borrow();
                    if b.is_empty() {
                        return Ok(if cx.language_version == 1 {
                            Value::Integer(0)
                        } else {
                            Value::Real(0.0)
                        });
                    }
                    let s: f64 = b
                        .iter()
                        .map(|(_, v)| number_from_value(v))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .sum();
                    let avg = s / b.len() as f64;
                    if cx.language_version == 1
                        && avg.is_finite()
                        && avg.fract() == 0.0
                        && avg >= i64::MIN as f64
                        && avg <= i64::MAX as f64
                    {
                        Ok(Value::Integer(avg as i64))
                    } else {
                        Ok(Value::Real(avg))
                    }
                }
                _ => Err(InterpretError::wrong_operand_types_binary()),
            }
        }
        "binString" => {
            expect_arity(name, 1, args.len())?;
            let i = int_operand(&args[0])?;
            Ok(Value::String(format!("{:b}", i as u64)))
        }
        "bitCount" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Integer(int_operand(&args[0])?.count_ones() as i64))
        }
        "bitReverse" => {
            expect_arity(name, 1, args.len())?;
            let x = int_operand(&args[0])? as u64;
            Ok(Value::Integer(x.reverse_bits() as i64))
        }
        "bitsToReal" => {
            expect_arity(name, 1, args.len())?;
            let bits = int_operand(&args[0])? as u64;
            Ok(Value::Real(f64::from_bits(bits)))
        }
        "byteReverse" => {
            expect_arity(name, 1, args.len())?;
            let x = int_operand(&args[0])? as u64;
            let b = x.to_le_bytes();
            let mut r = b;
            r.reverse();
            Ok(Value::Integer(u64::from_le_bytes(r) as i64))
        }
        "cbrt" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Real(number_from_value(&args[0])?.cbrt()))
        }
        "ceil" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Integer(number_from_value(&args[0])?.ceil() as i64))
        }
        "cos" => {
            let x = if cx.language_version <= 2 && args.is_empty() {
                0.0
            } else {
                expect_arity(name, 1, args.len())?;
                number_from_value(&args[0])?
            };
            let r = x.cos();
            if cx.language_version == 1 && r.is_finite() {
                let rr = r.round();
                if (r - rr).abs() < 1e-12 && rr >= i64::MIN as f64 && rr <= i64::MAX as f64 {
                    Ok(Value::Integer(rr as i64))
                } else {
                    Ok(Value::Real(r))
                }
            } else {
                Ok(Value::Real(r))
            }
        }
        "exp" => {
            expect_arity(name, 1, args.len())?;
            let r = number_from_value(&args[0])?.exp();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "fill" => {
            let a = expect_array(&args[0])?;
            if args.len() != 2 && args.len() != 3 {
                return Err(InterpretError::invalid_parameter_count(2, args.len()));
            }
            let count_full = a.borrow().len();
            let count = if args.len() == 3 {
                int_operand(&args[2])?.max(0) as usize
            } else {
                count_full
            };
            let mut b = a.borrow_mut();
            if args.len() == 3 && b.len() < count {
                let old_len = b.len();
                let delta = (count - old_len) as u64;
                cx.charge_ram_quads(delta)?;
                let fill_v = if cx.language_version == 1 {
                    clone_value_deep(&args[1], 1, None)?
                } else {
                    args[1].clone()
                };
                b.resize(count, fill_v);
            } else {
                let lim = if args.len() == 3 {
                    count.min(b.len())
                } else {
                    b.len()
                };
                for i in 0..lim {
                    b[i] = if cx.language_version == 1 {
                        clone_value_deep(&args[1], 1, None)?
                    } else {
                        args[1].clone()
                    };
                }
            }
            Ok(Value::Null)
        }
        "floor" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Integer(number_from_value(&args[0])?.floor() as i64))
        }
        "hexString" => {
            expect_arity(name, 1, args.len())?;
            let i = int_operand(&args[0])?;
            Ok(Value::String(format!("{:x}", i as u64)))
        }
        "hypot" => {
            expect_arity(name, 2, args.len())?;
            let r = number_from_value(&args[0])?.hypot(number_from_value(&args[1])?);
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "isFinite" => {
            expect_arity(name, 1, args.len())?;
            let r = match &args[0] {
                Value::Real(x) => *x,
                Value::Integer(_) => return Ok(Value::Bool(true)),
                _ => return Err(InterpretError::wrong_operand_types_binary()),
            };
            Ok(Value::Bool(r.is_finite()))
        }
        "isInfinite" => {
            expect_arity(name, 1, args.len())?;
            let r = match &args[0] {
                Value::Real(x) => *x,
                Value::Integer(_) => return Ok(Value::Bool(false)),
                _ => return Err(InterpretError::wrong_operand_types_binary()),
            };
            Ok(Value::Bool(r.is_infinite()))
        }
        "isNaN" => {
            expect_arity(name, 1, args.len())?;
            let r = match &args[0] {
                Value::Real(x) => *x,
                Value::Integer(_) => return Ok(Value::Bool(false)),
                _ => return Err(InterpretError::wrong_operand_types_binary()),
            };
            Ok(Value::Bool(r.is_nan()))
        }
        "isPermutation" => {
            expect_arity(name, 2, args.len())?;
            let da = digit_counts(int_operand(&args[0])?);
            let db = digit_counts(int_operand(&args[1])?);
            Ok(Value::Bool(da == db))
        }
        "leadingZeros" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Integer(int_operand(&args[0])?.leading_zeros() as i64))
        }
        "log" => {
            expect_arity(name, 1, args.len())?;
            let r = number_from_value(&args[0])?.ln();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "log10" => {
            expect_arity(name, 1, args.len())?;
            let r = number_from_value(&args[0])?.log10();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "log2" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Real(number_from_value(&args[0])?.log2()))
        }
        "max" => {
            expect_arity(name, 2, args.len())?;
            match (&args[0], &args[1]) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(*a.max(b))),
                _ => {
                    let r = number_from_value(&args[0])?.max(number_from_value(&args[1])?);
                    if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                        Ok(Value::Integer(r as i64))
                    } else {
                        Ok(Value::Real(r))
                    }
                }
            }
        }
        "min" => {
            expect_arity(name, 2, args.len())?;
            match (&args[0], &args[1]) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(*a.min(b))),
                _ => {
                    let r = number_from_value(&args[0])?.min(number_from_value(&args[1])?);
                    if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                        Ok(Value::Integer(r as i64))
                    } else {
                        Ok(Value::Real(r))
                    }
                }
            }
        }
        "pow" => {
            expect_arity(name, 2, args.len())?;
            let a = number_from_value(&args[0])?;
            let b = number_from_value(&args[1])?;
            let x = a.powf(b);
            if x.is_nan() {
                return Err(InterpretError::wrong_operand_types_binary());
            }
            Ok(Value::Real(x))
        }
        "rand" => {
            expect_arity(name, 0, args.len())?;
            Ok(Value::Real(fastrand::f64()))
        }
        "randFloat" => {
            require_leek_v1_to_v3(cx)?;
            builtin_rand_range_real(name, args)
        }
        "randReal" => {
            require_leek_v4(cx)?;
            builtin_rand_range_real(name, args)
        }
        "randInt" => {
            expect_arity(name, 2, args.len())?;
            let a = int_operand(&args[0])?;
            let b = int_operand(&args[1])?;
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            // Java `randInt(a, b)` samples uniformly in the half-open range [lo, hi).
            if lo > hi {
                return Err(InterpretError::randint_empty_range());
            }
            if lo == hi {
                return Ok(Value::Integer(lo));
            }
            Ok(Value::Integer(fastrand::i64(lo..hi)))
        }
        "realBits" => {
            expect_arity(name, 1, args.len())?;
            let r = number_from_value(&args[0])?;
            Ok(Value::Integer(r.to_bits() as i64))
        }
        "rotateLeft" => {
            expect_arity(name, 2, args.len())?;
            let x = int_operand(&args[0])?;
            let n = int_operand(&args[1])?.rem_euclid(64) as u32;
            Ok(Value::Integer(x.rotate_left(n)))
        }
        "rotateRight" => {
            expect_arity(name, 2, args.len())?;
            let x = int_operand(&args[0])?;
            let n = int_operand(&args[1])?.rem_euclid(64) as u32;
            Ok(Value::Integer(x.rotate_right(n)))
        }
        "round" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Integer(number_from_value(&args[0])?.round() as i64))
        }
        "signum" => {
            expect_arity(name, 1, args.len())?;
            match &args[0] {
                Value::Integer(i) => Ok(Value::Integer(i.signum())),
                _ => {
                    let x = number_from_value(&args[0])?;
                    Ok(Value::Integer(x.signum() as i64))
                }
            }
        }
        "sin" => {
            let x = if cx.language_version <= 2 && args.is_empty() {
                0.0
            } else {
                expect_arity(name, 1, args.len())?;
                number_from_value(&args[0])?
            };
            let r = x.sin();
            if cx.language_version == 1 && r.is_finite() {
                let rr = r.round();
                if (r - rr).abs() < 1e-12 && rr >= i64::MIN as f64 && rr <= i64::MAX as f64 {
                    Ok(Value::Integer(rr as i64))
                } else {
                    Ok(Value::Real(r))
                }
            } else {
                Ok(Value::Real(r))
            }
        }
        "sqrt" => {
            let x = if cx.language_version <= 2 {
                if args.is_empty() {
                    0.0
                } else {
                    number_from_value(&args[0])?
                }
            } else {
                expect_arity(name, 1, args.len())?;
                number_from_value(&args[0])?
            };
            let r = x.sqrt();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "tan" => {
            let x = if cx.language_version <= 2 && args.is_empty() {
                0.0
            } else {
                expect_arity(name, 1, args.len())?;
                number_from_value(&args[0])?
            };
            let r = x.tan();
            if cx.language_version == 1 && r.is_finite() {
                let rr = r.round();
                if rr == 0.0 {
                    // Preserve signed zero for v1 export (`-0`).
                    Ok(Value::Real(r))
                } else if (r - rr).abs() < 1e-12 && rr >= i64::MIN as f64 && rr <= i64::MAX as f64 {
                    Ok(Value::Integer(rr as i64))
                } else {
                    Ok(Value::Real(r))
                }
            } else {
                Ok(Value::Real(r))
            }
        }
        "toDegrees" => {
            expect_arity(name, 1, args.len())?;
            let r = number_from_value(&args[0])?.to_degrees();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "toRadians" => {
            expect_arity(name, 1, args.len())?;
            let r = number_from_value(&args[0])?.to_radians();
            if cx.language_version == 1 && r.is_finite() && r.fract() == 0.0 {
                Ok(Value::Integer(r as i64))
            } else {
                Ok(Value::Real(r))
            }
        }
        "trailingZeros" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Integer(
                int_operand(&args[0])?.trailing_zeros() as i64
            ))
        }

        "charAt" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_str(&args[0])?;
            let i = int_operand(&args[1])?;
            let idx = char_index(i, s.chars().count())?;
            let ch = s.chars().nth(idx).unwrap_or_default();
            Ok(Value::String(ch.to_string()))
        }
        "codePointAt" => {
            if args.len() != 1 && args.len() != 2 {
                return Err(InterpretError::invalid_parameter_count(2, args.len()));
            }
            let s = expect_str(&args[0])?;
            let i = if args.len() == 2 {
                int_operand(&args[1])?
            } else {
                0
            };
            Ok(Value::Integer(code_point_at_java(s.as_str(), i)?))
        }
        "contains" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_str(&args[0])?;
            let sub = expect_str(&args[1])?;
            Ok(Value::Bool(s.contains(sub.as_str())))
        }
        "endsWith" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_str(&args[0])?;
            let suf = expect_str(&args[1])?;
            Ok(Value::Bool(s.ends_with(suf.as_str())))
        }
        "indexOf" => index_of_dispatch(cx, args),
        "search" => search_dispatch(cx, args),
        "length" => {
            expect_arity(name, 1, args.len())?;
            Ok(match &args[0] {
                Value::String(s) => Value::Integer(s.chars().count() as i64),
                Value::Array(a) => Value::Integer(a.borrow().len() as i64),
                Value::Map(m) | Value::Object(m) => Value::Integer(m.borrow().len() as i64),
                Value::Set(s) => Value::Integer(s.borrow().elems.len() as i64),
                _ => return Err(InterpretError::wrong_operand_types_binary()),
            })
        }
        "replace" => {
            expect_arity(name, 3, args.len())?;
            let s = expect_str(&args[0])?;
            let oldc = expect_str(&args[1])?;
            let newc = expect_str(&args[2])?;
            Ok(Value::String(s.replacen(
                oldc.as_str(),
                newc.as_str(),
                usize::MAX,
            )))
        }
        "split" => split_string(args),
        "startsWith" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_str(&args[0])?;
            let p = expect_str(&args[1])?;
            Ok(Value::Bool(s.starts_with(p.as_str())))
        }
        "string" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::String(string_builtin(&args[0], cx.language_version)))
        }
        "substring" => {
            expect_arity(name, 3, args.len())?;
            let s = expect_str(&args[0])?;
            let start = int_operand(&args[1])?;
            let len = int_operand(&args[2])?;
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len();
            let st = char_index(start, n)?;
            let le = len.max(0) as usize;
            let end = (st + le).min(n);
            Ok(Value::String(chars[st..end].iter().collect()))
        }
        "subString" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_str(&args[0])?;
            let start = int_operand(&args[1])?;
            let chars: Vec<char> = s.chars().collect();
            let n = chars.len();
            let st = char_index(start, n)?;
            Ok(Value::String(chars[st..n].iter().collect()))
        }
        "toLower" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::String(expect_str(&args[0])?.to_lowercase()))
        }
        "toUpper" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::String(expect_str(&args[0])?.to_uppercase()))
        }

        "arrayChunk" => {
            expect_arity(name, 2, args.len())?;
            let a = expect_array(&args[0])?;
            let sz = int_operand(&args[1])?;
            // Java `arrayChunk`: non-positive size behaves like `1` (one element per chunk).
            let sz = if sz < 1 { 1 } else { sz };
            let b = a.borrow();
            let chunks: Vec<Value> = b
                .chunks(sz as usize)
                .map(|c| Value::array_from(c.to_vec()))
                .collect();
            Ok(Value::array_from(chunks))
        }
        "arrayClear" => {
            expect_arity(name, 1, args.len())?;
            let a = expect_array(&args[0])?;
            let n = a.borrow().len() as u64;
            a.borrow_mut().clear();
            cx.release_ram_quads(n);
            Ok(Value::Null)
        }
        "arrayConcat" => {
            expect_arity(name, 2, args.len())?;
            let a1 = expect_array(&args[0])?;
            let a2 = expect_array(&args[1])?;
            let mut out = a1.borrow().clone();
            out.extend(a2.borrow().iter().cloned());
            cx.charge_ram_quads(out.len() as u64)?;
            Ok(Value::array_from(out))
        }
        "arrayEvery" => {
            expect_arity(name, 2, args.len())?;
            let arr = expect_array(&args[0])?;
            let cb = &args[1];
            let len = arr.borrow().len();
            for i in 0..len {
                let x = arr.borrow()[i].clone();
                let r = invoke_array_pred(cx, cb, x, i as i64, &Value::Array(arr.clone()))?;
                if !value_truthy_native(&r) {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }
        "arrayFind" => {
            expect_arity(name, 2, args.len())?;
            let arr = expect_array(&args[0])?;
            let cb = &args[1];
            let len = arr.borrow().len();
            for i in 0..len {
                let x = arr.borrow()[i].clone();
                let r = invoke_array_pred(cx, cb, x.clone(), i as i64, &Value::Array(arr.clone()))?;
                if value_truthy_native(&r) {
                    return Ok(x);
                }
            }
            Ok(Value::Null)
        }
        "arrayFilter" => {
            expect_arity(name, 2, args.len())?;
            let arr = expect_array(&args[0])?;
            let cb = &args[1];
            let len = arr.borrow().len();
            // Leek v1: result is a bracket map `index : value` (legacy `arrayFilter`); v2+ is a plain array.
            if cx.language_version == 1 {
                let mut out: Vec<(Value, Value)> = Vec::new();
                for i in 0..len {
                    let x = arr.borrow()[i].clone();
                    let r = invoke_array_pred(cx, cb, x, i as i64, &Value::Array(arr.clone()))?;
                    if value_truthy_native(&r) {
                        out.push((Value::Integer(i as i64), arr.borrow()[i].clone()));
                    }
                }
                cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY * out.len() as u64)?;
                Ok(Value::map_from(out))
            } else {
                let mut out = Vec::new();
                for i in 0..len {
                    let x = arr.borrow()[i].clone();
                    let r = invoke_array_pred(cx, cb, x, i as i64, &Value::Array(arr.clone()))?;
                    if value_truthy_native(&r) {
                        out.push(arr.borrow()[i].clone());
                    }
                }
                cx.charge_ram_quads(out.len() as u64)?;
                Ok(Value::array_from(out))
            }
        }
        "arrayFlatten" => {
            let depth = if args.len() == 1 {
                1
            } else if args.len() == 2 {
                int_operand(&args[1])?
            } else {
                return Err(InterpretError::invalid_parameter_count(1, args.len()));
            };
            let a = expect_array(&args[0])?;
            let flat = {
                let b = a.borrow();
                flatten_array(&b, depth)?
            };
            Ok(Value::array_from(flat))
        }
        "arrayFoldLeft" => array_fold(cx, args, false),
        "arrayFoldRight" => array_fold(cx, args, true),
        "arrayFrequencies" => {
            expect_arity(name, 1, args.len())?;
            let a = expect_array(&args[0])?;
            let mut counts = MapStore::new();
            for x in a.borrow().iter() {
                if let Some(p) = counts.find_key(x) {
                    let c = int_from_val(&counts[p].1)? + 1;
                    counts[p].1 = Value::Integer(c);
                } else {
                    counts.push_kv(x.clone(), Value::Integer(1));
                }
            }
            Ok(Value::map_from(counts.to_vec()))
        }
        "arrayGet" => {
            require_leek_v4(cx)?;
            if !(2..=3).contains(&args.len()) {
                return Err(InterpretError::invalid_parameter_count(2, args.len()));
            }
            let a = expect_array(&args[0])?;
            let i = int_operand(&args[1])?;
            let def = if args.len() == 3 {
                args[2].clone()
            } else {
                Value::Null
            };
            let b = a.borrow();
            let n = b.len();
            if n == 0 {
                return Ok(def);
            }
            let idx = match array_index_ok(i, n) {
                Ok(u) => u,
                Err(_) => return Ok(def),
            };
            Ok(b[idx].clone())
        }
        "arrayGetOrElse" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let a = expect_array(&args[0])?;
            let i = int_operand(&args[1])?;
            let b = a.borrow();
            let n = b.len();
            if n == 0 {
                return Ok(Value::Null);
            }
            let idx = array_index_ok(i, n).ok();
            Ok(match idx {
                Some(u) => b[u].clone(),
                None => Value::Null,
            })
        }
        "arrayIter" => {
            expect_arity(name, 2, args.len())?;
            let arr = expect_array(&args[0])?;
            let cb = &args[1];
            let b = arr.borrow().clone();
            for (i, x) in b.iter().enumerate() {
                invoke_array_cb_void(cx, cb, x.clone(), i as i64, &Value::Array(arr.clone()))?;
            }
            Ok(Value::Null)
        }
        "arrayMap" => {
            expect_arity(name, 2, args.len())?;
            let cb = &args[1];
            match &args[0] {
                // Legacy bracket maps are `Type.ARRAY` in the Java compiler but still use key/value
                // callbacks like `arrayMap(['a': 1], …)` (see `TestMap.java` VM cases).
                Value::Map(m) => {
                    let b = m.borrow().clone();
                    let mut out = Vec::with_capacity(b.len());
                    let snap = args[0].clone();
                    for (k, v) in b.iter() {
                        let y = invoke_map_cb(cx, cb, v.clone(), k.clone(), &snap)?;
                        out.push((k.clone(), y));
                    }
                    cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY * out.len() as u64)?;
                    Ok(Value::wrap_keyed_pairs(&args[0], out))
                }
                Value::Array(arr) => {
                    let len = arr.borrow().len();
                    let mut out = Vec::with_capacity(len);
                    for i in 0..len {
                        let x = arr.borrow()[i].clone();
                        let y = invoke_array_cb(cx, cb, x, i as i64, &Value::Array(arr.clone()))?;
                        out.push(y);
                    }
                    cx.charge_ram_quads(out.len() as u64)?;
                    Ok(Value::array_from(out))
                }
                _ => Err(InterpretError::wrong_operand_types_binary()),
            }
        }
        "arrayMax" => {
            expect_arity(name, 1, args.len())?;
            array_extrema_by_value(&args[0], true)
        }
        "arrayMin" => {
            expect_arity(name, 1, args.len())?;
            array_extrema_by_value(&args[0], false)
        }
        "arrayPartition" => {
            expect_arity(name, 2, args.len())?;
            let arr = expect_array(&args[0])?;
            let cb = &args[1];
            let len = arr.borrow().len();
            // Leek v1–3: legacy `LegacyArrayLeekValue` buckets — dense `0..n-1` keys stringify as a
            // plain array; gaps / non-zero starts export as bracket maps (see `TestArray.partition`).
            if cx.language_version < 4 {
                let mut t: Vec<(i64, Value)> = Vec::new();
                let mut f: Vec<(i64, Value)> = Vec::new();
                for i in 0..len {
                    let x = arr.borrow()[i].clone();
                    let r = invoke_array_pred(cx, cb, x, i as i64, &Value::Array(arr.clone()))?;
                    let elem = arr.borrow()[i].clone();
                    let pair = (i as i64, elem);
                    if value_truthy_native(&r) {
                        t.push(pair);
                    } else {
                        f.push(pair);
                    }
                }
                Ok(Value::array_from(vec![
                    legacy_partition_bucket_export(t),
                    legacy_partition_bucket_export(f),
                ]))
            } else {
                let mut t = Vec::new();
                let mut f = Vec::new();
                for i in 0..len {
                    let x = arr.borrow()[i].clone();
                    let r = invoke_array_pred(cx, cb, x, i as i64, &Value::Array(arr.clone()))?;
                    let elem = arr.borrow()[i].clone();
                    if value_truthy_native(&r) {
                        t.push(elem);
                    } else {
                        f.push(elem);
                    }
                }
                let nt = t.len() as u64;
                let nf = f.len() as u64;
                cx.charge_ram_quads(2 + nt + nf)?;
                Ok(Value::array_from(vec![
                    Value::array_from(t),
                    Value::array_from(f),
                ]))
            }
        }
        "arrayRandom" => {
            let (a, count) = match args.len() {
                1 => (expect_array(&args[0])?, 1i64),
                2 => (expect_array(&args[0])?, int_operand(&args[1])?),
                _ => return Err(InterpretError::invalid_parameter_count(1, args.len())),
            };
            let mut b = a.borrow().clone();
            fastrand::shuffle(&mut b);
            let n = b.len() as i64;
            let final_count = count.max(0).min(n) as usize;
            b.truncate(final_count);
            Ok(Value::array_from(b))
        }
        "arrayRemove" => {
            expect_arity(name, 2, args.len())?;
            let a = expect_array(&args[0])?;
            let v = args[1].clone();
            let mut b = a.borrow_mut();
            let old_len = b.len();
            b.retain(|x| !values_equal_for_compare(x, &v));
            cx.release_ram_quads((old_len - b.len()) as u64);
            Ok(Value::Array(a.clone()))
        }
        "arrayRemoveAll" => {
            expect_arity(name, 2, args.len())?;
            let a = expect_array(&args[0])?;
            let v = args[1].clone();
            let mut b = a.borrow_mut();
            let old_len = b.len();
            b.retain(|x| !values_equal_for_compare(x, &v));
            cx.release_ram_quads((old_len - b.len()) as u64);
            Ok(Value::Array(a.clone()))
        }
        "arraySlice" => array_slice_dispatch(cx, args),
        "arraySome" => {
            expect_arity(name, 2, args.len())?;
            let arr = expect_array(&args[0])?;
            let cb = &args[1];
            let len = arr.borrow().len();
            for i in 0..len {
                let x = arr.borrow()[i].clone();
                let r = invoke_array_pred(cx, cb, x, i as i64, &Value::Array(arr.clone()))?;
                if value_truthy_native(&r) {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }
        "arraySort" => array_sort_native(cx, args),
        "arrayToSet" => {
            expect_arity(name, 1, args.len())?;
            let a = expect_array(&args[0])?;
            let mut s = Vec::new();
            for x in a.borrow().iter() {
                if !s.iter().any(|y| values_equal_for_compare(y, x)) {
                    s.push(x.clone());
                }
            }
            Ok(Value::set_from(s))
        }
        "arrayUnique" => {
            expect_arity(name, 1, args.len())?;
            let a = expect_array(&args[0])?;
            let mut out = Vec::new();
            for x in a.borrow().iter() {
                if !out.iter().any(|y| values_equal_for_compare(y, x)) {
                    out.push(x.clone());
                }
            }
            Ok(Value::array_from(out))
        }
        "count" => {
            expect_arity(name, 1, args.len())?;
            let v = &args[0];
            Ok(match v {
                Value::Array(a) => Value::Integer(a.borrow().len() as i64),
                // v1–v3: `count` is only defined on legacy arrays; bracket maps are not supported.
                Value::Map(_) | Value::Object(_) if cx.language_version <= 3 => {
                    return Err(InterpretError {
                        reference: "NONE",
                        message: "`count` on a map is not supported in Leek v1–v3".into(),
                    });
                }
                // v4 strict: type error on non-array containers (parity suite).
                Value::Map(_) | Value::Object(_)
                    if cx.language_version >= 4 && cx.strict == Some(true) =>
                {
                    return Err(InterpretError::wrong_operand_types_binary());
                }
                Value::Map(m) | Value::Object(m) => Value::Integer(m.borrow().len() as i64),
                Value::Set(s) => Value::Integer(s.borrow().elems.len() as i64),
                _ if cx.strict == Some(true) => {
                    return Err(InterpretError::wrong_operand_types_binary());
                }
                _ => Value::Integer(0),
            })
        }
        "insert" => {
            expect_arity(name, 3, args.len())?;
            let a = expect_array(&args[0])?;
            let v = args[1].clone();
            let idx = int_operand(&args[2])?.max(0) as usize;
            let mut b = a.borrow_mut();
            let at = idx.min(b.len());
            cx.charge_ram_quads(1)?;
            b.insert(at, v);
            Ok(Value::Null)
        }
        "inArray" => {
            expect_arity(name, 2, args.len())?;
            let v = &args[1];
            let hit = match &args[0] {
                Value::Array(a) => {
                    let b = a.borrow();
                    b.iter().any(|x| values_equal_for_compare(x, v))
                }
                Value::Map(m) | Value::Object(m) => {
                    let b = m.borrow();
                    b.as_slice()
                        .iter()
                        .any(|(_, x)| values_equal_for_compare(x, v))
                }
                _ => return Err(InterpretError::wrong_operand_types_binary()),
            };
            Ok(Value::Bool(hit))
        }
        "isEmpty" => {
            expect_arity(name, 1, args.len())?;
            Ok(match &args[0] {
                Value::Array(a) => Value::Bool(a.borrow().is_empty()),
                Value::Map(m) | Value::Object(m) => Value::Bool(m.borrow().is_empty()),
                Value::Set(s) => Value::Bool(s.borrow().elems.is_empty()),
                _ => return Err(InterpretError::wrong_operand_types_binary()),
            })
        }
        "join" => {
            expect_arity(name, 2, args.len())?;
            let a = expect_array(&args[0])?;
            let sep = expect_str(&args[1])?;
            let parts: Vec<String> = a
                .borrow()
                .iter()
                .map(|v| string_builtin(v, cx.language_version))
                .collect();
            Ok(Value::String(parts.join(sep.as_str())))
        }
        "pop" => {
            expect_arity(name, 1, args.len())?;
            let a = expect_array(&args[0])?;
            let mut b = a.borrow_mut();
            Ok(if b.is_empty() {
                Value::Null
            } else {
                cx.release_ram_quads(1);
                b.pop().expect("non-empty")
            })
        }
        "removeElement" => {
            expect_arity(name, 2, args.len())?;
            let want = &args[1];
            let id0 = arg_idents.and_then(|a| a.first()).and_then(|o| o.as_ref());
            if cx.language_version < 4 {
                match &args[0] {
                    Value::Array(a) => {
                        if let Some(var) = id0 {
                            let b = a.borrow();
                            let mut pairs: Vec<(Value, Value)> = b
                                .iter()
                                .enumerate()
                                .map(|(i, v)| (Value::Integer(i as i64), v.clone()))
                                .collect();
                            drop(b);
                            if let Some(p) = pairs
                                .iter()
                                .position(|(_, v)| values_equal_for_compare(v, want))
                            {
                                pairs.remove(p);
                            }
                            cx.assign_with_ram(var, Value::map_from(pairs))?;
                        } else {
                            let mut b = a.borrow_mut();
                            if let Some(i) =
                                b.iter().position(|x| values_equal_for_compare(x, want))
                            {
                                cx.release_ram_quads(1);
                                b.remove(i);
                            }
                        }
                        return Ok(Value::Null);
                    }
                    _ => return Err(InterpretError::wrong_operand_types_binary()),
                }
            }
            let a = expect_array(&args[0])?;
            let mut b = a.borrow_mut();
            if let Some(i) = b.iter().position(|x| values_equal_for_compare(x, want)) {
                cx.release_ram_quads(1);
                b.remove(i);
            }
            Ok(Value::Null)
        }
        "removeKey" => {
            require_leek_v1_to_v3(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_keyed(&args[0])?;
            let want = &args[1];
            let mut b = m.borrow_mut();
            if let Some(p) = map_find_key_legacy(&b, want) {
                let _ = b.remove_ordered(p);
            }
            Ok(Value::Null)
        }
        "remove" => remove_dispatch(cx, args),
        "push" => {
            expect_arity(name, 2, args.len())?;
            let a = expect_array(&args[0])?;
            let v = if cx.language_version == 1 {
                clone_value_deep(&args[1], 64, None)?
            } else {
                args[1].clone()
            };
            cx.charge_ram_quads(1)?;
            a.borrow_mut().push(v);
            Ok(Value::Null)
        }
        "pushAll" => {
            expect_arity(name, 2, args.len())?;
            let a = expect_array(&args[0])?;
            let src = expect_array(&args[1])?;
            let items = src.borrow().clone();
            let mut b = a.borrow_mut();
            if cx.language_version == 1 {
                for it in items {
                    cx.charge_ram_quads(1)?;
                    b.push(clone_value_deep(&it, 1, None)?);
                }
            } else {
                cx.charge_ram_quads(items.len() as u64)?;
                b.extend(items);
            }
            Ok(Value::Null)
        }
        "shift" => {
            expect_arity(name, 1, args.len())?;
            let a = expect_array(&args[0])?;
            let mut b = a.borrow_mut();
            Ok(if b.is_empty() {
                Value::Null
            } else {
                cx.release_ram_quads(1);
                b.remove(0)
            })
        }
        "subArray" => sub_array_dispatch(cx, args),
        "sum" => sum_dispatch(cx, &args[0]),
        "resize" => {
            if args.len() != 2 && args.len() != 3 {
                return Err(InterpretError::invalid_parameter_count(2, args.len()));
            }
            let a = expect_array(&args[0])?;
            let fill = args[1].clone();
            let new_len = if args.len() == 3 {
                int_operand(&args[2])? as usize
            } else {
                a.borrow().len()
            };
            let mut b = a.borrow_mut();
            let old_len = b.len();
            if new_len > old_len {
                cx.charge_ram_quads((new_len - old_len) as u64)?;
                b.resize(new_len, fill);
            } else {
                if new_len < old_len {
                    cx.release_ram_quads((old_len - new_len) as u64);
                }
                b.truncate(new_len);
                for slot in b.iter_mut() {
                    *slot = fill.clone();
                }
            }
            Ok(Value::Array(a.clone()))
        }
        "reverse" => {
            expect_arity(name, 1, args.len())?;
            let a = expect_array(&args[0])?;
            a.borrow_mut().reverse();
            Ok(Value::Null)
        }
        "shuffle" => {
            expect_arity(name, 1, args.len())?;
            let a = expect_array(&args[0])?;
            fastrand::shuffle(&mut *a.borrow_mut());
            Ok(Value::Null)
        }
        "sort" => sort_builtin(cx, args),
        "unknown" => {
            expect_arity(name, 1, args.len())?;
            Ok(args[0].clone())
        }
        "unshift" => {
            expect_arity(name, 2, args.len())?;
            let a = expect_array(&args[0])?;
            let v = if cx.language_version == 1 {
                clone_value_deep(&args[1], 1, None)?
            } else {
                args[1].clone()
            };
            cx.charge_ram_quads(1)?;
            a.borrow_mut().insert(0, v);
            Ok(Value::Null)
        }

        "mapAverage" => {
            require_leek_v4(cx)?;
            expect_arity(name, 1, args.len())?;
            let m = expect_map(&args[0])?;
            let b = m.borrow();
            if b.is_empty() {
                return Ok(Value::Real(0.0));
            }
            let s: f64 = b
                .iter()
                .map(|(_, v)| number_from_value(v))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .sum();
            Ok(Value::Real(s / b.len() as f64))
        }
        "mapClear" => {
            require_leek_v4(cx)?;
            expect_arity(name, 1, args.len())?;
            let m = expect_map(&args[0])?;
            let n = m.borrow().len();
            cx.release_ram_quads(MAP_RAM_QUADS_PER_ENTRY * n as u64);
            m.borrow_mut().clear();
            Ok(args[0].clone())
        }
        "mapContains" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let v = &args[1];
            let hit = {
                let b = m.borrow();
                b.as_slice()
                    .iter()
                    .any(|(_, vv)| values_equal_for_compare(vv, v))
            };
            Ok(Value::Bool(hit))
        }
        "mapContainsKey" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let has = {
                let b = m.borrow();
                map_find_key(&b, &args[1]).is_some()
            };
            Ok(Value::Bool(has))
        }
        "mapEvery" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let cb = &args[1];
            let b = m.borrow().clone();
            for (k, v) in b.iter() {
                let r = invoke_map_every_pred(cx, cb, v.clone(), k.clone())?;
                if !value_truthy_native(&r) {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }
        "mapFilter" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let cb = &args[1];
            let b = m.borrow().clone();
            let mut out = Vec::new();
            let snap = args[0].clone();
            for (k, v) in b.iter() {
                let r = invoke_map_pred(cx, cb, v.clone(), k.clone(), &snap)?;
                if value_truthy_native(&r) {
                    out.push((k.clone(), v.clone()));
                }
            }
            // Java `mapFilter`: result grows via `set` (+2 per entry) plus an extra `increaseRAM` on
            // the source map (+2 per result entry).
            cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY * 2 * out.len() as u64)?;
            Ok(Value::wrap_keyed_pairs(&args[0], out))
        }
        "mapFold" => {
            require_leek_v4(cx)?;
            expect_arity(name, 3, args.len())?;
            let m = expect_map(&args[0])?;
            let cb = &args[1];
            let mut acc = args[2].clone();
            let pairs = m.borrow().clone();
            let snap = args[0].clone();
            for (k, v) in pairs {
                acc = invoke_fold_map(cx, cb, acc, v, k, &snap)?;
            }
            Ok(acc)
        }
        "mapGet" => {
            require_leek_v4(cx)?;
            if !(2..=3).contains(&args.len()) {
                return Err(InterpretError::invalid_parameter_count(2, args.len()));
            }
            let m = expect_map(&args[0])?;
            let k = &args[1];
            let def = if args.len() == 3 {
                args[2].clone()
            } else {
                Value::Null
            };
            let b = m.borrow();
            if let Some(p) = map_find_key(&b, k) {
                Ok(b[p].1.clone())
            } else {
                Ok(def)
            }
        }
        "mapIsEmpty" => {
            require_leek_v4(cx)?;
            expect_arity(name, 1, args.len())?;
            Ok(Value::Bool(expect_map(&args[0])?.borrow().is_empty()))
        }
        "keySort" => {
            require_leek_v1_to_v3(cx)?;
            if args.is_empty() {
                return Err(InterpretError::invalid_parameter_count(1, 0));
            }
            if args.len() > 2 {
                return Err(InterpretError::invalid_parameter_count(2, args.len()));
            }
            let id0 = arg_idents.and_then(|a| a.first()).and_then(|o| o.as_ref());
            let m = expect_keyed(&args[0])?;
            let desc = args.len() == 2 && matches!(&args[1], Value::Integer(1));
            let mut pairs = m.borrow().to_vec();
            pairs.sort_by(|(k1, _), (k2, _)| {
                let ord = cmp_sort_values(k1, k2).unwrap_or(Ordering::Equal);
                if desc {
                    ord.reverse()
                } else {
                    ord
                }
            });
            if let Some(vals) = try_keysort_dense_array_values(&pairs) {
                if let Some(var) = id0 {
                    cx.assign_with_ram(var, Value::array_from(vals))?;
                } else {
                    let rebuilt: Vec<(Value, Value)> = vals
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| (Value::Integer(i as i64), v))
                        .collect();
                    m.borrow_mut().replace_all(rebuilt);
                }
            } else {
                m.borrow_mut().replace_all(pairs);
            }
            Ok(Value::Null)
        }
        "mapIter" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let cb = &args[1];
            let pairs = m.borrow().clone();
            let snap = args[0].clone();
            for (k, v) in pairs {
                invoke_map_cb_void(cx, cb, v, k, &snap)?;
            }
            Ok(Value::Null)
        }
        "mapSearch" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let want = &args[1];
            let b = m.borrow();
            for (k, v) in b.iter() {
                // `mapSearch` compares by *structural* equality (not pointer identity).
                if values_equal_for_compare(v, want) {
                    return Ok(k.clone());
                }
            }
            Ok(Value::Null)
        }
        "mapKeys" => {
            require_leek_v4(cx)?;
            expect_arity(name, 1, args.len())?;
            let m = expect_map(&args[0])?;
            let b = m.borrow();
            cx.charge_ram_quads(b.len() as u64)?;
            let keys: Vec<Value> = b.as_slice().iter().map(|(k, _)| k.clone()).collect();
            Ok(Value::array_from(keys))
        }
        "mapMap" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let cb = &args[1];
            let mut out = Vec::new();
            let pairs = m.borrow().clone();
            let snap = args[0].clone();
            for (k, v) in pairs {
                let nv = invoke_map_cb(cx, cb, v.clone(), k.clone(), &snap)?;
                out.push((k, nv));
            }
            // Java `mapMap`: same double accounting pattern as `mapFilter` on the VM.
            cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY * 2 * out.len() as u64)?;
            Ok(Value::wrap_keyed_pairs(&args[0], out))
        }
        "mapMax" => {
            require_leek_v4(cx)?;
            dispatch_map_extrema(name, args)
        }
        "mapMin" => {
            require_leek_v4(cx)?;
            dispatch_map_extrema(name, args)
        }
        "mapMerge" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m1 = expect_map(&args[0])?;
            let m2 = expect_map(&args[1])?;
            let mut acc = m1.borrow().clone();
            for (k, v) in m2.borrow().iter() {
                if map_find_key(&acc, k).is_none() {
                    acc.push_kv(k.clone(), v.clone());
                }
            }
            cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY * acc.len() as u64)?;
            Ok(Value::wrap_keyed_pairs(&args[0], acc.to_vec()))
        }
        "mapReplaceAll" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m1 = expect_map(&args[0])?;
            let m2 = expect_map(&args[1])?;
            let mut b = m1.borrow_mut();
            for (k, v) in m2.borrow().iter() {
                if let Some(p) = map_find_key(&b, k) {
                    b[p].1 = v.clone();
                }
            }
            Ok(Value::Null)
        }
        "mapPut" => {
            require_leek_v4(cx)?;
            expect_arity(name, 3, args.len())?;
            let m = expect_map(&args[0])?;
            let k = args[1].clone();
            let v = args[2].clone();
            let mut b = m.borrow_mut();
            if let Some(p) = map_find_key(&b, &k) {
                b[p].1 = v;
            } else {
                cx.charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY)?;
                b.push_kv(k, v);
            }
            ram::note_keyed_container_ram_peak(cx, b.len())?;
            Ok(args[0].clone())
        }
        "mapRemove" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let k = &args[1];
            let mut b = m.borrow_mut();
            if let Some(p) = map_find_key(&b, k) {
                let v = b.remove_ordered(p).1;
                cx.release_ram_quads(MAP_RAM_QUADS_PER_ENTRY);
                Ok(v)
            } else {
                Ok(Value::Null)
            }
        }
        "mapReplace" => {
            require_leek_v4(cx)?;
            expect_arity(name, 3, args.len())?;
            let m = expect_map(&args[0])?;
            let k = args[1].clone();
            let v = args[2].clone();
            let mut b = m.borrow_mut();
            if let Some(p) = map_find_key(&b, &k) {
                let old = std::mem::replace(&mut b[p].1, v);
                Ok(old)
            } else {
                Ok(Value::Null)
            }
        }
        "mapRemoveAll" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let want = args[1].clone();
            let mut b = m.borrow_mut();
            let before = b.len();
            b.retain(|(_, v)| !values_equal_for_compare(v, &want));
            let removed = before.saturating_sub(b.len());
            cx.release_ram_quads(MAP_RAM_QUADS_PER_ENTRY * removed as u64);
            Ok(Value::Null)
        }
        "mapSize" => {
            require_leek_v4(cx)?;
            expect_arity(name, 1, args.len())?;
            Ok(Value::Integer(expect_map(&args[0])?.borrow().len() as i64))
        }
        "mapSome" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let cb = &args[1];
            let b = m.borrow().clone();
            let snap = args[0].clone();
            for (k, v) in b.iter() {
                let r = invoke_map_pred(cx, cb, v.clone(), k.clone(), &snap)?;
                if value_truthy_native(&r) {
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(false))
        }
        "mapSum" => {
            require_leek_v4(cx)?;
            expect_arity(name, 1, args.len())?;
            map_sum_native(&args[0])
        }
        "mapValues" => {
            require_leek_v4(cx)?;
            expect_arity(name, 1, args.len())?;
            let m = expect_map(&args[0])?;
            let b = m.borrow();
            cx.charge_ram_quads(b.len() as u64)?;
            let vals: Vec<Value> = b.as_slice().iter().map(|(_, v)| v.clone()).collect();
            Ok(Value::array_from(vals))
        }
        "mapFill" => {
            require_leek_v4(cx)?;
            expect_arity(name, 2, args.len())?;
            let m = expect_map(&args[0])?;
            let v = args[1].clone();
            for (_, vv) in m.borrow_mut().iter_mut() {
                *vv = v.clone();
            }
            Ok(Value::Null)
        }

        "setClear" => {
            expect_arity(name, 1, args.len())?;
            let s = expect_set(&args[0])?;
            let mut b = s.borrow_mut();
            b.elems.clear();
            b.java_hash_export = false;
            b.ever_mutated = true;
            Ok(Value::Null)
        }
        "setContains" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_set(&args[0])?;
            let v = &args[1];
            let hit = {
                let b = s.borrow();
                b.elems.iter().any(|x| values_equal_for_compare(x, v))
            };
            Ok(Value::Bool(hit))
        }
        "setDifference" => {
            expect_arity(name, 2, args.len())?;
            set_binop(&args[0], &args[1], |a, b| {
                a.iter()
                    .filter(|x| !b.iter().any(|y| values_equal_for_compare(x, y)))
                    .cloned()
                    .collect()
            })
        }
        "setDisjunction" => {
            expect_arity(name, 2, args.len())?;
            let sa = expect_set(&args[0])?;
            let sb = expect_set(&args[1])?;
            let a = sa.borrow();
            let b = sb.borrow();
            let mut out = Vec::new();
            for x in a.elems.iter() {
                if !b.elems.iter().any(|y| values_equal_for_compare(x, y)) {
                    out.push(x.clone());
                }
            }
            for x in b.elems.iter() {
                if !a.elems.iter().any(|y| values_equal_for_compare(x, y)) {
                    out.push(x.clone());
                }
            }
            Ok(Value::set_from(out))
        }
        "setIntersection" => {
            expect_arity(name, 2, args.len())?;
            set_binop(&args[0], &args[1], |a, b| {
                a.iter()
                    .filter(|x| b.iter().any(|y| values_equal_for_compare(x, y)))
                    .cloned()
                    .collect()
            })
        }
        "setIsEmpty" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Bool(expect_set(&args[0])?.borrow().elems.is_empty()))
        }
        "setIsSubsetOf" => {
            expect_arity(name, 2, args.len())?;
            let a = expect_set(&args[0])?;
            let b = expect_set(&args[1])?;
            let ok = a.borrow().elems.iter().all(|x| {
                b.borrow()
                    .elems
                    .iter()
                    .any(|y| values_equal_for_compare(x, y))
            });
            Ok(Value::Bool(ok))
        }
        "setInsert" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_set(&args[0])?;
            let v = args[1].clone();
            let mut b = s.borrow_mut();
            if b.elems.iter().any(|x| values_equal_for_compare(x, &v)) {
                Ok(Value::Bool(false))
            } else {
                b.elems.push(v);
                b.java_hash_export = false;
                Ok(Value::Bool(true))
            }
        }
        "setPut" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_set(&args[0])?;
            let v = args[1].clone();
            let mut b = s.borrow_mut();
            if b.elems.iter().any(|x| values_equal_for_compare(x, &v)) {
                Ok(Value::Bool(false))
            } else {
                b.elems.push(v);
                b.java_hash_export = false;
                b.ever_mutated = true;
                Ok(Value::Bool(true))
            }
        }
        "setRemove" => {
            expect_arity(name, 2, args.len())?;
            let s = expect_set(&args[0])?;
            let v = &args[1];
            let mut b = s.borrow_mut();
            if let Some(i) = b.elems.iter().position(|x| values_equal_for_compare(x, v)) {
                b.elems.remove(i);
                b.java_hash_export = false;
                b.ever_mutated = true;
                Ok(Value::Bool(true))
            } else {
                Ok(Value::Bool(false))
            }
        }
        "setSize" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Integer(
                expect_set(&args[0])?.borrow().elems.len() as i64
            ))
        }
        "setToArray" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::array_from(
                expect_set(&args[0])?.borrow().elems.clone(),
            ))
        }
        "setUnion" => {
            expect_arity(name, 2, args.len())?;
            set_binop(&args[0], &args[1], |a, b| {
                let mut out = a.to_vec();
                for x in b {
                    if !out.iter().any(|y| values_equal_for_compare(y, x)) {
                        out.push(x.clone());
                    }
                }
                out
            })
        }

        "intervalAverage" => {
            expect_arity(name, 1, args.len())?;
            let iv = expect_interval(&args[0])?;
            if iv.max < iv.min {
                return Ok(Value::Real(f64::NAN));
            }
            if !iv.min.is_finite() && !iv.max.is_finite() {
                return Ok(Value::Real(f64::NAN));
            }
            if !iv.min.is_finite() {
                return Ok(Value::Real(f64::NEG_INFINITY));
            }
            if !iv.max.is_finite() {
                return Ok(Value::Real(f64::INFINITY));
            }
            if interval_is_empty(&iv) {
                return Ok(Value::Real(f64::NAN));
            }
            if iv.integer_lattice {
                let pairs = super::util::interval_kv_pairs(&iv)?;
                if pairs.is_empty() {
                    return Ok(Value::Real(f64::NAN));
                }
                let mut sum = 0.0f64;
                for (_, v) in &pairs {
                    sum += number_from_value(v)?;
                }
                return Ok(Value::Real(sum / pairs.len() as f64));
            }
            Ok(Value::Real((iv.min + iv.max) / 2.0))
        }
        "intervalCombine" => dispatch_interval_combine(args),
        "intervalIntersection" => dispatch_interval_intersection(args),
        "intervalIsBounded" => {
            expect_arity(name, 1, args.len())?;
            let iv = expect_interval(&args[0])?;
            Ok(Value::Bool(iv.min.is_finite() && iv.max.is_finite()))
        }
        "intervalIsClosed" => {
            expect_arity(name, 1, args.len())?;
            let iv = expect_interval(&args[0])?;
            Ok(Value::Bool(iv.min_closed && iv.max_closed))
        }
        "intervalIsEmpty" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Bool(interval_is_empty(&expect_interval(&args[0])?)))
        }
        "intervalIsLeftBounded" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Bool(expect_interval(&args[0])?.min.is_finite()))
        }
        "intervalIsRightBounded" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Bool(expect_interval(&args[0])?.max.is_finite()))
        }
        "intervalIsLeftClosed" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Bool(expect_interval(&args[0])?.min_closed))
        }
        "intervalIsRightClosed" => {
            expect_arity(name, 1, args.len())?;
            Ok(Value::Bool(expect_interval(&args[0])?.max_closed))
        }
        "intervalMax" => {
            expect_arity(name, 1, args.len())?;
            let iv = expect_interval(&args[0])?;
            Ok(f64_to_numeric_value(iv.max))
        }
        "intervalMin" => {
            expect_arity(name, 1, args.len())?;
            let iv = expect_interval(&args[0])?;
            Ok(f64_to_numeric_value(iv.min))
        }
        "intervalSize" => {
            expect_arity(name, 1, args.len())?;
            let iv = expect_interval(&args[0])?;
            if interval_is_empty(&iv) {
                return Ok(Value::Real(0.0));
            }
            let lo = if iv.min_closed { iv.min } else { iv.min + 1.0 };
            let hi = if iv.max_closed { iv.max } else { iv.max - 1.0 };
            Ok(Value::Real((hi - lo + 1.0).max(0.0)))
        }
        "intervalContains" => {
            expect_arity(name, 2, args.len())?;
            eval_in(args[1].clone(), args[0].clone())
        }
        "intervalValues" => {
            let iv = expect_interval(&args[0])?;
            if args.len() != 1 && args.len() != 2 {
                return Err(InterpretError::invalid_parameter_count(1, args.len()));
            }
            let pairs = super::util::interval_kv_pairs(&iv)?;
            let out: Vec<Value> = pairs.into_iter().map(|(_, v)| v).collect();
            Ok(Value::array_from(out))
        }
        "intervalToArray" => {
            let iv = expect_interval(&args[0])?;
            let step = match args.len() {
                1 => None,
                2 => Some(number_from_value(&args[1])?),
                n => {
                    return Err(InterpretError::invalid_parameter_count(1, n));
                }
            };
            match interval_array_values(&iv, step)? {
                None => Ok(Value::Null),
                Some(vals) => {
                    // Java VM operation accounting (parity suite):
                    // v1–v3: interval materialization is much more expensive than v4.
                    // v4: charge 2 ops per produced element for `intervalToArray([a..b])`.
                    let per_elem = if cx.language_version >= 4 { 2 } else { 5 };
                    cx.charge_ops((vals.len() as u64).saturating_mul(per_elem))?;
                    let vals = if cx.language_version == 1 {
                        vals.into_iter()
                            .map(|v| match v {
                                Value::Real(r)
                                    if r.is_finite()
                                        && r.fract() == 0.0
                                        && r >= i64::MIN as f64
                                        && r <= i64::MAX as f64 =>
                                {
                                    Value::Integer(r as i64)
                                }
                                other => other,
                            })
                            .collect()
                    } else {
                        vals
                    };
                    Ok(Value::array_from(vals))
                }
            }
        }
        "intervalToSet" => {
            let iv = expect_interval(&args[0])?;
            let step = match args.len() {
                1 => None,
                2 => Some(number_from_value(&args[1])?),
                n => {
                    return Err(InterpretError::invalid_parameter_count(1, n));
                }
            };
            match interval_array_values(&iv, step)? {
                None => Ok(Value::Null),
                Some(vals) => {
                    let per_elem = if cx.language_version >= 4 { 2 } else { 5 };
                    cx.charge_ops((vals.len() as u64).saturating_mul(per_elem))?;
                    let vals = if cx.language_version >= 4 {
                        super::util::java_hashset_iteration_order(vals)
                    } else {
                        vals
                    };
                    dispatch(cx, "arrayToSet", &[Value::array_from(vals)], None)
                }
            }
        }

        "clone" => clone_dispatch(cx, args),
        "debug" => emit_leek_debug(cx, name, DebugLogKind::Info, args),
        "debugC" => emit_leek_debug(cx, name, DebugLogKind::Colored, args),
        "debugE" => emit_leek_debug(cx, name, DebugLogKind::Error, args),
        "debugW" => emit_leek_debug(cx, name, DebugLogKind::Warning, args),
        "jsonDecode" => {
            expect_arity(name, 1, args.len())?;
            let s = expect_str(&args[0])?;
            json_decode_leek(&s, cx.language_version).map_err(|e| InterpretError {
                reference: "WRONG_ARGUMENT_TYPE",
                message: format!("jsonDecode: {e}"),
            })
        }
        "jsonEncode" => {
            expect_arity(name, 1, args.len())?;
            let mut visited = HashSet::new();
            let j = json_encode_leek(&args[0], &mut visited, cx.language_version)?;
            serde_json::to_string(&j)
                .map(Value::String)
                .map_err(|e| InterpretError {
                    reference: "WRONG_ARGUMENT_TYPE",
                    message: format!("jsonEncode: {e}"),
                })
        }
        "typeOf" => {
            expect_arity(name, 1, args.len())?;
            Ok(runtime_typeof_value(&args[0]))
        }
        "number" => {
            expect_arity(name, 1, args.len())?;
            Ok(f64_to_numeric_value(number_from_value(&args[0])?))
        }
        // Packed color ints are **RRGGBB** (high → red), matching Java getters and `color(r,g,b)`.
        // `getColor(b, g, r)` below still packs **BBGGRR**; both agree on e.g. magenta/yellow/cyan test values.
        "getRed" => {
            expect_arity(name, 1, args.len())?;
            let c = int_operand(&args[0])?;
            Ok(Value::Integer(((c >> 16) & 255) as i64))
        }
        "getGreen" => {
            expect_arity(name, 1, args.len())?;
            let c = int_operand(&args[0])?;
            Ok(Value::Integer(((c >> 8) & 255) as i64))
        }
        "getBlue" => {
            expect_arity(name, 1, args.len())?;
            let c = int_operand(&args[0])?;
            Ok(Value::Integer((c & 255) as i64))
        }
        "color" => {
            if cx.language_version >= 4 {
                return Err(InterpretError::removed_function_replacement());
            }
            expect_arity(name, 3, args.len())?;
            let r = int_operand(&args[0])?;
            let g = int_operand(&args[1])?;
            let b = int_operand(&args[2])?;
            let pack = ((r & 255) << 16) | ((g & 255) << 8) | (b & 255);
            Ok(Value::Integer(pack))
        }
        "getColor" => {
            expect_arity(name, 3, args.len())?;
            let b = int_operand(&args[0])?;
            let g = int_operand(&args[1])?;
            let r = int_operand(&args[2])?;
            let pack = ((b & 255) << 16) | ((g & 255) << 8) | (r & 255);
            Ok(Value::Integer(pack))
        }

        _ => Err(InterpretError::not_callable()),
    }
}

fn expect_arity(_name: &str, expected: usize, got: usize) -> Result<(), InterpretError> {
    if got == expected {
        Ok(())
    } else {
        Err(InterpretError::invalid_parameter_count(expected, got))
    }
}

fn expect_str(v: &Value) -> Result<String, InterpretError> {
    match v {
        Value::String(s) => Ok(s.clone()),
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn int_operand(v: &Value) -> Result<i64, InterpretError> {
    match v {
        Value::Integer(i) => Ok(*i),
        Value::Real(r) if r.is_finite() && r.fract() == 0.0 => Ok(*r as i64),
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn int_from_val(v: &Value) -> Result<i64, InterpretError> {
    int_operand(v)
}

fn expect_array(v: &Value) -> Result<super::value::SharedArray, InterpretError> {
    match v {
        Value::Array(a) => Ok(a.clone()),
        Value::Instance(rc) => {
            let b = rc.borrow();
            if b.extends.as_deref() == Some("Array") {
                b.array_backing
                    .clone()
                    .ok_or_else(|| InterpretError::wrong_operand_types_binary())
            } else {
                Err(InterpretError::wrong_operand_types_binary())
            }
        }
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn expect_keyed(v: &Value) -> Result<super::value::SharedMap, InterpretError> {
    match v {
        Value::Map(m) | Value::Object(m) => Ok(m.clone()),
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

/// Java `Map` class natives: `MapLeekValue` only (`LeekFunctions` uses `Type.MAP`), not `ObjectLeekValue`.
fn expect_map(v: &Value) -> Result<super::value::SharedMap, InterpretError> {
    match v {
        Value::Map(m) => Ok(m.clone()),
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn expect_set(v: &Value) -> Result<super::value::SharedSet, InterpretError> {
    match v {
        Value::Set(s) => Ok(s.clone()),
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn expect_interval(v: &Value) -> Result<IntervalValue, InterpretError> {
    match v {
        Value::Interval(iv) => Ok(iv.clone()),
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn char_index(i: i64, len: usize) -> Result<usize, InterpretError> {
    if len == 0 {
        return Err(InterpretError::array_index_out_of_bounds());
    }
    let n = len as i64;
    let j = if i < 0 { i + n } else { i };
    if j < 0 || j >= n {
        return Err(InterpretError::array_index_out_of_bounds());
    }
    Ok(j as usize)
}

fn array_index_ok(i: i64, len: usize) -> Result<usize, InterpretError> {
    if len == 0 {
        return Err(InterpretError::array_index_out_of_bounds());
    }
    let n = len as i64;
    let j = if i < 0 { i + n } else { i };
    if j < 0 || j >= n {
        return Err(InterpretError::array_index_out_of_bounds());
    }
    Ok(j as usize)
}

fn digit_counts(mut n: i64) -> [u8; 10] {
    let mut c = [0u8; 10];
    if n == 0 {
        c[0] = 1;
        return c;
    }
    n = n.abs();
    while n > 0 {
        c[(n % 10) as usize] += 1;
        n /= 10;
    }
    c
}

fn value_truthy_native(v: &Value) -> bool {
    super::util::value_truthy(v)
}

/// Legacy `arrayPartition` (v1–3): bucket keys that are exactly `0..n-1` after sorting stringify as a
/// plain array; otherwise as a bracket map (`index : value`).
fn legacy_partition_bucket_export(mut entries: Vec<(i64, Value)>) -> Value {
    if entries.is_empty() {
        return Value::array_from(Vec::new());
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let dense_prefix = entries.iter().enumerate().all(|(i, (k, _))| *k == i as i64);
    if dense_prefix {
        Value::array_from(entries.into_iter().map(|(_, v)| v).collect())
    } else {
        Value::map_from(
            entries
                .into_iter()
                .map(|(k, v)| (Value::Integer(k), v))
                .collect(),
        )
    }
}

fn cmp_sort_values(a: &Value, b: &Value) -> Result<Ordering, InterpretError> {
    use Value::*;
    let ord = match (a, b) {
        (Null, Null) => Ordering::Equal,
        (Null, _) => Ordering::Less,
        (_, Null) => Ordering::Greater,
        (Bool(x), Bool(y)) => x.cmp(y),
        (Bool(_), _) => Ordering::Less,
        (_, Bool(_)) => Ordering::Greater,
        (Integer(x), Integer(y)) => x.cmp(y),
        (String(x), String(y)) => x.cmp(y),
        _ => {
            let af = number_from_value(a)?;
            let bf = number_from_value(b)?;
            if af.is_nan() || bf.is_nan() {
                return Err(InterpretError::wrong_operand_types_compare());
            }
            af.partial_cmp(&bf).unwrap_or(Ordering::Equal)
        }
    };
    Ok(ord)
}

fn invoke_user(
    cx: &mut InterpCx,
    f: Value,
    args: Vec<Value>,
    arg_array_cells: Option<&[Option<(SharedArray, usize)>]>,
) -> Result<Value, InterpretError> {
    if cx.language_version == 1 {
        cx.v1_array_cb_depth += 1;
    }
    let out = match invoke_value(cx, None, None, f, args, false, arg_array_cells, None) {
        Ok(v) => Ok(v),
        Err(ExecAbort::Error(e)) => Err(e),
        Err(ExecAbort::Throw(_)) => Err(InterpretError::uncaught_throw()),
    };
    if cx.language_version == 1 {
        cx.v1_array_cb_depth -= 1;
    }
    out
}

fn invoke_array_cb(
    cx: &mut InterpCx,
    cb: &Value,
    elem: Value,
    idx: i64,
    arr: &Value,
) -> Result<Value, InterpretError> {
    let (f, n) = callable(cx, cb)?;
    let idx_v = Value::Integer(idx);
    let mut args = match n {
        0 => vec![],
        1 => vec![elem],
        2 => {
            if cx.language_version >= 4 {
                vec![elem, idx_v]
            } else {
                // Leek v1–3: `(index, value)` / `function(k, v)` (see `TestArray.java` map/filter).
                vec![idx_v, elem]
            }
        }
        _ => vec![elem, idx_v, arr.clone()],
    };

    if cx.language_version == 1 {
        if let Value::Function(func) = &f {
            args = args
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    let br = i < func.param_by_ref.len() && func.param_by_ref[i];
                    pass_parameter_value(1, v, br)
                })
                .collect();
        }
    }

    let cells: Option<Vec<Option<(SharedArray, usize)>>> = match (&f, arr) {
        (Value::Function(func), Value::Array(shared)) => {
            let len = shared.borrow().len();
            if len == 0 {
                None
            } else {
                let idx_u = array_index_at(&Value::Integer(idx), len)
                    .map_err(|_| InterpretError::array_index_out_of_bounds())?;
                let mut v = Vec::with_capacity(args.len());
                for i in 0..args.len() {
                    if i < func.param_by_ref.len() && func.param_by_ref[i] {
                        v.push(Some((shared.clone(), idx_u)));
                    } else {
                        v.push(None);
                    }
                }
                Some(v)
            }
        }
        _ => None,
    };
    let sl = cells.as_deref();
    invoke_user(cx, f, args, sl)
}

fn invoke_array_pred(
    cx: &mut InterpCx,
    cb: &Value,
    elem: Value,
    idx: i64,
    arr: &Value,
) -> Result<Value, InterpretError> {
    invoke_array_cb(cx, cb, elem, idx, arr)
}

fn invoke_array_cb_void(
    cx: &mut InterpCx,
    cb: &Value,
    elem: Value,
    idx: i64,
    arr: &Value,
) -> Result<(), InterpretError> {
    invoke_array_cb(cx, cb, elem, idx, arr)?;
    Ok(())
}

fn invoke_map_cb(
    cx: &mut InterpCx,
    cb: &Value,
    val: Value,
    key: Value,
    m: &Value,
) -> Result<Value, InterpretError> {
    let (f, n) = callable(cx, cb)?;
    let args = match n {
        0 => vec![],
        1 => vec![val],
        2 => {
            if cx.language_version >= 4 {
                vec![val, key]
            } else {
                vec![key, val]
            }
        }
        3 => vec![val, key, m.clone()],
        4 => vec![val, key, m.clone(), m.clone()],
        _ => vec![val, key, m.clone(), m.clone()],
    };
    invoke_user(cx, f, args, None)
}

fn invoke_map_pred(
    cx: &mut InterpCx,
    cb: &Value,
    val: Value,
    key: Value,
    m: &Value,
) -> Result<Value, InterpretError> {
    invoke_map_cb(cx, cb, val, key, m)
}

fn invoke_map_cb_void(
    cx: &mut InterpCx,
    cb: &Value,
    val: Value,
    key: Value,
    m: &Value,
) -> Result<(), InterpretError> {
    invoke_map_cb(cx, cb, val, key, m)?;
    Ok(())
}

/// `mapEvery` passes only `(value, key)` to the callback (never the map).
fn invoke_map_every_pred(
    cx: &mut InterpCx,
    cb: &Value,
    val: Value,
    key: Value,
) -> Result<Value, InterpretError> {
    let (f, n) = callable(cx, cb)?;
    let args = if n <= 1 {
        vec![val]
    } else if cx.language_version >= 4 {
        vec![val, key]
    } else {
        vec![key, val]
    };
    invoke_user(cx, f, args, None)
}

fn invoke_fold_map(
    cx: &mut InterpCx,
    cb: &Value,
    acc: Value,
    val: Value,
    key: Value,
    m: &Value,
) -> Result<Value, InterpretError> {
    let (f, n) = callable(cx, cb)?;
    let args = match n {
        2 => vec![acc, val],
        3 => vec![acc, val, key],
        _ => vec![acc, val, key, m.clone()],
    };
    invoke_user(cx, f, args, None)
}

fn callable(cx: &InterpCx, v: &Value) -> Result<(Value, usize), InterpretError> {
    match v {
        Value::Function(f) => Ok((v.clone(), f.params.len())),
        Value::Native(name) => Ok((v.clone(), native_callback_arity(name))),
        Value::UserClass(type_name) => {
            let n = cx
                .classes
                .get(type_name.as_str())
                .and_then(|cd| {
                    cd.methods
                        .get("constructor")
                        .or_else(|| cd.methods.get(type_name.as_str()))
                })
                .and_then(|vs| vs.first())
                .and_then(|m| match m {
                    Value::Function(f) => Some(f.params.len()),
                    _ => None,
                })
                .unwrap_or(0);
            Ok((v.clone(), n))
        }
        _ => Err(InterpretError::not_callable()),
    }
}

/// Argument count when a core native is passed as an `arrayMap` / `arrayFilter` / … callback.
/// Java passes one element (and optionally index / container) depending on the user function arity;
/// unary natives like `sqrt` must receive only the element.
fn native_callback_arity(name: &str) -> usize {
    match name {
        "sqrt" | "abs" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" => 1,
        "atan2" => 2,
        "toUpper" | "toLower" => 1,
        "round" | "floor" | "ceil" | "signum" | "log" | "log2" | "log10" | "exp" => 1,
        "binString" | "hex" | "octal" | "number" | "string" | "length" | "typeOf" => 1,
        "bitCount" | "bitReverse" | "isNaN" | "isFinite" | "isInfinite" => 1,
        "debug" | "debugE" | "debugW" => 1,
        "debugC" => 2,
        "randFloat" | "randReal" => 2,
        _ => 3,
    }
}

fn flatten_array(items: &[Value], depth: i64) -> Result<Vec<Value>, InterpretError> {
    if depth <= 0 {
        return Ok(items.to_vec());
    }
    let mut out = Vec::new();
    for x in items {
        if let Value::Array(a) = x {
            let inner = flatten_array(&a.borrow(), depth - 1)?;
            out.extend(inner);
        } else {
            out.push(x.clone());
        }
    }
    Ok(out)
}

/// Min/max over array elements or map **values** (Java `arrayMin` / `arrayMax` on bracket maps).
fn array_extrema_by_value(v: &Value, want_max: bool) -> Result<Value, InterpretError> {
    let values: Vec<Value> = match v {
        Value::Array(a) => a.borrow().clone(),
        Value::Map(m) | Value::Object(m) => m.borrow().iter().map(|(_, vv)| vv.clone()).collect(),
        _ => return Err(InterpretError::wrong_operand_types_binary()),
    };
    if values.is_empty() {
        return Ok(Value::Null);
    }
    let mut cur = values[0].clone();
    for x in values.iter().skip(1) {
        let ord = cmp_sort_values(x, &cur)?;
        let better = if want_max {
            ord == Ordering::Greater
        } else {
            ord == Ordering::Less
        };
        if better {
            cur = x.clone();
        }
    }
    Ok(cur)
}

fn sum_dispatch(cx: &mut InterpCx, v: &Value) -> Result<Value, InterpretError> {
    match v {
        Value::Array(_) => sum_array(cx, v),
        Value::Map(m) | Value::Object(m) => {
            let b = m.borrow();
            let mut all_int = true;
            let mut si: i64 = 0;
            let mut sr: f64 = 0.0;
            for (_, x) in b.iter() {
                match x {
                    Value::Integer(n) => {
                        si = si.wrapping_add(*n);
                        sr += *n as f64;
                    }
                    Value::Real(r) => {
                        all_int = false;
                        sr += *r;
                    }
                    _ => return Err(InterpretError::wrong_operand_types_binary()),
                }
            }
            if cx.language_version >= 2 {
                Ok(Value::Real(sr))
            } else if all_int {
                Ok(Value::Integer(si))
            } else {
                Ok(Value::Real(sr))
            }
        }
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn search_dispatch(cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    match args.len() {
        2 => {
            if let Value::String(s) = &args[0] {
                let needle = expect_str(&args[1])?;
                return Ok(Value::Integer(
                    s.find(needle.as_str()).map(|i| i as i64).unwrap_or(-1),
                ));
            }
            if let Value::Map(m) | Value::Object(m) = &args[0] {
                let want = &args[1];
                let b = m.borrow();
                for (k, v) in b.iter() {
                    if values_equal_for_compare(v, want) {
                        return Ok(k.clone());
                    }
                }
                return Ok(Value::Null);
            }
            index_of_dispatch(cx, args)
        }
        3 => {
            if matches!(&args[0], Value::Map(_) | Value::Object(_)) {
                return Err(InterpretError::invalid_parameter_count(2, 3));
            }
            if let Value::String(_) = &args[0] {
                return index_of_dispatch(cx, args);
            }
            let r = index_of_dispatch(cx, args)?;
            if cx.language_version < 4 && matches!(r, Value::Integer(-1)) {
                return Ok(Value::Null);
            }
            Ok(r)
        }
        _ => Err(InterpretError::invalid_parameter_count(2, args.len())),
    }
}

/// Java `String.indexOf`: indices are UTF-16 code units (not Rust byte or `char` indices).
fn utf16_index_of(haystack: &str, needle: &str, from_utf16: usize) -> i64 {
    let hay: Vec<u16> = haystack.encode_utf16().collect();
    let ndl: Vec<u16> = needle.encode_utf16().collect();
    if ndl.is_empty() {
        return if from_utf16 >= hay.len() {
            hay.len() as i64
        } else {
            from_utf16 as i64
        };
    }
    if from_utf16 > hay.len() {
        return -1;
    }
    let tail = &hay[from_utf16..];
    if let Some(rel) = tail.windows(ndl.len()).position(|w| w == ndl.as_slice()) {
        (from_utf16 + rel) as i64
    } else {
        -1
    }
}

/// Java `String.codePointAt(int index)` — `index` is a UTF-16 code unit index.
fn code_point_at_java(s: &str, i: i64) -> Result<i64, InterpretError> {
    if i < 0 {
        return Err(InterpretError::array_index_out_of_bounds());
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let idx = i as usize;
    if idx >= units.len() {
        return Err(InterpretError::array_index_out_of_bounds());
    }
    let u = units[idx];
    if (0xDC00..=0xDFFF).contains(&u) {
        return Err(InterpretError::array_index_out_of_bounds());
    }
    if (0xD800..=0xDBFF).contains(&u) {
        if idx + 1 >= units.len() {
            return Err(InterpretError::array_index_out_of_bounds());
        }
        let low = units[idx + 1];
        if !(0xDC00..=0xDFFF).contains(&low) {
            return Err(InterpretError::array_index_out_of_bounds());
        }
        let cp = 0x10000u32 + (((u as u32 - 0xD800) << 10) | (low as u32 - 0xDC00));
        return Ok(cp as i64);
    }
    Ok(u as i64)
}

fn index_of_dispatch(_cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    match args.len() {
        2 => {
            if let Value::String(s) = &args[0] {
                let search = expect_str(&args[1])?;
                return Ok(Value::Integer(utf16_index_of(s, search.as_str(), 0)));
            }
            let a = expect_array(&args[0])?;
            let el = &args[1];
            let b = a.borrow();
            for i in 0..b.len() {
                if values_equal_for_compare(&b[i], el) {
                    return Ok(Value::Integer(i as i64));
                }
            }
            Ok(Value::Integer(-1))
        }
        3 => {
            if let Value::String(s) = &args[0] {
                let needle = expect_str(&args[1])?;
                let start = int_operand(&args[2])?;
                let from = start.max(0) as usize;
                return Ok(Value::Integer(utf16_index_of(s, needle.as_str(), from)));
            }
            let a = expect_array(&args[0])?;
            let el = &args[1];
            let start_raw = int_operand(&args[2])?;
            let b = a.borrow();
            let n = b.len() as i64;
            let mut st = start_raw;
            if st < 0 {
                st += n;
            }
            let start = st.max(0) as usize;
            for i in start..b.len() {
                if values_equal_for_compare(&b[i], el) {
                    return Ok(Value::Integer(i as i64));
                }
            }
            Ok(Value::Integer(-1))
        }
        _ => Err(InterpretError::invalid_parameter_count(2, args.len())),
    }
}

fn split_string(args: &[Value]) -> Result<Value, InterpretError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(InterpretError::invalid_parameter_count(2, args.len()));
    }
    let s = expect_str(&args[0])?;
    let sep = expect_str(&args[1])?;
    let limit = if args.len() == 3 {
        int_operand(&args[2])?.max(0) as usize
    } else {
        usize::MAX
    };
    let mut out: Vec<Value> = Vec::new();
    if sep.is_empty() {
        for ch in s.chars().take(limit) {
            out.push(Value::String(ch.to_string()));
        }
        return Ok(Value::array_from(out));
    }
    // Java `String.split`: `limit` is the max number of segments; at most `limit - 1` separators apply.
    let mut rest = s.as_str();
    while out.len() + 1 < limit && !rest.is_empty() {
        if let Some(p) = rest.find(sep.as_str()) {
            out.push(Value::String(rest[..p].to_string()));
            rest = &rest[p + sep.len()..];
        } else {
            break;
        }
    }
    if out.len() < limit {
        out.push(Value::String(rest.to_string()));
    }
    Ok(Value::array_from(out))
}

fn array_fold(cx: &mut InterpCx, args: &[Value], right: bool) -> Result<Value, InterpretError> {
    expect_arity("arrayFold", 3, args.len())?;
    let a = expect_array(&args[0])?;
    let cb = &args[1];
    let mut acc = args[2].clone();
    let b = a.borrow().clone();
    let indices: Vec<usize> = if right {
        (0..b.len()).rev().collect()
    } else {
        (0..b.len()).collect()
    };
    let arr_val = Value::Array(a.clone());
    for i in indices {
        let elem = b[i].clone();
        let (f, n) = callable(cx, cb)?;
        let args_call = if right {
            match n {
                0 => vec![],
                2 => vec![elem, acc],
                3 => vec![elem, acc, Value::Integer(i as i64)],
                _ => vec![elem, acc, Value::Integer(i as i64), arr_val.clone()],
            }
        } else {
            match n {
                0 => vec![],
                2 => vec![acc, elem],
                3 => vec![acc, elem, Value::Integer(i as i64)],
                _ => vec![acc, elem, Value::Integer(i as i64), arr_val.clone()],
            }
        };
        acc = invoke_user(cx, f, args_call, None)?;
    }
    Ok(acc)
}

fn legacy_array_slice_inclusive(
    cx: &mut InterpCx,
    a: &SharedArray,
    start: i64,
    end: i64,
) -> Result<Value, InterpretError> {
    let b = a.borrow();
    let n = b.len() as i64;
    if start < 0 || end < start || end >= n {
        return Ok(Value::Null);
    }
    let slice = b[start as usize..=end as usize].to_vec();
    let nq = slice.len() as u64;
    drop(b);
    cx.charge_ram_quads(nq)?;
    Ok(Value::array_from(slice))
}

fn array_slice_dispatch(cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    if cx.language_version >= 4 {
        return array_slice_native(cx, args);
    }
    if args.len() < 2 || args.len() > 4 {
        return Err(InterpretError::invalid_parameter_count(2, args.len()));
    }
    let a = expect_array(&args[0])?;
    let start = int_operand(&args[1])?;
    let end = if args.len() >= 3 {
        int_operand(&args[2])?
    } else {
        a.borrow().len() as i64 - 1
    };
    legacy_array_slice_inclusive(cx, &a, start, end)
}

fn sub_array_dispatch(cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    expect_arity("subArray", 3, args.len())?;
    let a = expect_array(&args[0])?;
    let start = int_operand(&args[1])?;
    let end = int_operand(&args[2])?;
    if cx.language_version >= 4 {
        let narrow = [
            Value::Array(a.clone()),
            Value::Integer(start),
            Value::Integer(end),
        ];
        return array_slice_native(cx, &narrow);
    }
    legacy_array_slice_inclusive(cx, &a, start, end)
}

fn array_slice_native(cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    if args.len() < 2 || args.len() > 4 {
        return Err(InterpretError::invalid_parameter_count(2, args.len()));
    }
    let a = expect_array(&args[0])?;
    let b = a.borrow();
    let len = b.len();
    let len_i = len as i64;
    drop(b);
    if len == 0 {
        return Ok(Value::array_from(vec![]));
    }

    let step = if args.len() == 4 {
        let s = int_operand(&args[3])?;
        if s == 0 {
            1
        } else {
            s
        }
    } else {
        1
    };

    let default_start = if step > 0 { 0_i64 } else { (len_i - 1).max(0) };
    let bound_or = |i: usize, default: i64| -> Result<i64, InterpretError> {
        if i >= args.len() {
            return Ok(default);
        }
        match &args[i] {
            Value::Null => Ok(default),
            v => int_operand(v),
        }
    };

    let (start, end) = if args.len() == 2 {
        if matches!(&args[1], Value::Null) {
            (0_i64, len_i)
        } else {
            (int_operand(&args[1])?, len_i)
        }
    } else {
        (bound_or(1, default_start)?, bound_or(2, len_i)?)
    };
    let st = if start < 0 {
        (start + len_i).clamp(0, len_i) as usize
    } else {
        (start as usize).min(len)
    };
    let en = if end < 0 {
        (end + len_i).clamp(0, len_i) as usize
    } else {
        (end as usize).min(len)
    };
    if step > 0 {
        if st >= en {
            return Ok(Value::array_from(vec![]));
        }
        let b = a.borrow();
        let mut out = Vec::new();
        let mut i = st as i64;
        while (i as usize) < en {
            out.push(b[i as usize].clone());
            i += step as i64;
        }
        cx.charge_ram_quads(out.len() as u64)?;
        Ok(Value::array_from(out))
    } else {
        let end_was_null = args.len() >= 3 && matches!(&args[2], Value::Null);
        let exclusive_low: i64 = if end_was_null {
            -1
        } else {
            let el = if end < 0 { end + len_i } else { end };
            if el < 0 {
                -1
            } else {
                el
            }
        };
        let mut i = start as i64;
        if i >= len_i {
            i = len_i - 1;
        }
        if i < 0 {
            i += len_i;
        }
        i = i.clamp(0, len_i - 1);
        let b = a.borrow();
        let mut out = Vec::new();
        let step_abs = (-step) as i64;
        while i > exclusive_low && i < len_i {
            out.push(b[i as usize].clone());
            i -= step_abs;
        }
        cx.charge_ram_quads(out.len() as u64)?;
        Ok(Value::array_from(out))
    }
}

/// After [`keySort`], if keys are exactly `0..n-1` in order, Java exports a dense array of values.
fn try_keysort_dense_array_values(sorted_pairs: &[(Value, Value)]) -> Option<Vec<Value>> {
    if sorted_pairs.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::with_capacity(sorted_pairs.len());
    for (idx, (k, v)) in sorted_pairs.iter().enumerate() {
        match k {
            Value::Integer(i) if *i == idx as i64 => out.push(v.clone()),
            _ => return None,
        }
    }
    Some(out)
}

fn array_sort_native(cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    if args.is_empty() || args.len() > 2 {
        return Err(InterpretError::invalid_parameter_count(1, args.len()));
    }
    match &args[0] {
        // Same as `arrayMap`: keyed bracket literals are `Array` in Java's static types, `Map` here.
        Value::Map(m) => {
            let mut pairs = m.borrow().to_vec();
            if args.len() == 1 {
                pairs
                    .sort_by(|(k1, _), (k2, _)| cmp_sort_values(k1, k2).unwrap_or(Ordering::Equal));
            } else {
                let cb = &args[1];
                let (f, n) = callable(cx, cb)?;
                if n >= 4 {
                    pairs.sort_by(|(k1, v1), (k2, v2)| {
                        let args_cmp = vec![k1.clone(), v1.clone(), k2.clone(), v2.clone()];
                        let r = match invoke_user(cx, f.clone(), args_cmp, None) {
                            Ok(v) => int_operand(&v).unwrap_or(0),
                            Err(_) => 0,
                        };
                        r.cmp(&0)
                    });
                } else {
                    let snap = args[0].clone();
                    pairs.sort_by(|(_, v1), (_, v2)| {
                        let r =
                            invoke_array_cmp(cx, cb, v1.clone(), v2.clone(), &snap).unwrap_or(0);
                        r.cmp(&0)
                    });
                }
            }
            Ok(Value::wrap_keyed_pairs(&args[0], pairs))
        }
        Value::Array(a) => {
            if args.len() == 1 {
                let mut items = a.borrow().clone();
                items.sort_by(|x, y| cmp_sort_values(x, y).unwrap_or(Ordering::Equal));
                return Ok(Value::array_from(items));
            }
            let cb = &args[1];
            let (f, n) = callable(cx, cb)?;
            let mut items = a.borrow().clone();
            if n >= 4 {
                let len = items.len();
                let mut idx: Vec<usize> = (0..len).collect();
                idx.sort_by(|&i, &j| {
                    let args_cmp = vec![
                        Value::Integer(i as i64),
                        items[i].clone(),
                        Value::Integer(j as i64),
                        items[j].clone(),
                    ];
                    let r = match invoke_user(cx, f.clone(), args_cmp, None) {
                        Ok(v) => int_operand(&v).unwrap_or(0),
                        Err(_) => 0,
                    };
                    r.cmp(&0).then_with(|| i.cmp(&j))
                });
                items = idx.into_iter().map(|i| items[i].clone()).collect();
            } else {
                let snap = Value::Array(a.clone());
                items.sort_by(|x, y| {
                    let r = invoke_array_cmp(cx, cb, x.clone(), y.clone(), &snap).unwrap_or(0);
                    r.cmp(&0)
                });
            }
            Ok(Value::array_from(items))
        }
        _ => Err(InterpretError::wrong_operand_types_binary()),
    }
}

fn invoke_array_cmp(
    cx: &mut InterpCx,
    cb: &Value,
    x: Value,
    y: Value,
    arr: &Value,
) -> Result<i64, InterpretError> {
    let (f, n) = callable(cx, cb)?;
    let args = match n {
        0 => vec![],
        2 => vec![x, y],
        3 => vec![x, y, Value::Integer(0)],
        _ => vec![x, y, Value::Integer(0), arr.clone()],
    };
    let v = invoke_user(cx, f, args, None)?;
    int_operand(&v)
}

fn remove_dispatch(cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    expect_arity("remove", 2, args.len())?;
    let a = expect_array(&args[0])?;
    let key = &args[1];
    let mut b = a.borrow_mut();
    let len = b.len();
    if let Value::Integer(i) = key {
        return match array_index_ok(*i, len) {
            Ok(idx) => {
                cx.release_ram_quads(1);
                Ok(b.remove(idx))
            }
            Err(_) => Ok(Value::Null),
        };
    }
    if let Some(p) = b.iter().position(|x| values_equal_for_compare(x, key)) {
        cx.release_ram_quads(1);
        b.remove(p);
    }
    drop(b);
    Ok(Value::Array(a.clone()))
}

fn sum_array(cx: &InterpCx, v: &Value) -> Result<Value, InterpretError> {
    let a = expect_array(v)?;
    let b = a.borrow();
    let mut all_int = true;
    let mut si: i64 = 0;
    let mut sr: f64 = 0.0;
    for x in b.iter() {
        match x {
            Value::Integer(n) => {
                si = si.wrapping_add(*n);
                sr += *n as f64;
            }
            Value::Real(r) => {
                all_int = false;
                sr += *r;
            }
            _ => return Err(InterpretError::wrong_operand_types_binary()),
        }
    }
    if all_int && cx.language_version < 2 {
        Ok(Value::Integer(si))
    } else {
        Ok(Value::Real(sr))
    }
}

fn map_sum_native(v: &Value) -> Result<Value, InterpretError> {
    let m = expect_map(v)?;
    let b = m.borrow();
    let mut sr: f64 = 0.0;
    for (_, x) in b.iter() {
        sr += number_from_value(x)?;
    }
    Ok(Value::Real(sr))
}

fn sort_builtin(cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    if args.is_empty() || args.len() > 2 {
        return Err(InterpretError::invalid_parameter_count(1, args.len()));
    }
    let order = if args.len() == 2 {
        int_operand(&args[1])?
    } else {
        0
    };
    let a = expect_array(&args[0])?;
    let mut inner = a.borrow().clone();
    if order == 2 {
        fastrand::shuffle(&mut inner);
    } else {
        let v1 = cx.language_version == 1;
        inner.sort_by(|x, y| {
            if v1 {
                match order {
                    0 => cmp_sort_values_v1_nulls_last(x, y),
                    1 => cmp_sort_values_v1_nulls_first_desc(x, y),
                    _ => cmp_sort_values(x, y).unwrap_or(Ordering::Equal),
                }
            } else {
                let c = cmp_sort_values(x, y).unwrap_or(Ordering::Equal);
                if order == 0 {
                    c
                } else {
                    c.reverse()
                }
            }
        });
    }
    *a.borrow_mut() = inner;
    Ok(Value::Null)
}

fn cmp_sort_values_v1_nulls_last(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Greater,
        (_, Value::Null) => Ordering::Less,
        _ => cmp_sort_values(a, b).unwrap_or(Ordering::Equal),
    }
}

fn cmp_sort_values_v1_nulls_first_desc(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        _ => cmp_sort_values(a, b).unwrap_or(Ordering::Equal).reverse(),
    }
}

fn set_binop(
    a: &Value,
    b: &Value,
    f: fn(&[Value], &[Value]) -> Vec<Value>,
) -> Result<Value, InterpretError> {
    let sa = expect_set(a)?;
    let sb = expect_set(b)?;
    let out = f(&sa.borrow().elems, &sb.borrow().elems);
    Ok(Value::set_from(out))
}

fn clone_dispatch(cx: &mut InterpCx, args: &[Value]) -> Result<Value, InterpretError> {
    let depth = if args.len() == 1 {
        64
    } else if args.len() == 2 {
        int_operand(&args[1])?.max(0) as u32
    } else {
        return Err(InterpretError::invalid_parameter_count(1, args.len()));
    };
    clone_value_deep(&args[0], depth, Some(cx))
}

fn clone_value_deep(
    v: &Value,
    depth: u32,
    mut cx: Option<&mut InterpCx>,
) -> Result<Value, InterpretError> {
    if depth == 0 {
        return Ok(v.clone());
    }
    let d = depth - 1;
    Ok(match v {
        Value::Array(a) => {
            let mut inner = Vec::new();
            match &mut cx {
                Some(c) => {
                    for x in a.borrow().iter() {
                        inner.push(clone_value_deep(x, d, Some(&mut **c))?);
                    }
                    (*c).charge_ram_quads(inner.len() as u64)?;
                }
                None => {
                    for x in a.borrow().iter() {
                        inner.push(clone_value_deep(x, d, None)?);
                    }
                }
            }
            Value::array_from(inner)
        }
        Value::Map(m) | Value::Object(m) => {
            let mut pairs = Vec::new();
            match &mut cx {
                Some(c) => {
                    for (k, vv) in m.borrow().iter() {
                        pairs.push((
                            clone_value_deep(k, d, Some(&mut **c))?,
                            clone_value_deep(vv, d, Some(&mut **c))?,
                        ));
                    }
                    (*c).charge_ram_quads(MAP_RAM_QUADS_PER_ENTRY * pairs.len() as u64)?;
                }
                None => {
                    for (k, vv) in m.borrow().iter() {
                        pairs.push((
                            clone_value_deep(k, d, None)?,
                            clone_value_deep(vv, d, None)?,
                        ));
                    }
                }
            }
            Value::wrap_keyed_pairs(v, pairs)
        }
        Value::Set(s) => {
            let mut inner = Vec::new();
            match &mut cx {
                Some(c) => {
                    for x in s.borrow().elems.iter() {
                        inner.push(clone_value_deep(x, d, Some(&mut **c))?);
                    }
                    (*c).charge_ram_quads(inner.len() as u64)?;
                }
                None => {
                    for x in s.borrow().elems.iter() {
                        inner.push(clone_value_deep(x, d, None)?);
                    }
                }
            }
            Value::set_from(inner)
        }
        Value::Instance(inst) => {
            let b = inst.borrow();
            let mut fields: indexmap::IndexMap<String, Value> = indexmap::IndexMap::new();
            let array_backing: Option<super::value::SharedArray> = match &mut cx {
                Some(c) => {
                    for (k, vv) in b.fields.iter() {
                        fields.insert(k.clone(), clone_value_deep(vv, d, Some(&mut **c))?);
                    }
                    let ab = if let Some(arr) = &b.array_backing {
                        let inner: Result<Vec<Value>, InterpretError> = arr
                            .borrow()
                            .iter()
                            .map(|x| clone_value_deep(x, d, Some(&mut **c)))
                            .collect();
                        Some(std::rc::Rc::new(std::cell::RefCell::new(inner?)))
                    } else {
                        None
                    };
                    let mut n = MAP_RAM_QUADS_PER_ENTRY * fields.len() as u64;
                    if let Some(arr) = &ab {
                        n = n.saturating_add(arr.borrow().len() as u64);
                    }
                    (*c).charge_ram_quads(n)?;
                    ab
                }
                None => {
                    for (k, vv) in b.fields.iter() {
                        fields.insert(k.clone(), clone_value_deep(vv, d, None)?);
                    }
                    if let Some(arr) = &b.array_backing {
                        let inner: Result<Vec<Value>, InterpretError> = arr
                            .borrow()
                            .iter()
                            .map(|x| clone_value_deep(x, d, None))
                            .collect();
                        Some(std::rc::Rc::new(std::cell::RefCell::new(inner?)))
                    } else {
                        None
                    }
                }
            };
            let data = super::value::InstanceData {
                class_name: b.class_name.clone(),
                extends: b.extends.clone(),
                array_backing,
                string_override: b.string_override.clone(),
                fields,
            };
            Value::Instance(std::rc::Rc::new(std::cell::RefCell::new(data)))
        }
        _ => v.clone(),
    })
}

fn json_value_cycle_ptr(v: &Value) -> Option<usize> {
    match v {
        Value::Array(a) => Some(Rc::as_ptr(a) as usize),
        Value::Map(m) | Value::Object(m) => Some(Rc::as_ptr(m) as usize),
        Value::Set(s) => Some(Rc::as_ptr(s) as usize),
        Value::Instance(i) => Some(Rc::as_ptr(i) as usize),
        _ => None,
    }
}

fn json_encode_leek(
    v: &Value,
    visited: &mut HashSet<usize>,
    ver: u8,
) -> Result<JsonValue, InterpretError> {
    Ok(match v {
        Value::Null => JsonValue::Null,
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::Integer(n) => JsonValue::Number(serde_json::Number::from(*n)),
        Value::Real(r) | Value::RealDotZero(r) => serde_json::Number::from_f64(*r)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Array(a) => {
            let ap = Rc::as_ptr(a) as usize;
            visited.insert(ap);
            let mut arr = Vec::new();
            for x in a.borrow().iter() {
                if let Some(p) = json_value_cycle_ptr(x) {
                    if visited.contains(&p) {
                        continue;
                    }
                }
                arr.push(json_encode_leek(x, visited, ver)?);
            }
            JsonValue::Array(arr)
        }
        Value::Map(m) | Value::Object(m) => {
            let mp = Rc::as_ptr(m) as usize;
            visited.insert(mp);
            let mut pairs: Vec<(Value, Value)> = m.borrow().iter().cloned().collect();
            pairs.sort_by(|(k1, _), (k2, _)| string_builtin(k1, ver).cmp(&string_builtin(k2, ver)));
            let mut obj = serde_json::Map::new();
            for (k, vv) in pairs {
                if let Some(p) = json_value_cycle_ptr(&vv) {
                    if visited.contains(&p) {
                        continue;
                    }
                }
                let key = match &k {
                    Value::String(s) => s.clone(),
                    _ => string_builtin(&k, ver),
                };
                obj.insert(key, json_encode_leek(&vv, visited, ver)?);
            }
            JsonValue::Object(obj)
        }
        Value::Set(s) => {
            let sp = Rc::as_ptr(s) as usize;
            visited.insert(sp);
            let mut arr = Vec::new();
            for x in s.borrow().elems.iter() {
                if let Some(p) = json_value_cycle_ptr(x) {
                    if visited.contains(&p) {
                        continue;
                    }
                }
                arr.push(json_encode_leek(x, visited, ver)?);
            }
            JsonValue::Array(arr)
        }
        Value::Interval(_)
        | Value::Function { .. }
        | Value::Native(_)
        | Value::Instance(_)
        | Value::UserClass(_)
        | Value::Super => {
            return Err(InterpretError {
                reference: "WRONG_ARGUMENT_TYPE",
                message: "jsonEncode: unsupported value".into(),
            });
        }
    })
}

fn json_decode_leek(s: &str, ver: u8) -> Result<Value, InterpretError> {
    if s.is_empty() {
        // Java `JSONClass.jsonDecode`: parse failure → null.
        return Ok(Value::Null);
    }
    let j: JsonValue = serde_json::from_str(s).map_err(|e| InterpretError {
        reference: "WRONG_ARGUMENT_TYPE",
        message: e.to_string(),
    })?;
    json_to_value(&j, ver)
}

fn json_to_value(j: &JsonValue, ver: u8) -> Result<Value, InterpretError> {
    Ok(match j {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(u) = n.as_u64() {
                Value::Integer(u as i64)
            } else {
                Value::Real(n.as_f64().unwrap_or(0.0))
            }
        }
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(a) => {
            let mut v = Vec::new();
            for x in a {
                v.push(json_to_value(x, ver)?);
            }
            Value::array_from(v)
        }
        JsonValue::Object(o) => {
            // v1–v3: Java `LeekValueManager.parseJSON` uses `LegacyArrayLeekValue`; `{}` is an empty array.
            if ver <= 3 && o.is_empty() {
                Value::array_from(vec![])
            } else {
                let mut keys: Vec<&String> = o.keys().collect();
                keys.sort();
                let mut pairs = Vec::new();
                for k in keys {
                    pairs.push((
                        Value::String(k.clone()),
                        json_to_value(o.get(k).expect("key from map"), ver)?,
                    ));
                }
                if ver >= 4 {
                    Value::object_from(pairs)
                } else {
                    Value::map_from(pairs)
                }
            }
        }
    })
}
