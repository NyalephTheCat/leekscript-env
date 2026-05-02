//! Byte offsets into UTF-8 source text (must stay on `char` boundaries).
#![warn(clippy::pedantic)]

use std::ops::Range;

/// Inclusive start, exclusive end, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// # Panics
    ///
    /// If `range.start` or `range.end` is greater than `u32::MAX`.
    #[must_use]
    pub fn new(range: Range<usize>) -> Self {
        let start = u32::try_from(range.start).expect("span start must fit in u32");
        let end = u32::try_from(range.end).expect("span end must fit in u32");
        Self { start, end }
    }

    /// # Panics
    ///
    /// If `at` is greater than `u32::MAX`.
    #[must_use]
    pub fn point(at: usize) -> Self {
        let u = u32::try_from(at).expect("span offset must fit in u32");
        Self { start: u, end: u }
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// # Panics
    ///
    /// If `self.end - self.start` does not fit in `usize` (the difference is always ≤ `u32::MAX`).
    #[must_use]
    pub fn len(self) -> usize {
        usize::try_from(self.end - self.start).expect("span length must fit in usize")
    }
}

/// 1-based line, 1-based column (character offset within the line, UTF-8 safe via byte scan).
///
/// # Panics
///
/// If the character count on the current line exceeds `u32::MAX`.
#[must_use]
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
    let count = source[line_start..byte_offset].chars().count();
    let col = u32::try_from(count)
        .expect("column char count must fit in u32")
        .saturating_add(1);
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
