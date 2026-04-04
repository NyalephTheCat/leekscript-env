//! Values carried on the VM stack (minimal set for the first execution tier).

use std::string::String;
use std::vec::Vec;

/// A runtime value (intentionally small; extend for maps, arrays, closures, etc.).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// IEEE double; LeekScript `integer` is still represented here for the MVP VM.
    Number(f64),
    String(String),
    /// Dense array (Java `ArrayLeekValue`–style indexing; equality is deep for this VM).
    Array(Vec<Value>),
}

impl Value {
    /// V4 `==` / `equals_equals` for the VM value subset (matches `AI.equals_equals` for null, bool,
    /// number, string only: same Leek type and equal value).
    #[must_use]
    pub fn equals_equals_v4(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals_equals_v4(y))
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
            Self::Number(n) => *n,
            Self::Array(a) => a.len() as f64,
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

    /// String form comparable to the Java snippet runner (`TopLevel` / `AI.string` style) for tests.
    #[must_use]
    pub fn to_leek_export_string(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Number(n) => format_leek_number(*n),
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
            Self::String(s) => {
                let mut out = String::with_capacity(s.len() + 2);
                out.push('\'');
                for ch in s.chars() {
                    match ch {
                        '\\' => out.push_str("\\\\"),
                        '\'' => out.push_str("\\'"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        _ => out.push(ch),
                    }
                }
                out.push('\'');
                out
            }
        }
    }

    /// Estimated live footprint in **quads** (64-bit words), matching Leek Wars `AI` RAM accounting
    /// (`MAX_RAM` is in quads; `System.getUsedRAM` multiplies by 8 for bytes).
    #[must_use]
    pub fn ram_quads(&self) -> u64 {
        match self {
            Self::Null | Self::Bool(_) => 1,
            Self::Number(_) => 1,
            Self::String(s) => 1u64.saturating_add(((s.len() as u64).saturating_add(7)) / 8),
            Self::Array(a) => 1u64.saturating_add(a.iter().map(Self::ram_quads).sum()),
        }
    }

    #[must_use]
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }

    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null | Self::Bool(false) => false,
            Self::Number(n) => *n != 0.0,
            _ => true,
        }
    }

    /// Java `string(x)` for `+` when either operand is a string (no extra quotes for numbers).
    #[must_use]
    pub fn to_leek_coerce_string(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Number(n) => format_leek_number(*n),
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
        }
    }
}

fn format_leek_number(n: f64) -> String {
    if n.is_nan() {
        return "nan".into();
    }
    let r = n.round();
    if (n - r).abs() < 1e-9 && r.is_finite() {
        if r >= (i64::MIN as f64) && r <= (i64::MAX as f64) {
            return format!("{}", r as i64);
        }
    }
    format!("{n}")
}
