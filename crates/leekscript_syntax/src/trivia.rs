//! Split source gaps between lexer tokens into trivia + whitespace runs.

use crate::kind::LeekSyntaxKind;

/// Split `gap` (text strictly between two lexer token spans) into rowan leaf pieces.
pub fn split_trivia(gap: &str) -> Vec<(LeekSyntaxKind, &str)> {
    if gap.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    let b = gap.as_bytes();
    while i < gap.len() {
        if i + 1 < gap.len() && b[i] == b'/' && b[i + 1] == b'/' {
            let start = i;
            i += 2;
            while i < gap.len() && b[i] != b'\n' {
                i += 1;
            }
            out.push((LeekSyntaxKind::LineComment, &gap[start..i]));
            continue;
        }
        if i + 1 < gap.len() && b[i] == b'/' && b[i + 1] == b'*' {
            let start = i;
            i += 2;
            let mut closed = false;
            while i + 1 < gap.len() {
                if b[i] == b'*' && b[i + 1] == b'/' {
                    i += 2;
                    closed = true;
                    break;
                }
                i += 1;
            }
            if !closed {
                i = gap.len();
            }
            out.push((LeekSyntaxKind::BlockComment, &gap[start..i]));
            continue;
        }
        let ch = gap[i..].chars().next().unwrap();
        if ch.is_whitespace() {
            let start = i;
            i += ch.len_utf8();
            while i < gap.len() {
                let c = gap[i..].chars().next().unwrap();
                if !c.is_whitespace() {
                    break;
                }
                i += c.len_utf8();
            }
            out.push((LeekSyntaxKind::Whitespace, &gap[start..i]));
            continue;
        }
        // Unexpected byte in gap (shouldn't happen between valid tokens) — keep as whitespace bucket.
        let start = i;
        i += ch.len_utf8();
        out.push((LeekSyntaxKind::Whitespace, &gap[start..i]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_line_comment_and_ws() {
        let gap = "  // hi\n  ";
        let v = split_trivia(gap);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].0, LeekSyntaxKind::Whitespace);
        assert_eq!(v[1].0, LeekSyntaxKind::LineComment);
        assert_eq!(v[1].1, "// hi");
        assert_eq!(v[2].0, LeekSyntaxKind::Whitespace);
    }
}
