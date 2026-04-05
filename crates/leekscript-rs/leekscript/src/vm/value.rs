//! Values carried on the VM stack (minimal set for the first execution tier).

use std::string::String;
use std::vec::Vec;

/// Numeric value: exact `i64` for integers, IEEE `f64` for reals / non-representable results.
#[derive(Debug, Clone, Copy)]
pub enum NumberBits {
    Int(i64),
    Real(f64),
}

impl NumberBits {
    #[inline]
    #[must_use]
    pub const fn int(v: i64) -> Self {
        Self::Int(v)
    }

    #[inline]
    #[must_use]
    pub const fn real(v: f64) -> Self {
        Self::Real(v)
    }

    #[must_use]
    pub fn as_f64(self) -> f64 {
        match self {
            Self::Int(i) => i as f64,
            Self::Real(x) => x,
        }
    }

    #[must_use]
    pub fn is_real(self) -> bool {
        matches!(self, Self::Real(_))
    }

    /// Coerce a float to [`Int`] when it is a finite integer in `i64` range (Java-ish rounding).
    #[must_use]
    pub fn coerce_integerish_f64(x: f64) -> Self {
        if x.is_nan() || x.is_infinite() {
            return Self::Real(x);
        }
        let r = x.round();
        if (x - r).abs() < 1e-9 && r >= i64::MIN as f64 && r <= i64::MAX as f64 {
            Self::Int(r as i64)
        } else {
            Self::Real(x)
        }
    }

    /// Literal from lexer: real token → always [`Real`]; integer token → [`Int`] when exact in range.
    #[must_use]
    pub fn from_literal(is_real_token: bool, x: f64) -> Self {
        if is_real_token {
            Self::Real(x)
        } else {
            Self::coerce_integerish_f64(x)
        }
    }

    #[must_use]
    pub fn add(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Int(a), Self::Int(b)) => match a.checked_add(b) {
                Some(c) => Self::Int(c),
                None => Self::Real(self.as_f64() + rhs.as_f64()),
            },
            _ => Self::Real(self.as_f64() + rhs.as_f64()),
        }
    }

    #[must_use]
    pub fn sub(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Int(a), Self::Int(b)) => match a.checked_sub(b) {
                Some(c) => Self::Int(c),
                None => Self::Real(self.as_f64() - rhs.as_f64()),
            },
            _ => Self::Real(self.as_f64() - rhs.as_f64()),
        }
    }

    #[must_use]
    pub fn mul(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Self::Int(a), Self::Int(b)) => match a.checked_mul(b) {
                Some(c) => Self::Int(c),
                None => Self::Real(self.as_f64() * rhs.as_f64()),
            },
            _ => Self::Real(self.as_f64() * rhs.as_f64()),
        }
    }

    /// `/` — always a real (Java floating division).
    #[must_use]
    pub fn div(self, rhs: Self) -> Result<Self, ()> {
        let d = rhs.as_f64();
        if d == 0.0 {
            return Err(());
        }
        Ok(Self::Real(self.as_f64() / d))
    }

    /// `//` — truncating division; both ints → [`Int`] when possible.
    #[must_use]
    pub fn int_div(self, rhs: Self) -> Result<Self, ()> {
        let d = rhs.as_f64();
        if d == 0.0 {
            return Err(());
        }
        match (self, rhs) {
            (Self::Int(a), Self::Int(b)) if b != 0 => Ok(Self::Int(a / b)),
            _ => Ok(Self::coerce_integerish_f64(
                (self.as_f64() / rhs.as_f64()).trunc(),
            )),
        }
    }

    #[must_use]
    pub fn modulo(self, rhs: Self) -> Result<Self, ()> {
        match (self, rhs) {
            (Self::Int(a), Self::Int(b)) => {
                if b == 0 {
                    return Err(());
                }
                Ok(Self::Int(a % b))
            }
            _ => {
                let d = rhs.as_f64();
                if d == 0.0 {
                    return Err(());
                }
                Ok(Self::Real(self.as_f64() % d))
            }
        }
    }

    #[must_use]
    pub fn neg(self) -> Self {
        match self {
            Self::Int(a) => a
                .checked_neg()
                .map(Self::Int)
                .unwrap_or(Self::Real(-(a as f64))),
            Self::Real(x) => Self::Real(-x),
        }
    }
}

impl PartialEq for NumberBits {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Real(a), Self::Real(b)) => a == b,
            (Self::Int(a), Self::Real(b)) | (Self::Real(b), Self::Int(a)) => {
                b.is_finite() && b.fract() == 0.0 && (a as f64) == b
            }
        }
    }
}

/// Builtin class objects exposed as globals (`Array`, `Null`), matching Java `<class Name>` export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreludeClass {
    Array,
    Null,
}

impl PreludeClass {
    #[inline]
    #[must_use]
    pub const fn simple_name(self) -> &'static str {
        match self {
            Self::Array => "Array",
            Self::Null => "Null",
        }
    }

    /// Java / Leek Wars `AI.string` body (`<class Array>`, …) without outer Leek quotes.
    #[must_use]
    pub fn java_class_string(self) -> String {
        format!("<class {}>", self.simple_name())
    }

    #[must_use]
    pub fn from_u8(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Array),
            1 => Some(Self::Null),
            _ => None,
        }
    }

    #[must_use]
    pub const fn to_u8(self) -> u8 {
        match self {
            Self::Array => 0,
            Self::Null => 1,
        }
    }
}

/// A runtime value (intentionally small; extend for maps, arrays, closures, etc.).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(NumberBits),
    String(String),
    /// Prelude class (`Array`, `Null`): formatted via [`PreludeClass::java_class_string`], not constant-pool strings.
    Class(PreludeClass),
    /// Dense array (Java `ArrayLeekValue`–style indexing; equality is deep for this VM).
    Array(Vec<Value>),
    /// Insertion-ordered map literal (`[:]` / `[k: v, …]`); merge uses Java `putIfAbsent` rules.
    Map(Vec<(Value, Value)>),
    /// Object literal `{a: 1, …}` — Java export uses `{a: 1}` (not bracket-map syntax).
    Object(Vec<(Value, Value)>),
}

impl Value {
    #[inline]
    #[must_use]
    pub fn num_int(v: i64) -> Self {
        Self::Number(NumberBits::int(v))
    }

    #[inline]
    #[must_use]
    pub fn num_real(v: f64) -> Self {
        Self::Number(NumberBits::real(v))
    }

    /// Java `MapLeekValue.mapMerge`: clone `base`, then append entries from `other` whose keys are absent.
    #[must_use]
    pub fn map_merge_java(
        base: &[(Value, Value)],
        other: &[(Value, Value)],
    ) -> Vec<(Value, Value)> {
        let mut out = base.to_vec();
        for (k, v) in other {
            if !out.iter().any(|(bk, _)| bk == k) {
                out.push((k.clone(), v.clone()));
            }
        }
        out
    }

    /// V4 `==` / `equals_equals` for the VM value subset (matches `AI.equals_equals` for null, bool,
    /// number, string only: same Leek type and equal value).
    #[must_use]
    pub fn equals_equals_v4(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Class(a), Self::Class(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals_equals_v4(y))
            }
            (Self::Map(a), Self::Map(b)) | (Self::Object(a), Self::Object(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                a.iter().all(|(k, v)| {
                    b.iter()
                        .find(|(bk, _)| bk == k)
                        .is_some_and(|(_, bv)| v.equals_equals_v4(bv))
                })
            }
            _ => false,
        }
    }

    /// Numeric ordering helper aligned with `AI.less` / `AI.real` for null, bool, number, string.
    #[must_use]
    pub fn to_real_for_compare(&self) -> f64 {
        match self {
            Self::Null => 0.0,
            Self::Bool(b) => f64::from(u8::from(*b)),
            Self::Number(n) => n.as_f64(),
            Self::Array(a) => a.len() as f64,
            Self::Map(m) | Self::Object(m) => m.len() as f64,
            Self::Class(c) => {
                let s = c.java_class_string();
                if s == "true" {
                    return 1.0;
                }
                if s == "false" || s.is_empty() {
                    return 0.0;
                }
                s.parse::<f64>().unwrap_or_else(|_| s.len() as f64)
            }
            Self::String(s) => {
                if s == "true" {
                    return 1.0;
                }
                if s == "false" || s.is_empty() {
                    return 0.0;
                }
                s.parse::<f64>().unwrap_or_else(|_| s.len() as f64)
            }
        }
    }

    /// String form comparable to the Java snippet runner (`TopLevel` / `AI.string`) for tests.
    #[must_use]
    pub fn to_leek_export_string(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Number(nb) => match nb {
                NumberBits::Int(i) => format!("{i}"),
                NumberBits::Real(x) => format_java_double_export(*x),
            },
            Self::Array(a) => {
                let mut out = String::new();
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&v.to_leek_export_string());
                }
                out.push(']');
                out
            }
            Self::Map(m) => {
                if m.is_empty() {
                    return "[:]".into();
                }
                let mut out = String::from("[");
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&k.to_leek_export_string());
                    out.push_str(" : ");
                    out.push_str(&v.to_leek_export_string());
                }
                out.push(']');
                out
            }
            Self::Object(o) => format_object_brace_export(o, |v| v.to_leek_export_string()),
            Self::Class(c) => c.java_class_string(),
            Self::String(s) => {
                // Java `AI.string`: values that are exactly one JSON string token use a doubled-`"`
                // wrapper (`""hello""`, `""""` for empty) — see `TestJSON.java` / `jsonEncode`.
                if let Some(inner) = entire_json_string_body(s) {
                    let mut out = String::with_capacity(inner.len() + 4);
                    out.push('"');
                    out.push('"');
                    for ch in inner.chars() {
                        match ch {
                            '\\' => out.push_str("\\\\"),
                            '"' => out.push_str("\\\""),
                            '\n' => out.push_str("\\n"),
                            '\r' => out.push_str("\\r"),
                            '\t' => out.push_str("\\t"),
                            c if c <= '\u{1f}' => {
                                out.push_str(&format!("\\u{:04x}", c as u32));
                            }
                            c => out.push(c),
                        }
                    }
                    out.push('"');
                    out.push('"');
                    return out;
                }
                // `jsonEncode` of objects/arrays/maps yields JSON text; Java wraps it in one pair of
                // Leek string quotes without escaping interior `"` (see `TestJSON.java:41`,
                // `TestMap.java:244`…).
                // Leek `string([k : v, …])` / V4 `string({…})` bodies are not valid JSON but use the
                // same raw embed rule (see `TestString.java` v4 map/object cases).
                if super::json::decode(s).is_ok() || leek_export_raw_embed_composite_string(s) {
                    let mut out = String::with_capacity(s.len() + 2);
                    out.push('"');
                    out.push_str(s);
                    out.push('"');
                    return out;
                }
                let mut out = String::with_capacity(s.len() + 2);
                out.push('"');
                for ch in s.chars() {
                    match ch {
                        '\\' => out.push_str("\\\\"),
                        '"' => out.push_str("\\\""),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        _ => out.push(ch),
                    }
                }
                out.push('"');
                out
            }
        }
    }

    /// Estimated live footprint in **quads** (64-bit words), matching Leek Wars `AI` RAM accounting
    /// (`MAX_RAM` is in quads; `System.getUsedRAM` multiplies by 8 for bytes).
    #[must_use]
    pub fn ram_quads(&self) -> u64 {
        match self {
            Self::Null | Self::Bool(_) | Self::Class(_) => 1,
            Self::Number(_) => 1,
            Self::String(s) => 1u64.saturating_add(((s.len() as u64).saturating_add(7)) / 8),
            Self::Array(a) => 1u64.saturating_add(a.iter().map(Self::ram_quads).sum()),
            Self::Map(m) | Self::Object(m) => 1u64.saturating_add(
                m.iter()
                    .map(|(k, v)| k.ram_quads().saturating_add(v.ram_quads()))
                    .sum(),
            ),
        }
    }

    #[must_use]
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(n.as_f64()),
            _ => None,
        }
    }

    #[must_use]
    pub fn number_bits(&self) -> Option<NumberBits> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null | Self::Bool(false) => false,
            Self::Number(n) => match n {
                NumberBits::Int(i) => *i != 0,
                NumberBits::Real(x) => *x != 0.0,
            },
            _ => true,
        }
    }

    /// Java `string(x)` for `+` when either operand is a string (no extra quotes for numbers).
    #[must_use]
    pub fn to_leek_coerce_string(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Class(c) => c.java_class_string(),
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Number(n) => match n {
                NumberBits::Int(i) => format!("{i}"),
                NumberBits::Real(x) => format_java_double_export(*x),
            },
            Self::String(s) => s.clone(),
            Self::Array(a) => {
                let mut out = String::new();
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&v.to_leek_coerce_string());
                }
                out.push(']');
                out
            }
            Self::Map(m) => {
                if m.is_empty() {
                    return "[:]".into();
                }
                let mut out = String::from("[");
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&k.to_leek_coerce_string());
                    out.push_str(" : ");
                    out.push_str(&v.to_leek_coerce_string());
                }
                out.push(']');
                out
            }
            Self::Object(o) => format_object_brace_export(o, |v| v.to_leek_coerce_string()),
        }
    }

    /// Java `string(x)` for **V4**: composite values embed string elements as JSON string literals
    /// (`["a", "b"]`), matching `TestString.java` v4 export expectations.
    #[must_use]
    pub fn to_java_string_builtin_v4(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Class(c) => c.java_class_string(),
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Number(n) => match n {
                NumberBits::Int(i) => format!("{i}"),
                NumberBits::Real(x) => format_java_double_export(*x),
            },
            Self::String(s) => java_string_builtin_v4_json_string(s),
            Self::Array(a) => {
                let mut out = String::new();
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&v.to_java_string_builtin_v4());
                }
                out.push(']');
                out
            }
            Self::Map(m) => {
                if m.is_empty() {
                    return "[:]".into();
                }
                let mut out = String::from("[");
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&k.to_java_string_builtin_v4());
                    out.push_str(" : ");
                    out.push_str(&v.to_java_string_builtin_v4());
                }
                out.push(']');
                out
            }
            Self::Object(o) => format_object_brace_export(o, |v| v.to_java_string_builtin_v4()),
        }
    }
}

/// True when Java `AI.string` export embeds `s` inside one pair of Leek quotes without escaping
/// interior `"` (same branch as valid JSON text).
fn leek_export_raw_embed_composite_string(s: &str) -> bool {
    let t = s.trim();
    if t.starts_with('[') && t.ends_with(']') && t.contains(" : ") {
        return true;
    }
    if t.starts_with('{') && t.ends_with('}') && t.contains(": \"") {
        return true;
    }
    false
}

fn java_string_builtin_v4_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len().saturating_add(2));
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c <= '\u{1f}' => {
                use std::fmt::Write;
                let _ = write!(&mut out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn is_leek_ident(s: &str) -> bool {
    let mut it = s.chars();
    let Some(f) = it.next() else {
        return false;
    };
    if !(f.is_ascii_alphabetic() || f == '_') {
        return false;
    }
    it.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn object_field_key_export(k: &Value) -> String {
    match k {
        Value::String(s) if is_leek_ident(s) => s.clone(),
        _ => k.to_leek_export_string(),
    }
}

fn format_object_brace_export(o: &[(Value, Value)], fmt_val: impl Fn(&Value) -> String) -> String {
    if o.is_empty() {
        return "{}".into();
    }
    let mut out = String::from("{");
    for (i, (k, v)) in o.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&object_field_key_export(k));
        out.push_str(": ");
        out.push_str(&fmt_val(v));
    }
    out.push('}');
    out
}

/// Java `AI.string` / `Double.toString` style for values that are reals in LeekScript.
/// `s` is a single JSON string literal (including delimiters); returns decoded UTF-8 body.
fn entire_json_string_body(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if bytes.get(i)? != &b'"' {
        return None;
    }
    i += 1;
    let mut out = String::new();
    loop {
        let c = *bytes.get(i)?;
        i += 1;
        match c {
            b'"' => break,
            b'\\' => {
                let e = *bytes.get(i)?;
                i += 1;
                match e {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let slice = bytes.get(i..i.checked_add(4)?)?;
                        let hex = std::str::from_utf8(slice).ok()?;
                        i += 4;
                        let cp = u32::from_str_radix(hex, 16).ok()?;
                        out.push(char::from_u32(cp)?);
                    }
                    _ => return None,
                }
            }
            x if x < 0x20 => return None,
            x if x < 0x80 => out.push(char::from(x)),
            _ => return None,
        }
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i != bytes.len() {
        return None;
    }
    Some(out)
}

fn format_java_double_export(n: f64) -> String {
    if n.is_nan() {
        return "nan".into();
    }
    if n.is_infinite() {
        return if n.is_sign_positive() {
            "\u{221e}".into()
        } else {
            "-\u{221e}".into()
        };
    }
    let r = n.round();
    if (n - r).abs() < 1e-9 && r.is_finite() && r >= (i64::MIN as f64) && r <= (i64::MAX as f64) {
        return format!("{}.0", r as i64);
    }
    format!("{n}")
}
