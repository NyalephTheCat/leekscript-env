//! Walk the CST, resolve typed nodes at a byte offset, and combine with [`crate::document::LeekDoc`]
//! transforms.
//!
//! ## Visitor (pre/post order)
//!
//! With the `walk` feature (on by default), use [`Visitor`] and [`WalkOptions`] with
//! [`SyntaxNode::walk`](sipha::tree::red::SyntaxNode::walk) on [`LeekDoc::root_syntax`](crate::document::LeekDoc::root_syntax)
//! or call [`LeekDoc::walk`](crate::document::LeekDoc::walk).
//!
//! ## Typed node at offset
//!
//! [`typed_at_offset`] starts from the deepest [`SyntaxNode`] covering `offset`, then walks toward
//! the root until [`AstNode::cast`] succeeds for your wrapper type
//! (for example [`crate::ast::VarDecl`]).
//!
//! ## Rewriting source vs green-tree transforms
//!
//! - **Text splice + reparse**: [`LeekDoc::replace_span`](crate::document::LeekDoc::replace_span)
//!   replaces a [`Span`](crate::Span) in the buffer; with the `partial-reparse` feature (default),
//!   it tries a statement-level partial reparse first, then falls back to a full parse when needed.
//! - **Structured replace**: implement sipha’s [`Transformer`](sipha::tree::transform::Transformer)
//!   and [`LeekDoc::apply_transform`](crate::document::LeekDoc::apply_transform) (requires the
//!   `transform` feature), or build a new tree and [`LeekDoc::set_syntax_root`](crate::document::LeekDoc::set_syntax_root).

use sipha::tree::ast::AstNode;
use sipha::tree::red::SyntaxNode;
use sipha::types::Pos;

#[cfg(feature = "walk")]
pub use sipha::tree::walk::{Visitor, WalkOptions, WalkResult, walk};

/// Re-export for bounds on [`typed_at_offset`] without importing sipha separately.
pub use sipha::tree::ast::AstNode as AstNodeTrait;

/// Re-export for typed literal / token wrappers (e.g. [`crate::ast::LitStr`]).
pub use sipha::tree::ast::AstToken as AstTokenTrait;

/// Re-export for [`AstNodeExt::children`] and related helpers on [`SyntaxNode`].
pub use sipha::tree::ast::AstNodeExt;

/// Re-export for [`AstTokenExt::token_ast`] on [`SyntaxNode`].
pub use sipha::tree::ast::AstTokenExt;

/// Deepest node covering `offset`, then ancestors toward `root` until `N::cast` succeeds.
#[must_use]
pub fn typed_at_offset<N: AstNode>(root: &SyntaxNode, offset: Pos) -> Option<N> {
    let node = root.node_at_offset(offset)?;
    if let Some(n) = N::cast(node.clone()) {
        return Some(n);
    }
    for anc in node.ancestors(root) {
        if let Some(n) = N::cast(anc.clone()) {
            return Some(n);
        }
    }
    None
}
