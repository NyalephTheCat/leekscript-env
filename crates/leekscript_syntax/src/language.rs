//! Rowan [`Language`] marker for `LeekScript`.

use crate::kind::LeekSyntaxKind;

/// Marker type: [`rowan::SyntaxNode`] / [`rowan::SyntaxToken`] use this `Language`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LeekLanguage;

impl rowan::Language for LeekLanguage {
    type Kind = LeekSyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        LeekSyntaxKind::from_raw(raw.0)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}
