//! Recognize lexer tokens that the grammar accepts as binding names via the `name` rule
//! (`ident` plus selected keywords such as type names and `function`).

use crate::syntax::kinds::Lex;
use sipha::tree::red::{SyntaxNode, SyntaxToken};

#[must_use]
pub(crate) fn is_lexical_binding_name(k: Lex) -> bool {
    matches!(
        k,
        Lex::Ident
            | Lex::StringTypeKw
            | Lex::IntegerKw
            | Lex::RealKw
            | Lex::BooleanKw
            | Lex::AnyKw
            | Lex::VoidKw
            | Lex::DefaultKw
            | Lex::IncludeKw
            | Lex::FunctionKw
    )
}

/// Binding token in [`Node::FnParam`](crate::syntax::kinds::Node) (`T name` or bare `name`).
#[must_use]
pub(crate) fn fn_param_binding_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.child_tokens()
        .find(|t| t.kind_as::<Lex>().is_some_and(is_lexical_binding_name))
}

/// Name token after the leading `function` keyword in [`Node::FunctionDecl`].
#[must_use]
pub(crate) fn function_decl_name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    let mut after_decl_kw = false;
    for t in node.child_tokens() {
        let Some(k) = t.kind_as::<Lex>() else {
            continue;
        };
        if !after_decl_kw {
            if k == Lex::FunctionKw {
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
