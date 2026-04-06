//! Minimal JSON encode/decode for VM values (Java `jsonEncode` / `jsonDecode` parity subset).

use super::value::{NumberBits, Value};

/// JSON text for a value (compact: no extra spaces).
#[must_use]
pub fn encode(v: &Value) -> String {
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
        Value::NativeFunction { .. } => "null".into(),
        Value::Set(s) => {
            let mut out = String::from('[');
            for (i, v) in s.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&encode(v));
            }
            out.push(']');
            out
        }
        Value::Array(a) => {
            let mut out = String::from("[");
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&encode(x));
            }
            out.push(']');
            out
        }
        Value::Map(m) | Value::Object(m) => {
            let mut out = String::from("{");
            // Java jsonEncode deterministically orders object keys (lexicographic by key string).
            // This matters for parity tests and avoids hash/insertion-order differences.
            let mut entries: Vec<(&Value, &Value, String)> = m
                .iter()
                .map(|(k, v)| (k, v, sort_key_string(k)))
                .collect();
            entries.sort_by(|a, b| a.2.cmp(&b.2));
            for (i, (k, v, _)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&encode_key(k));
                out.push(':');
                out.push_str(&encode(v));
            }
            out.push('}');
            out
        }
    }
}

fn sort_key_string(k: &Value) -> String {
    match k {
        Value::String(s) => s.to_string(),
        _ => encode(k),
    }
}

fn encode_key(k: &Value) -> String {
    match k {
        Value::String(s) => encode_string(s),
        _ => encode_string(&encode(k)),
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
            return Ok(Value::Array(items));
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
        Ok(Value::Array(items))
    }

    fn parse_object(&mut self) -> Result<Value, ()> {
        self.bump().filter(|&b| b == b'{').ok_or(())?;
        self.skip_ws();
        let mut pairs: Vec<(Value, Value)> = Vec::new();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Object(pairs));
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
        Ok(Value::Object(pairs))
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
