//! Typed trivia + lexical layout of a flat [`super::SourceFile`](super::SourceFile).

use crate::kind::LeekSyntaxKind;
use crate::language::LeekLanguage;
use rowan::SyntaxToken;

/// One trivia leaf from the rowan tree (whitespace or comment).
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum TriviaPiece {
    Whitespace(SyntaxToken<LeekLanguage>),
    LineComment(SyntaxToken<LeekLanguage>),
    BlockComment(SyntaxToken<LeekLanguage>),
}

impl std::fmt::Debug for TriviaPiece {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TriviaPiece::Whitespace(t) => f.debug_tuple("Whitespace").field(&t.text()).finish(),
            TriviaPiece::LineComment(t) => f.debug_tuple("LineComment").field(&t.text()).finish(),
            TriviaPiece::BlockComment(t) => f.debug_tuple("BlockComment").field(&t.text()).finish(),
        }
    }
}

impl TriviaPiece {
    pub fn cast(token: SyntaxToken<LeekLanguage>) -> Option<Self> {
        match token.kind() {
            LeekSyntaxKind::Whitespace => Some(Self::Whitespace(token)),
            LeekSyntaxKind::LineComment => Some(Self::LineComment(token)),
            LeekSyntaxKind::BlockComment => Some(Self::BlockComment(token)),
            _ => None,
        }
    }

    pub fn is_comment(&self) -> bool {
        matches!(
            self,
            TriviaPiece::LineComment(_) | TriviaPiece::BlockComment(_)
        )
    }

    pub fn as_syntax_token(&self) -> &SyntaxToken<LeekLanguage> {
        match self {
            TriviaPiece::Whitespace(t)
            | TriviaPiece::LineComment(t)
            | TriviaPiece::BlockComment(t) => t,
        }
    }

    /// Source slice for this trivia piece.
    pub fn text(&self) -> &str {
        self.as_syntax_token().text()
    }
}

/// Flat view of a [`super::SourceFile`](super::SourceFile): leading trivia, lexical tokens, gaps, trailing trivia.
///
/// Invariant (when there is at least one lexical token): `between.len() + 1 == lexicals.len()`.
/// When there are no lexical tokens, `lexicals` and `between` are empty and all trivia is in `prefix`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSegments {
    pub prefix: Vec<TriviaPiece>,
    /// Trivia after `lexicals[i]` and before `lexicals[i + 1]`.
    pub between: Vec<Vec<TriviaPiece>>,
    pub lexicals: Vec<SyntaxToken<LeekLanguage>>,
    pub suffix: Vec<TriviaPiece>,
}

impl FileSegments {
    /// Concatenate trivia for compatibility with string-based gap helpers.
    pub fn join_pieces(pieces: &[TriviaPiece]) -> String {
        pieces.iter().map(|p| p.text().to_string()).collect()
    }

    /// Join pieces with `\r\n` normalized inside comment bodies only (whitespace unchanged).
    pub fn join_pieces_normalize_comments(pieces: &[TriviaPiece]) -> String {
        let mut s = String::new();
        for p in pieces {
            match p {
                TriviaPiece::Whitespace(w) => s.push_str(w.text()),
                TriviaPiece::LineComment(c) | TriviaPiece::BlockComment(c) => {
                    s.push_str(&c.text().replace("\r\n", "\n"));
                }
            }
        }
        s
    }
}
