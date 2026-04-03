use crate::Span;
use crate::ast::FunctionDecl;
use crate::ast::types::TypeExpr;

/// Parameter list for a top-level [`FunctionDecl`] (uses AST [`FnParam`](crate::ast::FnParam) nodes).
#[must_use]
pub fn extract_function_params(fd: &FunctionDecl) -> Vec<(Option<TypeExpr>, String, Span)> {
    fd.fn_params()
        .filter_map(|p| {
            let name = p.name()?;
            let sp = p.name_span()?;
            Some((p.type_expr(), name, sp))
        })
        .collect()
}
