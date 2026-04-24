//! Byte offsets into UTF-8 source text (must stay on `char` boundaries).

use std::ops::Range;

/// Inclusive start, exclusive end, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(range: Range<usize>) -> Self {
        Self {
            start: range.start as u32,
            end: range.end as u32,
        }
    }

    pub fn point(at: usize) -> Self {
        let u = at as u32;
        Self { start: u, end: u }
    }

    pub fn len(self) -> usize {
        (self.end - self.start) as usize
    }
}

/// 1-based line, 1-based column (character offset within the line, UTF-8 safe via byte scan).
pub fn line_col_at(source: &str, byte_offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut line_start = 0usize;
    for (i, ch) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = i + ch.len_utf8();
        }
    }
    let col = source[line_start..byte_offset].chars().count() as u32 + 1;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_col_second_line() {
        let s = "a\nbc\n";
        assert_eq!(line_col_at(s, 0), (1, 1));
        assert_eq!(line_col_at(s, 2), (2, 1));
        assert_eq!(line_col_at(s, 3), (2, 2));
    }
}
