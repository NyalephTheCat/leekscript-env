use sipha::tree::red::SyntaxToken;

/// Quoted string literal (`"..."` or `'...'`) as a lexer token.
#[derive(Debug, Clone)]
pub struct LitStr(SyntaxToken);

impl LitStr {
    pub(crate) const fn new(token: SyntaxToken) -> Self {
        Self(token)
    }

    pub fn raw_text(&self) -> &str {
        self.0.text()
    }

    /// Content without surrounding quotes (no escape processing).
    pub fn value(&self) -> String {
        let t = self.raw_text();
        let b = t.as_bytes();
        if b.len() >= 2 {
            let q = b[0];
            if (q == b'"' || q == b'\'') && b[b.len() - 1] == q {
                return t[1..t.len() - 1].to_string();
            }
        }
        t.to_string()
    }
}
