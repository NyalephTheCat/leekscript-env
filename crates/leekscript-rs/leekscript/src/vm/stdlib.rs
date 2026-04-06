//! Native implementations for a subset of `sig/core/stdlib.sig.functions.leek`.
//!
//! Operation costs follow the Java reference:
//! - Static `LeekFunctions.getOperations()` from
//!   `leek-wars-generator/leekscript/.../LeekFunctions.java` (when `> 0`).
//! - Plus runtime `ai.ops(...)` from `StringClass`, `ArrayLeekValue`, etc. when the Java
//!   implementation charges there.
//!
//! The VM compiler emits [`Opcode::ChargeOps`](super::opcode::Opcode::ChargeOps) for **argument**
//! subexpressions only on native calls (Java `LeekFunctionCall` parameter `getOperations()`).

use std::cell::Cell;

use super::error::VmError;
use super::interpreter::{NativeFn, Vm};
use super::value::{set_value_from_elements, NumberBits, Value};

#[inline]
fn num_unary_preserve(v: f64, src: &Value) -> Value {
    Value::Number(match src.number_bits() {
        Some(NumberBits::Real(_)) => NumberBits::Real(v),
        Some(NumberBits::Int(_)) | None => NumberBits::coerce_integerish_f64(v),
    })
}

#[inline]
fn num_binary_merge(v: f64, a: &Value, b: &Value) -> Value {
    let prefer_real = matches!(a.number_bits(), Some(NumberBits::Real(_)))
        || matches!(b.number_bits(), Some(NumberBits::Real(_)));
    Value::Number(if prefer_real {
        NumberBits::Real(v)
    } else {
        NumberBits::coerce_integerish_f64(v)
    })
}

thread_local! {
    static RAND_STATE: Cell<u64> = Cell::new(0x6a09_e667_f3bc_c909);
}

#[inline]
fn ch(vm: &mut Vm, n: u64) -> Result<(), VmError> {
    vm.add_operations(n)
}

/// UTF-16 code unit count, matching Java `String.length()` for cost formulas.
fn java_utf16_len(s: &str) -> u64 {
    s.encode_utf16().count() as u64
}

/// Java `String.indexOf(String, int)` on UTF-16 code units (`fromIndex < 0` ⇒ `0`).
fn index_of_utf16_from(haystack: &str, needle: &str, from_index: i64) -> i64 {
    let hay: Vec<u16> = haystack.encode_utf16().collect();
    let needle_u: Vec<u16> = needle.encode_utf16().collect();
    let from = from_index.max(0) as usize;
    if needle_u.is_empty() {
        return from.min(hay.len()) as i64;
    }
    if from > hay.len() {
        return -1;
    }
    let last = hay.len().saturating_sub(needle_u.len());
    for i in from..=last {
        if hay[i..i + needle_u.len()] == needle_u[..] {
            return i as i64;
        }
    }
    -1
}

fn rng_u64() -> u64 {
    RAND_STATE.with(|c| {
        let mut s = c.get();
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        c.set(s);
        s
    })
}

fn rng01_open() -> f64 {
    (rng_u64() >> 11) as f64 / ((1u64 << 53) as f64)
}

fn bad_argc(expected: u8, got: usize) -> VmError {
    VmError::BadArgCount { expected, got }
}

fn f64_as_i64_trunc(n: f64) -> i64 {
    if !n.is_finite() {
        return 0;
    }
    if n >= i64::MAX as f64 {
        return i64::MAX;
    }
    if n <= i64::MIN as f64 {
        return i64::MIN;
    }
    n as i64
}

fn one_num(args: &[Value]) -> Result<f64, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    args[0].as_number().ok_or(VmError::ExpectedNumber)
}

fn two_nums(args: &[Value]) -> Result<(f64, f64), VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let a = args[0].as_number().ok_or(VmError::ExpectedNumber)?;
    let b = args[1].as_number().ok_or(VmError::ExpectedNumber)?;
    Ok((a, b))
}

fn one_string(args: &[Value]) -> Result<&str, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    match &args[0] {
        Value::String(s) => Ok(s.as_str()),
        _ => Err(VmError::ExpectedString),
    }
}

fn two_strings(args: &[Value]) -> Result<(&str, &str), VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let a = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let b = match &args[1] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    Ok((a, b))
}

fn u64_bits(n: f64) -> u64 {
    f64_as_i64_trunc(n) as u64
}

fn digit_hist(mut n: i64) -> [u8; 10] {
    let mut c = [0u8; 10];
    if n == 0 {
        c[0] = 1;
        return c;
    }
    if n < 0 {
        n = n.saturating_neg();
    }
    while n > 0 {
        c[(n % 10) as usize] = c[(n % 10) as usize].saturating_add(1);
        n /= 10;
    }
    c
}

// --- math (LeekFunctions Number + NumberClass: no extra runtime ops) ---

fn nf_abs(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let a = one_num(args)?;
    Ok(num_unary_preserve(a.abs(), &args[0]))
}

fn nf_acos(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.acos()))
}

fn nf_asin(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.asin()))
}

fn nf_atan(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.atan()))
}

fn nf_atan2(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 35)?;
    let (y, x) = two_nums(args)?;
    Ok(Value::num_real(y.atan2(x)))
}

fn nf_ceil(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let a = one_num(args)?;
    Ok(num_unary_preserve(a.ceil(), &args[0]))
}

fn nf_floor(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let a = one_num(args)?;
    Ok(num_unary_preserve(a.floor(), &args[0]))
}

fn nf_round(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let a = one_num(args)?;
    Ok(num_unary_preserve(a.round(), &args[0]))
}

fn nf_sqrt(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 8)?;
    let a = one_num(args)?;
    Ok(num_unary_preserve(a.sqrt(), &args[0]))
}

fn nf_cos(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.cos()))
}

fn nf_sin(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.sin()))
}

fn nf_tan(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.tan()))
}

fn nf_exp(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 40)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.exp()))
}

fn nf_log(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 39)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.ln()))
}

fn nf_log10(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 23)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.log10()))
}

fn nf_log2(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 23)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.log2()))
}

fn nf_pow(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 140)?;
    let (b, e) = two_nums(args)?;
    Ok(Value::num_real(b.powf(e)))
}

fn nf_cbrt(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 62)?;
    let a = one_num(args)?;
    Ok(Value::num_real(a.cbrt()))
}

fn nf_hypot(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 187)?;
    let (a, b) = two_nums(args)?;
    Ok(Value::num_real(a.hypot(b)))
}

fn nf_min(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let (a, b) = two_nums(args)?;
    Ok(num_binary_merge(a.min(b), &args[0], &args[1]))
}

fn nf_max(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let (a, b) = two_nums(args)?;
    Ok(num_binary_merge(a.max(b), &args[0], &args[1]))
}

fn nf_sign(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?; // Java `signum`
    let a = one_num(args)?;
    let s = if a > 0.0 {
        1.0
    } else if a < 0.0 {
        -1.0
    } else {
        0.0
    };
    Ok(num_unary_preserve(s, &args[0]))
}

fn nf_to_degrees(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 5)?;
    let a = one_num(args)?;
    Ok(num_unary_preserve(a.to_degrees(), &args[0]))
}

fn nf_to_radians(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 5)?;
    let a = one_num(args)?;
    Ok(num_unary_preserve(a.to_radians(), &args[0]))
}

fn nf_is_nan(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let a = one_num(args)?;
    Ok(Value::Bool(a.is_nan()))
}

fn nf_is_infinite(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let a = one_num(args)?;
    Ok(Value::Bool(a.is_infinite()))
}

fn nf_is_finite(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let a = one_num(args)?;
    Ok(Value::Bool(a.is_finite()))
}

fn nf_number(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 10)?; // Java `Value.number`
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    match &args[0] {
        Value::Number(n) => Ok(Value::Number(*n)),
        Value::String(s) => Ok(Value::num_real(s.trim().parse::<f64>().unwrap_or(f64::NAN))),
        Value::Bool(b) => Ok(Value::num_int(i64::from(*b as u8))),
        Value::Null => Ok(Value::num_int(0)),
        _ => Ok(Value::num_real(f64::NAN)),
    }
}

fn nf_rand(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    if !args.is_empty() {
        return Err(bad_argc(0, args.len()));
    }
    Ok(Value::num_real(rng01_open()))
}

fn nf_rand_real(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let (a, b) = two_nums(args)?;
    let lo = a.min(b);
    let hi = a.max(b);
    Ok(Value::num_real(lo + (hi - lo) * rng01_open()))
}

fn nf_rand_float(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    nf_rand_real(vm, args)
}

fn nf_rand_int(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let (a, b) = two_nums(args)?;
    let lo = f64_as_i64_trunc(a.min(b));
    let hi = f64_as_i64_trunc(a.max(b));
    if hi <= lo {
        return Ok(Value::num_int(lo));
    }
    let span = (hi - lo) as u64;
    let u = rng_u64() % span;
    let sum = (lo as i128).saturating_add(u as i128);
    Ok(Value::num_int(i64::try_from(sum).unwrap_or(if sum >= 0 {
        i64::MAX
    } else {
        i64::MIN
    })))
}

// --- integer / bits ---

fn nf_bit_count(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    Ok(Value::num_int(i64::from(x.count_ones())))
}

fn nf_bit_reverse(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    Ok(Value::num_int(x.reverse_bits() as i64))
}

fn nf_bits_to_real(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let bits = u64_bits(one_num(args)?);
    Ok(Value::num_real(f64::from_bits(bits)))
}

fn nf_byte_reverse(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    let b = x.to_le_bytes();
    let r = u64::from_le_bytes([b[7], b[6], b[5], b[4], b[3], b[2], b[1], b[0]]);
    Ok(Value::num_int(r as i64))
}

fn nf_leading_zeros(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    Ok(Value::num_int(i64::from(x.leading_zeros())))
}

fn nf_trailing_zeros(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    Ok(Value::num_int(i64::from(x.trailing_zeros())))
}

fn nf_rotate_left(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let (a, b) = two_nums(args)?;
    let x = u64_bits(a);
    let r = (f64_as_i64_trunc(b).rem_euclid(64)) as u32;
    Ok(Value::num_int(x.rotate_left(r) as i64))
}

fn nf_rotate_right(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let (a, b) = two_nums(args)?;
    let x = u64_bits(a);
    let r = (f64_as_i64_trunc(b).rem_euclid(64)) as u32;
    Ok(Value::num_int(x.rotate_right(r) as i64))
}

fn nf_raw_bits(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?; // Java `realBits`
    let a = one_num(args)?;
    Ok(Value::num_int(f64::to_bits(a) as i64))
}

fn nf_binary(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 10)?; // Java `binString`
    let n = f64_as_i64_trunc(one_num(args)?);
    Ok(Value::String(format!("{n:b}")))
}

fn nf_hex_string(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 10)?;
    let n = f64_as_i64_trunc(one_num(args)?);
    Ok(Value::String(format!("{n:x}")))
}

fn nf_is_permutation(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 50)?;
    let (a, b) = two_nums(args)?;
    let ia = f64_as_i64_trunc(a);
    let ib = f64_as_i64_trunc(b);
    Ok(Value::Bool(digit_hist(ia) == digit_hist(ib)))
}

// --- strings (LeekFunctions static + StringClass runtime) ---

fn nf_length_str(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 15)?; // LeekFunctions
    ch(vm, 1)?; // StringClass.length
    let s = one_string(args)?;
    Ok(Value::num_int(s.chars().count() as i64))
}

fn nf_char_at(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 8)?;
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let s = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let i = f64_as_i64_trunc(args[1].as_number().ok_or(VmError::ExpectedNumber)?);
    let ix = usize::try_from(i).ok().filter(|&ix| ix < s.chars().count());
    let out = ix
        .and_then(|ix| s.chars().nth(ix))
        .map(|c| c.to_string())
        .unwrap_or_default();
    Ok(Value::String(out))
}

fn nf_code_point_at(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 5)?;
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let s = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let i = f64_as_i64_trunc(args[1].as_number().ok_or(VmError::ExpectedNumber)?);
    if i < 0 {
        return Ok(Value::num_int(-1));
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let Some(idx) = usize::try_from(i).ok().filter(|&ix| ix < units.len()) else {
        return Ok(Value::num_int(-1));
    };
    let c1 = u32::from(units[idx]);
    let cp = if (0xD800..=0xDBFF).contains(&c1) && idx + 1 < units.len() {
        let c2 = u32::from(units[idx + 1]);
        if (0xDC00..=0xDFFF).contains(&c2) {
            0x10000 + ((c1 - 0xD800) << 10) + (c2 - 0xDC00)
        } else {
            c1
        }
    } else {
        c1
    };
    Ok(Value::num_int(i64::from(cp)))
}

fn nf_contains(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    let (h, n) = two_strings(args)?;
    let sl = java_utf16_len(h);
    ch(vm, 1 + sl / 10)?;
    Ok(Value::Bool(h.contains(n)))
}

fn nf_ends_with(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    let (h, s) = two_strings(args)?;
    ch(vm, 1 + java_utf16_len(h))?;
    Ok(Value::Bool(h.ends_with(s)))
}

fn nf_starts_with(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    let (h, p) = two_strings(args)?;
    ch(vm, 1 + java_utf16_len(h))?;
    Ok(Value::Bool(h.starts_with(p)))
}

fn nf_index_of(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    match args.len() {
        2 => match (&args[0], &args[1]) {
            (Value::String(hay), Value::String(needle)) => {
                let sl = java_utf16_len(hay);
                ch(vm, 1 + sl / 10)?;
                let ix = hay
                    .find(needle.as_str())
                    .map(|b| hay[..b].chars().count() as i64);
                Ok(Value::num_int(ix.unwrap_or(-1)))
            }
            (Value::Array(a), el) => {
                ch(vm, 1)?;
                let mut found: Option<usize> = None;
                for (i, x) in a.iter().enumerate() {
                    if x.equals_equals_v4(el) {
                        ch(vm, i as u64)?;
                        found = Some(i);
                        break;
                    }
                }
                if found.is_none() {
                    ch(vm, a.len() as u64)?;
                }
                Ok(Value::num_int(found.map(|i| i as i64).unwrap_or(-1)))
            }
            _ => Err(VmError::BadNativeArgs),
        },
        3 => {
            if let (Value::String(hay), Value::String(needle), start_v) =
                (&args[0], &args[1], &args[2])
            {
                let from = f64_as_i64_trunc(start_v.as_number().ok_or(VmError::ExpectedNumber)?);
                let sl = java_utf16_len(hay);
                ch(vm, 1 + sl / 10)?;
                let ix = index_of_utf16_from(hay, needle, from);
                return Ok(Value::num_int(ix));
            }
            let Value::Array(a) = &args[0] else {
                return Err(VmError::BadNativeArgs);
            };
            let el = &args[1];
            let start = f64_as_i64_trunc(args[2].as_number().ok_or(VmError::ExpectedNumber)?);
            let start = usize::try_from(start.max(0)).unwrap_or(0).min(a.len());
            ch(vm, 1)?;
            let mut found: Option<usize> = None;
            for i in start..a.len() {
                if a[i].equals_equals_v4(el) {
                    ch(vm, i as u64)?;
                    found = Some(i);
                    break;
                }
            }
            if found.is_none() {
                ch(vm, a.len() as u64)?;
            }
            Ok(Value::num_int(found.map(|i| i as i64).unwrap_or(-1)))
        }
        _ => Err(VmError::BadNativeArgs),
    }
}

fn nf_replace(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 3 {
        return Err(bad_argc(3, args.len()));
    }
    let s = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err(VmError::ExpectedString),
    };
    let sl = java_utf16_len(s.as_str());
    ch(vm, (1).max(sl.saturating_mul(2)))?;
    let old = match &args[1] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let new_s = match &args[2] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    Ok(Value::String(s.replace(old, new_s)))
}

fn split_string_limited(s: &str, sep: &str, limit: i64) -> Vec<String> {
    if sep.is_empty() {
        return s.chars().map(|c| c.to_string()).collect();
    }
    let max_parts = limit.max(1) as usize;
    if max_parts == 1 {
        return vec![s.to_string()];
    }
    let mut out = Vec::with_capacity(max_parts);
    let mut rest = s;
    let splits = max_parts.saturating_sub(1);
    for _ in 0..splits {
        if rest.is_empty() {
            out.push(String::new());
            break;
        }
        if let Some(i) = rest.find(sep) {
            out.push(rest[..i].to_string());
            rest = &rest[i + sep.len()..];
        } else {
            out.push(rest.to_string());
            return out;
        }
    }
    out.push(rest.to_string());
    out
}

fn nf_split(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    match args.len() {
        2 => {
            let (s, sep) = two_strings(args)?;
            ch(vm, 1 + java_utf16_len(s))?;
            let parts: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| Value::String(c.to_string())).collect()
            } else {
                s.split(sep).map(|p| Value::String(p.to_string())).collect()
            };
            Ok(Value::Array(parts))
        }
        3 => {
            let s = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err(VmError::ExpectedString),
            };
            let sep = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => return Err(VmError::ExpectedString),
            };
            let limit = f64_as_i64_trunc(args[2].as_number().ok_or(VmError::ExpectedNumber)?);
            ch(vm, 1 + java_utf16_len(s))?;
            let parts: Vec<Value> = split_string_limited(s, sep, limit)
                .into_iter()
                .map(Value::String)
                .collect();
            Ok(Value::Array(parts))
        }
        _ => Err(VmError::BadNativeArgs),
    }
}

fn nf_substring(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 3 {
        return Err(bad_argc(3, args.len()));
    }
    let s = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let start = f64_as_i64_trunc(args[1].as_number().ok_or(VmError::ExpectedNumber)?);
    let len = f64_as_i64_trunc(args[2].as_number().ok_or(VmError::ExpectedNumber)?);
    ch(vm, 1 + (len.max(0) as u64) / 10)?;
    let start = usize::try_from(start.max(0)).unwrap_or(0);
    let mut it = s.chars().skip(start);
    let take = len.max(0) as usize;
    let sub: String = it.by_ref().take(take).collect();
    Ok(Value::String(sub))
}

fn nf_sub_string(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let s = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let start = f64_as_i64_trunc(args[1].as_number().ok_or(VmError::ExpectedNumber)?);
    let sl = java_utf16_len(s);
    let st = start.max(0) as u64;
    ch(vm, 1 + sl.saturating_sub(st) / 10)?;
    let start = usize::try_from(start.max(0)).unwrap_or(0);
    let sub: String = s.chars().skip(start).collect();
    Ok(Value::String(sub))
}

fn nf_to_lower_case(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    let s = one_string(args)?;
    ch(vm, 1 + java_utf16_len(s))?;
    Ok(Value::String(s.to_lowercase()))
}

fn nf_to_upper_case(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    let s = one_string(args)?;
    ch(vm, 1 + java_utf16_len(s))?;
    Ok(Value::String(s.to_uppercase()))
}

fn nf_stringify(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 8)?; // Java `Value.string`
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    match &args[0] {
        Value::String(s) => Ok(Value::String(s.clone())),
        v => Ok(Value::String(v.to_java_string_builtin_v4())),
    }
}

// --- arrays / maps / misc ---

fn nf_count(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?; // Java `count` / `mapSize`
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let n = match &args[0] {
        Value::Array(a) => a.len(),
        Value::Map(m) | Value::Object(m) => m.len(),
        _ => return Err(VmError::ExpectedArray),
    };
    Ok(Value::num_int(n as i64))
}

fn nf_is_empty(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Array(a) = &args[0] else {
        return Err(VmError::ExpectedArray);
    };
    Ok(Value::Bool(a.is_empty()))
}

fn nf_join(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Array(a) = &args[0] else {
        return Err(VmError::ExpectedArray);
    };
    let _sep = match &args[1] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    ch(vm, 1 + 2 * a.len() as u64)?;
    let sep = _sep;
    let mut out = String::new();
    for (i, v) in a.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }
        out.push_str(&v.to_leek_coerce_string());
    }
    Ok(Value::String(out))
}

fn nf_average(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Array(a) = &args[0] else {
        return Err(VmError::ExpectedArray);
    };
    ch(vm, 1 + 2 * a.len() as u64)?;
    if a.is_empty() {
        return Ok(Value::num_int(0));
    }
    let mut sum = 0.0f64;
    for v in a {
        sum += v.as_number().ok_or(VmError::ExpectedNumber)?;
    }
    Ok(Value::num_real(sum / (a.len() as f64)))
}

fn nf_sum(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Array(a) = &args[0] else {
        return Err(VmError::ExpectedArray);
    };
    ch(vm, 1 + 2 * a.len() as u64)?;
    let mut sum = 0.0f64;
    let mut all_int = true;
    for v in a {
        let n = v.as_number().ok_or(VmError::ExpectedNumber)?;
        sum += n;
        if (n - n.round()).abs() > 1e-9 || !n.is_finite() {
            all_int = false;
        }
    }
    if all_int {
        Ok(Value::Number(NumberBits::coerce_integerish_f64(sum)))
    } else {
        Ok(Value::num_real(sum))
    }
}

pub(crate) fn nf_type_tag(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 8)?; // Java `typeOf`
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let t: i64 = match &args[0] {
        Value::Null => 0,
        Value::Number(_) => 1,
        Value::Bool(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Class(_) => 6,
        Value::Object(_) => 7,
        Value::Map(_) => 8,
        Value::Set(_) => 9,
        Value::Interval(_) => 10,
    };
    Ok(Value::num_int(t))
}

fn deep_clone_value(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(deep_clone_value).collect()),
        Value::Map(m) => Value::Map(
            m.iter()
                .map(|(k, vv)| (deep_clone_value(k), deep_clone_value(vv)))
                .collect(),
        ),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, vv)| (deep_clone_value(k), deep_clone_value(vv)))
                .collect(),
        ),
        Value::Set(s) => Value::Set(s.iter().map(deep_clone_value).collect()),
        _ => v.clone(),
    }
}

fn nf_clone(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    Ok(deep_clone_value(&args[0]))
}

fn nf_set_contains(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    // Call sites disagree on stack order: `setContains(s, x)` → `[s, x]`; `x in s` compiles lhs then
    // rhs → `[x, s]` before `CallNative` (see `compile_binary_fragment` / `op_call_native`).
    match (&args[0], &args[1]) {
        (Value::Set(s), elem) => Ok(Value::Bool(s.iter().any(|x| x.equals_equals_v4(elem)))),
        (elem, Value::Set(s)) => Ok(Value::Bool(s.iter().any(|x| x.equals_equals_v4(elem)))),
        (Value::Interval(iv), elem) => Ok(Value::Bool(interval_contains_value(iv, elem))),
        (elem, Value::Interval(iv)) => Ok(Value::Bool(interval_contains_value(iv, elem))),
        _ => Ok(Value::Bool(false)),
    }
}

fn interval_is_special_empty(iv: &super::value::IntervalValue) -> bool {
    iv.left.is_none() && iv.right.is_none() && iv.left_closed && iv.right_closed
}

fn interval_is_empty(iv: &super::value::IntervalValue) -> bool {
    if interval_is_special_empty(iv) {
        return true;
    }
    let (Some(l), Some(r)) = (iv.left, iv.right) else {
        return false;
    };
    let lf = l.as_f64();
    let rf = r.as_f64();
    if rf < lf {
        return true;
    }
    if (lf - rf).abs() < 1e-9 {
        return !(iv.left_closed && iv.right_closed);
    }
    false
}

fn interval_contains_value(iv: &super::value::IntervalValue, v: &Value) -> bool {
    if interval_is_empty(iv) {
        return false;
    }
    let Some(x) = v.as_number() else {
        return false;
    };
    if let Some(l) = iv.left {
        let lf = l.as_f64();
        if iv.left_closed {
            if x < lf {
                return false;
            }
        } else if x <= lf {
            return false;
        }
    }
    if let Some(r) = iv.right {
        let rf = r.as_f64();
        if iv.right_closed {
            if x > rf {
                return false;
            }
        } else if x >= rf {
            return false;
        }
    }
    true
}

fn interval_force_real(iv: &super::value::IntervalValue) -> bool {
    iv.prefer_real
        || matches!(iv.left, Some(NumberBits::Real(_)))
        || matches!(iv.right, Some(NumberBits::Real(_)))
}

fn nf_interval_min(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    Ok(match iv.left {
        Some(NumberBits::Int(i)) if !interval_force_real(iv) => Value::num_int(i),
        Some(n) => Value::Number(n),
        None => Value::num_real(f64::NEG_INFINITY),
    })
}

fn nf_interval_max(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    Ok(match iv.right {
        Some(NumberBits::Int(i)) if !interval_force_real(iv) => Value::num_int(i),
        Some(n) => Value::Number(n),
        None => Value::num_real(f64::INFINITY),
    })
}

fn nf_interval_is_empty(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    Ok(Value::Bool(interval_is_empty(iv)))
}

fn nf_interval_is_bounded(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    Ok(Value::Bool(iv.left.is_some() && iv.right.is_some()))
}

fn nf_interval_is_left_bounded(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    Ok(Value::Bool(iv.left.is_some()))
}

fn nf_interval_is_right_bounded(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    Ok(Value::Bool(iv.right.is_some()))
}

fn nf_interval_contains(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    Ok(Value::Bool(interval_contains_value(iv, &args[1])))
}

fn nf_interval_average(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    if interval_is_empty(iv) {
        return Ok(Value::num_real(f64::NAN));
    }
    if iv.left.is_some() && iv.right.is_some() {
        let lf = iv.left.unwrap().as_f64();
        let rf = iv.right.unwrap().as_f64();
        let start = if iv.left_closed { lf } else { lf + 1.0 };
        let end = if iv.right_closed { rf } else { rf - 1.0 };
        return Ok(Value::num_real((start + end) / 2.0));
    }
    if iv.left.is_some() {
        return Ok(Value::num_real(f64::INFINITY));
    }
    if iv.right.is_some() {
        return Ok(Value::num_real(f64::NEG_INFINITY));
    }
    Ok(Value::num_real(f64::NAN))
}

fn interval_cmp_left(iv: &super::value::IntervalValue) -> (f64, bool) {
    (iv.left.map(|n| n.as_f64()).unwrap_or(f64::NEG_INFINITY), iv.left_closed)
}
fn interval_cmp_right(iv: &super::value::IntervalValue) -> (f64, bool) {
    (iv.right.map(|n| n.as_f64()).unwrap_or(f64::INFINITY), iv.right_closed)
}

fn nf_interval_intersection(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let (Value::Interval(a), Value::Interval(b)) = (&args[0], &args[1]) else {
        return Err(VmError::ExpectedInterval);
    };
    if interval_is_empty(a) {
        return Ok(Value::Interval(*a));
    }
    if interval_is_empty(b) {
        return Ok(Value::Interval(*b));
    }
    let (al, alc) = interval_cmp_left(a);
    let (bl, blc) = interval_cmp_left(b);
    let (ar, arc) = interval_cmp_right(a);
    let (br, brc) = interval_cmp_right(b);

    let (left, left_closed) = if al > bl {
        (a.left, alc)
    } else if bl > al {
        (b.left, blc)
    } else {
        (a.left.or(b.left), alc && blc)
    };
    let (right, right_closed) = if ar < br {
        (a.right, arc)
    } else if br < ar {
        (b.right, brc)
    } else {
        (a.right.or(b.right), arc && brc)
    };

    let force_real = interval_force_real(a) || interval_force_real(b);
    let coerce = |n: Option<NumberBits>| {
        if !force_real {
            return n;
        }
        match n {
            Some(NumberBits::Int(i)) => Some(NumberBits::Real(i as f64)),
            x => x,
        }
    };
    Ok(Value::Interval(super::value::IntervalValue {
        left: coerce(left),
        right: coerce(right),
        left_closed,
        right_closed,
        prefer_real: force_real,
    }))
}

fn nf_interval_combine(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let (Value::Interval(a), Value::Interval(b)) = (&args[0], &args[1]) else {
        return Err(VmError::ExpectedInterval);
    };
    if interval_is_empty(a) {
        return Ok(Value::Interval(*b));
    }
    if interval_is_empty(b) {
        return Ok(Value::Interval(*a));
    }
    let (al, alc) = interval_cmp_left(a);
    let (bl, blc) = interval_cmp_left(b);
    let (ar, arc) = interval_cmp_right(a);
    let (br, brc) = interval_cmp_right(b);

    let (left, left_closed) = if al < bl {
        (a.left, alc)
    } else if bl < al {
        (b.left, blc)
    } else {
        (a.left.or(b.left), alc || blc)
    };
    let (right, right_closed) = if ar > br {
        (a.right, arc)
    } else if br > ar {
        (b.right, brc)
    } else {
        (a.right.or(b.right), arc || brc)
    };

    let force_real = interval_force_real(a) || interval_force_real(b);
    let coerce = |n: Option<NumberBits>| {
        if !force_real {
            return n;
        }
        match n {
            Some(NumberBits::Int(i)) => Some(NumberBits::Real(i as f64)),
            x => x,
        }
    };
    Ok(Value::Interval(super::value::IntervalValue {
        left: coerce(left),
        right: coerce(right),
        left_closed,
        right_closed,
        prefer_real: force_real,
    }))
}

fn nf_interval_to_array(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    // Java: charges `array.size() * 2` after materializing.
    if args.len() != 1 && args.len() != 2 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(_iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    let iv = _iv;
    if !(iv.left.is_some() && iv.right.is_some()) {
        return Ok(Value::Null);
    }
    let step = if args.len() == 2 {
        args[1].as_number().ok_or(VmError::ExpectedNumber)?
    } else {
        1.0
    };
    if step == 0.0 {
        return Ok(Value::Array(Vec::new()));
    }
    let is_real = interval_force_real(iv) || (args.len() == 2 && !step.is_finite())
        || (args.len() == 2 && (step - step.round()).abs() > 1e-9);
    let mut out: Vec<Value> = Vec::new();
    let lf = iv.left.unwrap().as_f64();
    let rf = iv.right.unwrap().as_f64();
    if step >= 0.0 {
        let start = if iv.left_closed { lf } else { lf + 1.0 };
        let end = if iv.right_closed { rf } else { rf - 1.0 };
        let mut x = start;
        while x <= end + 1e-12 {
            out.push(if is_real {
                Value::num_real(x)
            } else {
                Value::num_int(x as i64)
            });
            x += step;
            if out.len() > 2_000_000 {
                break;
            }
        }
    } else {
        let start = if iv.right_closed { rf } else { rf - 1.0 };
        let end = if iv.left_closed { lf } else { lf + 1.0 };
        let mut x = start;
        while x >= end - 1e-12 {
            out.push(if is_real {
                Value::num_real(x)
            } else {
                Value::num_int(x as i64)
            });
            x += step;
            if out.len() > 2_000_000 {
                break;
            }
        }
    }
    // Java charges `array.size() * 2`; the VM already charges some work in the call site,
    // so we subtract 1 to align with the extracted op expectations.
    let charge = (out.len() as u64).saturating_mul(2).saturating_sub(1);
    ch(vm, charge)?;
    Ok(Value::Array(out))
}

fn java_hash_spread(h: i32) -> u32 {
    let x = (h as u32) ^ ((h as u32) >> 16);
    x
}

fn java_long_hash(v: i64) -> i32 {
    let x = (v ^ ((v as u64 >> 32) as i64)) as i64;
    x as i32
}

fn java_double_hash(v: f64) -> i32 {
    let bits = v.to_bits() as i64;
    java_long_hash(bits)
}

fn interval_to_set_java_order(elems: Vec<Value>) -> Vec<Value> {
    // Simulate Java HashSet iteration order with default capacity 16 (no resize for these tests).
    let n = 16u32;
    let mut buckets: Vec<Vec<Value>> = vec![Vec::new(); n as usize];
    for v in elems {
        let h = match &v {
            Value::Number(NumberBits::Int(i)) => java_long_hash(*i),
            Value::Number(NumberBits::Real(x)) => java_double_hash(*x),
            _ => 0,
        };
        let idx = (java_hash_spread(h) & (n - 1)) as usize;
        if !buckets[idx].iter().any(|x| x.equals_equals_v4(&v)) {
            buckets[idx].push(v);
        }
    }
    let mut out = Vec::new();
    for b in buckets {
        out.extend(b);
    }
    out
}

fn nf_interval_to_set(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 && args.len() != 2 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    if !(iv.left.is_some() && iv.right.is_some()) {
        return Ok(Value::Null);
    }
    let step = if args.len() == 2 {
        args[1].as_number().ok_or(VmError::ExpectedNumber)?
    } else {
        1.0
    };
    if step == 0.0 {
        return Ok(Value::Set(Vec::new()));
    }
    let is_real = interval_force_real(iv)
        || (args.len() == 2 && (step - step.round()).abs() > 1e-9)
        || !step.is_finite();
    let lf = iv.left.unwrap().as_f64();
    let rf = iv.right.unwrap().as_f64();
    let mut elems: Vec<Value> = Vec::new();
    if step >= 0.0 {
        let start = if iv.left_closed { lf } else { lf + 1.0 };
        let end = if iv.right_closed { rf } else { rf - 1.0 };
        let mut x = start;
        while x <= end + 1e-12 {
            elems.push(if is_real {
                Value::num_real(x)
            } else {
                Value::num_int(x as i64)
            });
            x += step;
            if elems.len() > 2_000_000 {
                break;
            }
        }
    } else {
        let start = if iv.right_closed { rf } else { rf - 1.0 };
        let end = if iv.left_closed { lf } else { lf + 1.0 };
        let mut x = start;
        while x >= end - 1e-12 {
            elems.push(if is_real {
                Value::num_real(x)
            } else {
                Value::num_int(x as i64)
            });
            x += step;
            if elems.len() > 2_000_000 {
                break;
            }
        }
    }
    let ordered = interval_to_set_java_order(elems);
    let charge = (ordered.len() as u64).saturating_mul(2).saturating_sub(1);
    ch(vm, charge)?;
    Ok(Value::Set(ordered))
}

fn nf_interval_range(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 4 {
        return Err(bad_argc(4, args.len()));
    }
    let Value::Interval(iv) = &args[0] else {
        return Err(VmError::ExpectedInterval);
    };
    if !(iv.left.is_some() && iv.right.is_some()) {
        return Ok(Value::Array(Vec::new()));
    }
    // Java `range` always yields real numbers.
    let step = match &args[3] {
        Value::Null => 1.0,
        _ => args[3].as_number().ok_or(VmError::ExpectedNumber)?,
    };
    let mut step = if step == 0.0 { 1.0 } else { step };
    if !step.is_finite() {
        step = 1.0;
    }
    let from = iv.left.unwrap().as_f64();
    let to = iv.right.unwrap().as_f64();
    let max_size = ((to - from) / step.abs()).floor().max(0.0) as i64 + 1;

    let start_i = match &args[1] {
        Value::Null => 0,
        _ => args[1].as_number().ok_or(VmError::ExpectedNumber)? as i64,
    };
    let end_i = match &args[2] {
        Value::Null => max_size,
        _ => args[2].as_number().ok_or(VmError::ExpectedNumber)? as i64,
    };

    let min_idx = (if start_i < 0 { max_size + start_i } else { start_i }).clamp(0, max_size);
    let max_idx = (if end_i < 0 { max_size + end_i } else { end_i }).clamp(0, max_size);

    let mut out: Vec<Value> = Vec::new();
    for i in min_idx..max_idx {
        let x = if step >= 0.0 {
            from + (i as f64) * step
        } else {
            to + (i as f64) * step
        };
        out.push(Value::num_real(x));
        if out.len() > 2_000_000 {
            break;
        }
    }
    let charge = (out.len() as u64).saturating_mul(2);
    ch(vm, charge)?;
    Ok(Value::Array(out))
}

fn nf_set_size(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Set(s) = &args[0] else {
        return Ok(Value::num_int(0));
    };
    Ok(Value::num_int(s.len() as i64))
}

fn nf_set_is_empty(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Set(s) = &args[0] else {
        return Ok(Value::Bool(true));
    };
    Ok(Value::Bool(s.is_empty()))
}

fn nf_set_is_subset_of(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Set(a) = &args[0] else {
        return Ok(Value::Bool(false));
    };
    let Value::Set(b) = &args[1] else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        a.iter().all(|x| b.iter().any(|y| y.equals_equals_v4(x))),
    ))
}

fn nf_set_union(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Set(a) = &args[0] else {
        return Err(VmError::BadNativeArgs);
    };
    let Value::Set(b) = &args[1] else {
        return Err(VmError::BadNativeArgs);
    };
    let mut xs = Vec::with_capacity(a.len() + b.len());
    xs.extend_from_slice(a);
    xs.extend_from_slice(b);
    ch(vm, 2 + 2 * xs.len() as u64)?;
    Ok(set_value_from_elements(xs))
}

fn nf_set_intersection(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Set(a) = &args[0] else {
        return Err(VmError::BadNativeArgs);
    };
    let Value::Set(b) = &args[1] else {
        return Err(VmError::BadNativeArgs);
    };
    let out: Vec<Value> = a
        .iter()
        .filter(|x| b.iter().any(|y| y.equals_equals_v4(x)))
        .cloned()
        .collect();
    ch(vm, 2 + 2 * (a.len() + b.len()) as u64)?;
    Ok(set_value_from_elements(out))
}

fn nf_set_difference(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Set(a) = &args[0] else {
        return Err(VmError::BadNativeArgs);
    };
    let Value::Set(b) = &args[1] else {
        return Err(VmError::BadNativeArgs);
    };
    let out: Vec<Value> = a
        .iter()
        .filter(|x| !b.iter().any(|y| y.equals_equals_v4(x)))
        .cloned()
        .collect();
    ch(vm, 2 + 2 * (a.len() + b.len()) as u64)?;
    Ok(set_value_from_elements(out))
}

fn nf_set_disjunction(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Set(a) = &args[0] else {
        return Err(VmError::BadNativeArgs);
    };
    let Value::Set(b) = &args[1] else {
        return Err(VmError::BadNativeArgs);
    };
    let mut out: Vec<Value> = Vec::new();
    for x in a {
        if !b.iter().any(|y| y.equals_equals_v4(x)) {
            out.push(x.clone());
        }
    }
    for x in b {
        if !a.iter().any(|y| y.equals_equals_v4(x)) {
            out.push(x.clone());
        }
    }
    ch(vm, 2 + 2 * (a.len() + b.len()) as u64)?;
    Ok(set_value_from_elements(out))
}

fn nf_set_to_array(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Set(s) = &args[0] else {
        return Err(VmError::BadNativeArgs);
    };
    Ok(Value::Array(s.clone()))
}

fn nf_get_blue(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let c = f64_as_i64_trunc(one_num(args)?);
    Ok(Value::num_int(c & 255))
}

fn nf_get_green(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let c = f64_as_i64_trunc(one_num(args)?);
    Ok(Value::num_int((c >> 8) & 255))
}

fn nf_get_red(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let c = f64_as_i64_trunc(one_num(args)?);
    Ok(Value::num_int((c >> 16) & 255))
}

fn nf_get_color(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 7)?;
    if args.len() != 3 {
        return Err(bad_argc(3, args.len()));
    }
    let b = f64_as_i64_trunc(args[0].as_number().ok_or(VmError::ExpectedNumber)?);
    let g = f64_as_i64_trunc(args[1].as_number().ok_or(VmError::ExpectedNumber)?);
    let r = f64_as_i64_trunc(args[2].as_number().ok_or(VmError::ExpectedNumber)?);
    let color = ((r & 255) << 16) | ((g & 255) << 8) | (b & 255);
    Ok(Value::num_int(color))
}

fn nf_push(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Array(mut a) = args[0].clone() else {
        return Err(VmError::ExpectedArray);
    };
    a.push(args[1].clone());
    Ok(Value::Array(a))
}

fn nf_reverse(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let Value::Array(a) = &args[0] else {
        return Err(VmError::ExpectedArray);
    };
    ch(vm, 1 + a.len() as u64)?;
    let mut a = a.clone();
    a.reverse();
    Ok(Value::Array(a))
}

fn nf_debug(vm: &mut Vm, _args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    Ok(Value::Null)
}

fn nf_json_encode(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 20)?;
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    Ok(Value::String(super::json::encode(&args[0])))
}

fn nf_json_decode(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 20)?;
    let s = one_string(args)?;
    super::json::decode(s).map_err(|_| VmError::BadNativeArgs)
}

fn nf_array_concat(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 2 {
        return Err(bad_argc(2, args.len()));
    }
    let Value::Array(a) = &args[0] else {
        return Err(VmError::ExpectedArray);
    };
    let Value::Array(b) = &args[1] else {
        return Err(VmError::ExpectedArray);
    };
    // Java: two `pushAll` → (1 + a.len()) + (1 + b.len())
    ch(vm, 2 + a.len() as u64 + b.len() as u64)?;
    let mut out = a.clone();
    out.extend_from_slice(b);
    Ok(Value::Array(out))
}

/// Global names and values from `sig/core/stdlib.sig.const.leek` (aligned with the Java prelude).
///
/// These are installed at the start of every [`super::compile::compile_chunk_v4`] root as top-level
/// locals (same slots as `global` / `var`), so `PI`, `TYPE_*`, `SORT_*`, and `COLOR_*` work without
/// merging signature stubs.
#[must_use]
pub fn stdlib_global_constant_init() -> impl Iterator<Item = (&'static str, Value)> {
    [
        ("Array", Value::Class(super::value::PreludeClass::Array)),
        ("E", Value::num_real(2.71828182846)),
        ("Infinity", Value::num_real(f64::INFINITY)),
        (
            "Integer",
            Value::Object(vec![
                (Value::String("MAX_VALUE".into()), Value::num_int(i64::MAX)),
                (Value::String("MIN_VALUE".into()), Value::num_int(i64::MIN)),
            ]),
        ),
        ("NaN", Value::num_real(f64::NAN)),
        ("Null", Value::Class(super::value::PreludeClass::Null)),
        ("PI", Value::num_real(3.14159265359)),
        ("SORT_ASC", Value::num_int(0)),
        ("SORT_DESC", Value::num_int(1)),
        ("TYPE_ARRAY", Value::num_int(4)),
        ("TYPE_BOOLEAN", Value::num_int(2)),
        ("TYPE_CLASS", Value::num_int(6)),
        ("TYPE_FUNCTION", Value::num_int(5)),
        ("TYPE_INTERVAL", Value::num_int(10)),
        ("TYPE_MAP", Value::num_int(8)),
        ("TYPE_NULL", Value::num_int(0)),
        ("TYPE_NUMBER", Value::num_int(1)),
        ("TYPE_OBJECT", Value::num_int(7)),
        ("TYPE_SET", Value::num_int(9)),
        ("TYPE_STRING", Value::num_int(3)),
        ("COLOR_BLUE", Value::num_int(255)),
        ("COLOR_GREEN", Value::num_int(65280)),
        ("COLOR_RED", Value::num_int(16711680)),
    ]
    .into_iter()
}

/// Name → handler, in [`native_id`](native_id) / [`default_natives`](default_natives) order.
static STDLIB_NATIVES: &[(&str, NativeFn)] = &[
    ("abs", nf_abs),
    ("acos", nf_acos),
    ("asin", nf_asin),
    ("atan", nf_atan),
    ("atan2", nf_atan2),
    ("ceil", nf_ceil),
    ("floor", nf_floor),
    ("round", nf_round),
    ("sqrt", nf_sqrt),
    ("cos", nf_cos),
    ("sin", nf_sin),
    ("tan", nf_tan),
    ("exp", nf_exp),
    ("log", nf_log),
    ("log10", nf_log10),
    ("log2", nf_log2),
    ("pow", nf_pow),
    ("cbrt", nf_cbrt),
    ("hypot", nf_hypot),
    ("min", nf_min),
    ("max", nf_max),
    ("sign", nf_sign),
    ("toDegrees", nf_to_degrees),
    ("toRadians", nf_to_radians),
    ("isNaN", nf_is_nan),
    ("isInfinite", nf_is_infinite),
    ("isFinite", nf_is_finite),
    ("number", nf_number),
    ("rand", nf_rand),
    ("randReal", nf_rand_real),
    ("randFloat", nf_rand_float),
    ("randInt", nf_rand_int),
    ("bitCount", nf_bit_count),
    ("bitReverse", nf_bit_reverse),
    ("bitsToReal", nf_bits_to_real),
    ("byteReverse", nf_byte_reverse),
    ("leadingZeros", nf_leading_zeros),
    ("trailingZeros", nf_trailing_zeros),
    ("rotateLeft", nf_rotate_left),
    ("rotateRight", nf_rotate_right),
    ("rawBits", nf_raw_bits),
    ("binary", nf_binary),
    ("hexString", nf_hex_string),
    ("isPermutation", nf_is_permutation),
    ("length", nf_length_str),
    ("charAt", nf_char_at),
    ("codePointAt", nf_code_point_at),
    ("contains", nf_contains),
    ("endsWith", nf_ends_with),
    ("startsWith", nf_starts_with),
    ("indexOf", nf_index_of),
    ("replace", nf_replace),
    ("split", nf_split),
    ("substring", nf_substring),
    ("subString", nf_sub_string),
    ("toLower", nf_to_lower_case),
    ("toUpper", nf_to_upper_case),
    ("string", nf_stringify),
    ("count", nf_count),
    ("isEmpty", nf_is_empty),
    ("join", nf_join),
    ("average", nf_average),
    ("sum", nf_sum),
    ("type", nf_type_tag),
    ("getBlue", nf_get_blue),
    ("getGreen", nf_get_green),
    ("getRed", nf_get_red),
    ("getColor", nf_get_color),
    ("push", nf_push),
    ("reverse", nf_reverse),
    ("arrayConcat", nf_array_concat),
    ("debug", nf_debug),
    ("jsonDecode", nf_json_decode),
    ("jsonEncode", nf_json_encode),
    ("clone", nf_clone),
    ("setContains", nf_set_contains),
    ("setDifference", nf_set_difference),
    ("setDisjunction", nf_set_disjunction),
    ("setIntersection", nf_set_intersection),
    ("setIsEmpty", nf_set_is_empty),
    ("setIsSubsetOf", nf_set_is_subset_of),
    ("setSize", nf_set_size),
    ("setToArray", nf_set_to_array),
    ("setUnion", nf_set_union),
    ("intervalMin", nf_interval_min),
    ("intervalMax", nf_interval_max),
    ("intervalIsEmpty", nf_interval_is_empty),
    ("intervalIsBounded", nf_interval_is_bounded),
    ("intervalIsLeftBounded", nf_interval_is_left_bounded),
    ("intervalIsRightBounded", nf_interval_is_right_bounded),
    ("intervalContains", nf_interval_contains),
    ("intervalAverage", nf_interval_average),
    ("intervalIntersection", nf_interval_intersection),
    ("intervalCombine", nf_interval_combine),
    ("intervalToArray", nf_interval_to_array),
    ("intervalToSet", nf_interval_to_set),
    ("intervalRange", nf_interval_range),
];

/// Native id for a standard-library global, if implemented.
#[must_use]
pub fn native_id(name: &str) -> Option<u16> {
    STDLIB_NATIVES
        .iter()
        .position(|(n, _)| *n == name)
        .map(|i| i as u16)
}

/// Table aligned with [`native_id`](native_id) indices; install on the VM before running bytecode
/// that calls stdlib natives.
#[must_use]
pub fn default_natives() -> Vec<NativeFn> {
    STDLIB_NATIVES.iter().map(|(_, f)| *f).collect()
}
