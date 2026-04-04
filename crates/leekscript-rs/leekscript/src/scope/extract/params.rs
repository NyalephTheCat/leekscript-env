use crate::Span;
use crate::ast::FunctionDecl;
use crate::ast::fn_param_children;
use crate::ast::types::TypeExpr;
use sipha::tree::ast::AstNode;
use sipha::tree::red::SyntaxNode;

/// Formal parameters for any node that has direct [`FnParam`](crate::ast::FnParam) children
/// (`FunctionDecl`, [`AnonFunctionExpr`](crate::ast::AnonFunctionExpr), …).
#[must_use]
pub fn extract_fn_params_from_syntax(node: &SyntaxNode) -> Vec<(Option<TypeExpr>, String, Span)> {
    fn_param_children(node)
        .filter_map(|p| {
            let name = p.name()?;
            let sp = p.name_span()?;
            Some((p.type_expr(), name, sp))
        })
        .collect()
}

/// Parameter list for a top-level [`FunctionDecl`] (uses AST [`FnParam`](crate::ast::FnParam) nodes).
#[must_use]
pub fn extract_function_params(fd: &FunctionDecl) -> Vec<(Option<TypeExpr>, String, Span)> {
    extract_fn_params_from_syntax(fd.syntax())
}
