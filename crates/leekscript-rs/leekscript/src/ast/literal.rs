use crate::syntax::kinds::K;
use sipha::tree::ast::AstToken;
use sipha::tree::red::SyntaxToken;
use sipha::types::{IntoSyntaxKind, SyntaxKind};

/// Quoted string literal (`"..."` or `'...'`) as a lexer token.
///
/// Implements [`AstToken`] (not [`sipha::tree::ast::AstNode`]): literals are leaf tokens, not
/// [`SyntaxNode`](sipha::tree::red::SyntaxNode)s. Use [`sipha::tree::ast::AstTokenExt::token_ast`] on a
/// parent node to find the first matching string token.
#[derive(Debug, Clone)]
pub struct LitStr(SyntaxToken);

impl AstToken for LitStr {
    #[inline]
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == K::String.into_syntax_kind()
    }

    #[inline]
    fn cast(token: SyntaxToken) -> Option<Self> {
        Self::can_cast(token.kind()).then(|| Self(token))
    }

    #[inline]
    fn syntax(&self) -> &SyntaxToken {
        &self.0
    }
}

impl LitStr {
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
