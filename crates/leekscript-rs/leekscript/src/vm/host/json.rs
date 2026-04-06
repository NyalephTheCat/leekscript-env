//! Minimal JSON encode/decode for VM values (Java `jsonEncode` / `jsonDecode` parity subset).

use crate::vm::value::{NumberBits, Value};
use std::collections::{BTreeMap, HashSet};

/// JSON text for a value (compact: no extra spaces).
#[must_use]
pub fn encode(v: &Value) -> String {
    let mut visited: HashSet<usize> = HashSet::new();
    encode_with_visited(v, &mut visited)
}

fn value_visit_id(v: &Value) -> Option<usize> {
    match v {
        Value::Array(a) => Some(std::rc::Rc::as_ptr(a) as usize),
        Value::Map(m) => Some(std::rc::Rc::as_ptr(m) as usize),
        Value::Object(o) => Some(std::rc::Rc::as_ptr(o) as usize),
        Value::Instance { fields, .. } => Some(std::rc::Rc::as_ptr(fields) as usize),
        _ => None,
    }
}

fn encode_with_visited(v: &Value, visited: &mut HashSet<usize>) -> String {
    // Java `toJSON` skips revisiting composite values (arrays/maps/objects).
    if let Some(id) = value_visit_id(v) {
        if visited.contains(&id) {
            // For JSON, cycles/repeats become "null" / omitted (omission is handled by callers).
            return "null".into();
        }
        visited.insert(id);
    }

    match v {
        Value::Null => "null".into(),
        Value::Bool(true) => "true".into(),
        Value::Bool(false) => "false".into(),
        Value::Number(nb) => match nb {
            NumberBits::Int(i) => format!("{i}"),
            NumberBits::Real(x) => encode_number_float(*x),
        },
        Value::Class(_) => "null".into(),
        Value::String(s) => encode_string(s),
        Value::Interval(_) => "null".into(),
        Value::Function { .. } => "null".into(),
        Value::Closure { .. } => "null".into(),
        Value::NativeFunction { .. } => "null".into(),
        Value::Set(s) => {
            let mut out = String::from('[');
            for (i, v) in s.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let enc = encode_with_visited(v, visited);
                out.push_str(&enc);
            }
            out.push(']');
            out
        }
        Value::Array(a) => {
            let a = a.borrow();
            let mut out = String::from("[");
            let mut first = true;
            for x in a.iter() {
                // Skip already-visited composite values (Java behavior).
                if let Some(id) = value_visit_id(x) {
                    if visited.contains(&id) {
                        continue;
                    }
                }
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&encode_with_visited(x, visited));
            }
            out.push(']');
            out
        }
        Value::Map(m) | Value::Object(m) => {
            let m = m.borrow();
            let mut out = String::from("{");
            // Java builds a TreeMap<String, Object> of keys, so keys are sorted lexicographically
            // and duplicate stringified keys overwrite earlier ones.
            let mut sorted: BTreeMap<String, (&Value, &Value)> = BTreeMap::new();
            for (k, v) in m.iter() {
                sorted.insert(json_key_string(k), (k, v));
            }
            let mut first = true;
            for (string_key, (_orig_key, v)) in sorted.into_iter() {
                // Skip already-visited composite values (Java behavior: omit field).
                if let Some(id) = value_visit_id(v) {
                    if visited.contains(&id) {
                        continue;
                    }
                }
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&encode_string(&string_key));
                out.push(':');
                out.push_str(&encode_with_visited(v, visited));
            }
            out.push('}');
            out
        }
        Value::Instance { .. } => "null".into(),
    }
}

fn json_key_string(k: &Value) -> String {
    // Matches Java `ai.string(key)` used by `MapLeekValue.toJSON`:
    // - strings are returned as-is (no quotes)
    // - numbers/bools/null are stringified without quotes
    // - composite values use their export string (cycle-safe)
    match k {
        Value::String(s) => s.to_string(),
        Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::Class(_)
        | Value::Function { .. }
        | Value::Closure { .. }
        | Value::NativeFunction { .. }
        | Value::Interval(_)
        | Value::Set(_) => k.to_leek_coerce_string(),
        _ => k.to_leek_export_string(),
    }
}

fn encode_number_float(n: f64) -> String {
    if n.is_nan() {
        return "null".into();
    }
    if !n.is_finite() {
        return "null".into();
    }
    let r = n.round();
    if (n - r).abs() < 1e-9 && r.is_finite() && r >= (i64::MIN as f64) && r <= (i64::MAX as f64) {
        return format!("{}", r as i64);
    }
    format!("{n}")
}

fn encode_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
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
    out
}

/// Parse JSON into a [`Value`] (subset used by LeekScript tests).
pub fn decode(s: &str) -> Result<Value, ()> {
    let mut p = Parser {
        bytes: s.as_bytes(),
        i: 0,
    };
    let v = p.parse_value()?;
    p.skip_ws();
    if p.i != p.bytes.len() {
        return Err(());
    }
    Ok(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.i < self.bytes.len() && self.bytes[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.i += 1;
        Some(b)
    }

    fn parse_value(&mut self) -> Result<Value, ()> {
        self.skip_ws();
        match self.peek().ok_or(())? {
            b'n' => self.parse_null(),
            b't' | b'f' => self.parse_bool(),
            b'"' => Ok(Value::String(self.parse_string_inner()?)),
            b'[' => self.parse_array(),
            b'{' => self.parse_object(),
            b'-' | b'0'..=b'9' => self.parse_number(),
            _ => Err(()),
        }
    }

    fn parse_null(&mut self) -> Result<Value, ()> {
        if self.bytes[self.i..].starts_with(b"null") {
            self.i += 4;
            Ok(Value::Null)
        } else {
            Err(())
        }
    }

    fn parse_bool(&mut self) -> Result<Value, ()> {
        if self.bytes[self.i..].starts_with(b"true") {
            self.i += 4;
            return Ok(Value::Bool(true));
        }
        if self.bytes[self.i..].starts_with(b"false") {
            self.i += 5;
            return Ok(Value::Bool(false));
        }
        Err(())
    }

    fn parse_string_inner(&mut self) -> Result<String, ()> {
        self.bump().filter(|&b| b == b'"').ok_or(())?;
        let mut out = String::new();
        loop {
            let c = self.bump().ok_or(())?;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.bump().ok_or(())?;
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
                            let hex: String = (0..4)
                                .filter_map(|_| self.bump())
                                .map(|b| b as char)
                                .collect();
                            if hex.len() != 4 {
                                return Err(());
                            }
                            let cp = u32::from_str_radix(&hex, 16).map_err(|_| ())?;
                            let ch = char::from_u32(cp).ok_or(())?;
                            out.push(ch);
                        }
                        _ => return Err(()),
                    }
                }
                x if x < 0x20 => return Err(()),
                x => out.push(char::from(x)),
            }
        }
        Ok(out)
    }

    fn parse_array(&mut self) -> Result<Value, ()> {
        self.bump().filter(|&b| b == b'[').ok_or(())?;
        self.skip_ws();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(items))));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.bump().ok_or(())? {
                b',' => continue,
                b']' => break,
                _ => return Err(()),
            }
        }
        Ok(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(items))))
    }

    fn parse_object(&mut self) -> Result<Value, ()> {
        self.bump().filter(|&b| b == b'{').ok_or(())?;
        self.skip_ws();
        let mut pairs: Vec<(Value, Value)> = Vec::new();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Object(std::rc::Rc::new(std::cell::RefCell::new(pairs))));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string_inner()?;
            self.skip_ws();
            if self.bump().ok_or(())? != b':' {
                return Err(());
            }
            let val = self.parse_value()?;
            pairs.push((Value::String(key), val));
            self.skip_ws();
            match self.bump().ok_or(())? {
                b',' => continue,
                b'}' => break,
                _ => return Err(()),
            }
        }
        Ok(Value::Object(std::rc::Rc::new(std::cell::RefCell::new(pairs))))
    }

    fn parse_number(&mut self) -> Result<Value, ()> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        let slice = std::str::from_utf8(&self.bytes[start..self.i]).map_err(|_| ())?;
        let n: f64 = slice.parse().map_err(|_| ())?;
        let export_real = slice.contains('.') || slice.contains('e') || slice.contains('E');
        let nb = NumberBits::from_literal(export_real, n);
        Ok(Value::Number(nb))
    }
}
