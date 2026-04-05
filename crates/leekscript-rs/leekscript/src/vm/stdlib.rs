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
use super::value::Value;

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
    Ok(Value::Number(a.abs()))
}

fn nf_acos(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.acos()))
}

fn nf_asin(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.asin()))
}

fn nf_atan(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.atan()))
}

fn nf_atan2(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 35)?;
    let (y, x) = two_nums(args)?;
    Ok(Value::Number(y.atan2(x)))
}

fn nf_ceil(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.ceil()))
}

fn nf_floor(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.floor()))
}

fn nf_round(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.round()))
}

fn nf_sqrt(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 8)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.sqrt()))
}

fn nf_cos(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.cos()))
}

fn nf_sin(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.sin()))
}

fn nf_tan(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.tan()))
}

fn nf_exp(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 40)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.exp()))
}

fn nf_log(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 39)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.ln()))
}

fn nf_log10(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 23)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.log10()))
}

fn nf_log2(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 23)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.log2()))
}

fn nf_pow(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 140)?;
    let (b, e) = two_nums(args)?;
    Ok(Value::Number(b.powf(e)))
}

fn nf_cbrt(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 62)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.cbrt()))
}

fn nf_hypot(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 187)?;
    let (a, b) = two_nums(args)?;
    Ok(Value::Number(a.hypot(b)))
}

fn nf_min(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let (a, b) = two_nums(args)?;
    Ok(Value::Number(a.min(b)))
}

fn nf_max(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let (a, b) = two_nums(args)?;
    Ok(Value::Number(a.max(b)))
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
    Ok(Value::Number(s))
}

fn nf_to_degrees(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 5)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.to_degrees()))
}

fn nf_to_radians(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 5)?;
    let a = one_num(args)?;
    Ok(Value::Number(a.to_radians()))
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
        Value::String(s) => Ok(Value::Number(s.trim().parse::<f64>().unwrap_or(f64::NAN))),
        Value::Bool(b) => Ok(Value::Number(f64::from(*b as u8))),
        Value::Null => Ok(Value::Number(0.0)),
        _ => Ok(Value::Number(f64::NAN)),
    }
}

fn nf_rand(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    if !args.is_empty() {
        return Err(bad_argc(0, args.len()));
    }
    Ok(Value::Number(rng01_open()))
}

fn nf_rand_real(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 30)?;
    let (a, b) = two_nums(args)?;
    let lo = a.min(b);
    let hi = a.max(b);
    Ok(Value::Number(lo + (hi - lo) * rng01_open()))
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
        return Ok(Value::Number(lo as f64));
    }
    let span = (hi - lo) as u64;
    let u = rng_u64() % span;
    Ok(Value::Number((lo as i128 + u as i128) as f64))
}

// --- integer / bits ---

fn nf_bit_count(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    Ok(Value::Number(x.count_ones() as f64))
}

fn nf_bit_reverse(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    Ok(Value::Number(x.reverse_bits() as i64 as f64))
}

fn nf_bits_to_real(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let bits = u64_bits(one_num(args)?);
    Ok(Value::Number(f64::from_bits(bits)))
}

fn nf_byte_reverse(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    let b = x.to_le_bytes();
    let r = u64::from_le_bytes([
        b[7], b[6], b[5], b[4], b[3], b[2], b[1], b[0],
    ]);
    Ok(Value::Number(r as i64 as f64))
}

fn nf_leading_zeros(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    Ok(Value::Number(x.leading_zeros() as f64))
}

fn nf_trailing_zeros(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let x = u64_bits(one_num(args)?);
    Ok(Value::Number(x.trailing_zeros() as f64))
}

fn nf_rotate_left(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let (a, b) = two_nums(args)?;
    let x = u64_bits(a);
    let r = (f64_as_i64_trunc(b).rem_euclid(64)) as u32;
    Ok(Value::Number(x.rotate_left(r) as i64 as f64))
}

fn nf_rotate_right(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let (a, b) = two_nums(args)?;
    let x = u64_bits(a);
    let r = (f64_as_i64_trunc(b).rem_euclid(64)) as u32;
    Ok(Value::Number(x.rotate_right(r) as i64 as f64))
}

fn nf_raw_bits(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?; // Java `realBits`
    let a = one_num(args)?;
    Ok(Value::Number(f64::to_bits(a) as i64 as f64))
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
    Ok(Value::Number(s.chars().count() as f64))
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
    let i = f64_as_i64_trunc(
        args[1]
            .as_number()
            .ok_or(VmError::ExpectedNumber)?,
    );
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
    let i = f64_as_i64_trunc(
        args[1]
            .as_number()
            .ok_or(VmError::ExpectedNumber)?,
    );
    let ix = usize::try_from(i).ok().filter(|&ix| ix < s.chars().count());
    let cp = ix
        .and_then(|ix| s.chars().nth(ix))
        .map(|c| c as u32 as f64)
        .unwrap_or(-1.0);
    Ok(Value::Number(cp))
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
                let ix = hay.find(needle.as_str()).map(|b| hay[..b].chars().count() as f64);
                Ok(Value::Number(ix.unwrap_or(-1.0)))
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
                Ok(Value::Number(found.map(|i| i as f64).unwrap_or(-1.0)))
            }
            _ => Err(VmError::BadNativeArgs),
        },
        3 => {
            let Value::Array(a) = &args[0] else {
                return Err(VmError::ExpectedArray);
            };
            let el = &args[1];
            let start = f64_as_i64_trunc(
                args[2]
                    .as_number()
                    .ok_or(VmError::ExpectedNumber)?,
            );
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
            Ok(Value::Number(found.map(|i| i as f64).unwrap_or(-1.0)))
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

fn nf_split(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    let (s, sep) = two_strings(args)?;
    ch(vm, 1 + java_utf16_len(s))?;
    let parts: Vec<Value> = if sep.is_empty() {
        s.chars().map(|c| Value::String(c.to_string())).collect()
    } else {
        s.split(sep)
            .map(|p| Value::String(p.to_string()))
            .collect()
    };
    Ok(Value::Array(parts))
}

fn nf_substring(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    if args.len() != 3 {
        return Err(bad_argc(3, args.len()));
    }
    let s = match &args[0] {
        Value::String(s) => s.as_str(),
        _ => return Err(VmError::ExpectedString),
    };
    let start = f64_as_i64_trunc(
        args[1]
            .as_number()
            .ok_or(VmError::ExpectedNumber)?,
    );
    let len = f64_as_i64_trunc(
        args[2]
            .as_number()
            .ok_or(VmError::ExpectedNumber)?,
    );
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
    let start = f64_as_i64_trunc(
        args[1]
            .as_number()
            .ok_or(VmError::ExpectedNumber)?,
    );
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
        v => Ok(Value::String(v.to_leek_coerce_string())),
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
        Value::Map(m) => m.len(),
        _ => return Err(VmError::ExpectedArray),
    };
    Ok(Value::Number(n as f64))
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
        return Ok(Value::Number(0.0));
    }
    let mut sum = 0.0f64;
    for v in a {
        sum += v.as_number().ok_or(VmError::ExpectedNumber)?;
    }
    Ok(Value::Number(sum / (a.len() as f64)))
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
        Ok(Value::Number(sum.round()))
    } else {
        Ok(Value::Number(sum))
    }
}

fn nf_type_tag(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 8)?; // Java `typeOf`
    if args.len() != 1 {
        return Err(bad_argc(1, args.len()));
    }
    let t = match &args[0] {
        Value::Null => 0.0,
        Value::Number(_) => 1.0,
        Value::Bool(_) => 2.0,
        Value::String(_) => 3.0,
        Value::Array(_) => 4.0,
        Value::Map(_) => 8.0,
    };
    Ok(Value::Number(t))
}

fn nf_get_blue(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 1)?;
    let c = f64_as_i64_trunc(one_num(args)?);
    Ok(Value::Number((c & 255) as f64))
}

fn nf_get_green(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let c = f64_as_i64_trunc(one_num(args)?);
    Ok(Value::Number(((c >> 8) & 255) as f64))
}

fn nf_get_red(vm: &mut Vm, args: &[Value]) -> Result<Value, VmError> {
    ch(vm, 2)?;
    let c = f64_as_i64_trunc(one_num(args)?);
    Ok(Value::Number(((c >> 16) & 255) as f64))
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
    Ok(Value::Number(color as f64))
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
        ("E", Value::Number(2.71828182846)),
        ("Infinity", Value::Number(f64::INFINITY)),
        ("NaN", Value::Number(f64::NAN)),
        ("PI", Value::Number(3.14159265359)),
        ("SORT_ASC", Value::Number(0.0)),
        ("SORT_DESC", Value::Number(1.0)),
        ("TYPE_ARRAY", Value::Number(4.0)),
        ("TYPE_BOOLEAN", Value::Number(2.0)),
        ("TYPE_CLASS", Value::Number(6.0)),
        ("TYPE_FUNCTION", Value::Number(5.0)),
        ("TYPE_INTERVAL", Value::Number(10.0)),
        ("TYPE_MAP", Value::Number(8.0)),
        ("TYPE_NULL", Value::Number(0.0)),
        ("TYPE_NUMBER", Value::Number(1.0)),
        ("TYPE_OBJECT", Value::Number(7.0)),
        ("TYPE_SET", Value::Number(9.0)),
        ("TYPE_STRING", Value::Number(3.0)),
        ("COLOR_BLUE", Value::Number(255.0)),
        ("COLOR_GREEN", Value::Number(65280.0)),
        ("COLOR_RED", Value::Number(16711680.0)),
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
    ("toLowerCase", nf_to_lower_case),
    ("toUpperCase", nf_to_upper_case),
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
