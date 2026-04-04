//! Recognize lexer tokens that the grammar accepts as binding names via the `name` rule
//! (`ident` plus selected keywords such as type names and `function`).

use crate::syntax::kinds::K;
use sipha::tree::red::{SyntaxNode, SyntaxToken};

#[must_use]
pub(crate) fn is_lexical_binding_name(k: K) -> bool {
    matches!(
        k,
        K::Ident
            | K::StringTypeKw
            | K::IntegerKw
            | K::RealKw
            | K::BooleanKw
            | K::AnyKw
            | K::VoidKw
            | K::DefaultKw
            | K::IncludeKw
            | K::FunctionKw
    )
}

/// Binding token in [`K::FnParam`](crate::syntax::kinds::K) (`T name` or bare `name`).
#[must_use]
pub(crate) fn fn_param_binding_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.child_tokens()
        .find(|t| t.kind_as::<K>().is_some_and(is_lexical_binding_name))
}

/// Name token after the leading `function` keyword in [`K::FunctionDecl`].
#[must_use]
pub(crate) fn function_decl_name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    let mut after_decl_kw = false;
    for t in node.child_tokens() {
        let Some(k) = t.kind_as::<K>() else {
            continue;
        };
        if !after_decl_kw {
            if k == K::FunctionKw {
                after_decl_kw = true;
            }
            continue;
        }
        if is_lexical_binding_name(k) {
            return Some(t);
        }
    }
    None
}
