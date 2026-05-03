//! Typed API over the rowan layer: [`SourceFile`], trivia layout, and (later) grammar nodes.

mod nodes;
mod segment;

pub use nodes::AstNode;
pub use segment::{FileSegments, TriviaPiece};

use crate::kind::LeekSyntaxKind;
use crate::language::LeekLanguage;
use rowan::{NodeOrToken, SyntaxNode, SyntaxToken};

/// Root of a parsed `.leek` file (wraps a [`SOURCE_FILE`](LeekSyntaxKind::SourceFile) node).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SourceFile {
    inner: SyntaxNode<LeekLanguage>,
}

impl SourceFile {
    #[must_use]
    pub fn cast(node: SyntaxNode<LeekLanguage>) -> Option<Self> {
        (node.kind() == LeekSyntaxKind::SourceFile).then(|| Self { inner: node })
    }

    #[must_use]
    pub fn syntax(&self) -> &SyntaxNode<LeekLanguage> {
        &self.inner
    }

    /// Full source text represented by this tree (lossless for token+trivia trees).
    #[must_use]
    pub fn text(&self) -> rowan::SyntaxText {
        self.inner.text()
    }

    /// Direct children: trivia and lexical tokens (until nested grammar nodes exist).
    #[must_use]
    pub fn children_with_tokens(&self) -> rowan::SyntaxElementChildren<LeekLanguage> {
        self.inner.children_with_tokens()
    }

    /// Split the flat `SOURCE_FILE` into typed trivia pieces and lexical [`SyntaxToken`]s.
    ///
    /// Returns `None` if the tree contains nested nodes (future grammar) or malformed structure.
    #[must_use]
    pub fn file_segments(&self) -> Option<FileSegments> {
        if self.inner.kind() != LeekSyntaxKind::SourceFile {
            return None;
        }

        let mut prefix = Vec::new();
        let mut between: Vec<Vec<TriviaPiece>> = Vec::new();
        let mut lexicals: Vec<SyntaxToken<LeekLanguage>> = Vec::new();
        let mut current: Vec<TriviaPiece> = Vec::new();
        let mut seen_lex: u32 = 0;

        for el in self.inner.children_with_tokens() {
            match el {
                NodeOrToken::Token(t) => {
                    let k = t.kind();
                    if k.is_trivia() {
                        current.push(TriviaPiece::cast(t)?);
                    } else {
                        if seen_lex == 0 {
                            prefix = std::mem::take(&mut current);
                        } else {
                            between.push(std::mem::take(&mut current));
                        }
                        lexicals.push(t);
                        seen_lex += 1;
                    }
                }
                NodeOrToken::Node(_) => return None,
            }
        }

        if seen_lex == 0 {
            return Some(FileSegments {
                prefix: current,
                between: Vec::new(),
                lexicals: Vec::new(),
                suffix: Vec::new(),
            });
        }

        Some(FileSegments {
            prefix,
            between,
            lexicals,
            suffix: current,
        })
    }
}

impl AstNode for SourceFile {
    fn syntax(&self) -> &SyntaxNode<LeekLanguage> {
        &self.inner
    }
}

impl std::fmt::Debug for SourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceFile")
            .field("text", &self.text().to_string())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_source_file_tree;
    use leekscript_lexer::{Lexer, LexerConfig};

    #[test]
    fn segments_match_string_gaps() {
        let src = "// a\nvar  x=1;\n";
        let (tokens, errs) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(errs.is_empty());
        let root = build_source_file_tree(src, &tokens);
        let sf = SourceFile::cast(root).unwrap();
        let seg = sf.file_segments().unwrap();
        let (p, bet, s) = crate::gaps_from_source_file(sf.syntax()).unwrap();
        assert_eq!(FileSegments::join_pieces(&seg.prefix), p);
        assert_eq!(seg.between.len(), bet.len());
        for (i, g) in bet.iter().enumerate() {
            assert_eq!(FileSegments::join_pieces(&seg.between[i]), *g);
        }
        assert_eq!(FileSegments::join_pieces(&seg.suffix), s);
    }
}
