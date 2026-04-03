use crate::ast::types::TypeExpr;
use crate::ast::ClassMember;
use crate::syntax::kinds::K;
use crate::Span;
use sipha::tree::ast::AstNode;
use sipha::tree::red::SyntaxNode;

use super::type_expr::leek_ty_from_type_expr;
use crate::scope::leek_ty::LeekTy;

/// Method or constructor body member (`foo() {}`, `constructor() {}`).
#[must_use]
pub fn try_extract_class_method(cm: &SyntaxNode, class_name: &str) -> Option<ClassMethodHead> {
    let cm = ClassMember::cast(cm.clone())?;
    if !cm.has_method_body() {
        return None;
    }
    let (name, name_span) = cm.method_name_and_span(class_name)?;
    let params = cm
        .fn_params()
        .filter_map(|p| {
            let n = p.name()?;
            let sp = p.name_span()?;
            Some((p.type_expr(), n, sp))
        })
        .collect();
    Some(ClassMethodHead {
        name,
        name_span,
        params,
        is_constructor: cm.is_constructor(),
    })
}

/// Parsed method / constructor header (no body).
#[derive(Clone, Debug)]
pub struct ClassMethodHead {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<(Option<TypeExpr>, String, Span)>,
    pub is_constructor: bool,
}

/// Field declaration without method body (`integer x;`, `T name = expr`).
#[must_use]
pub fn try_extract_class_field(cm: &SyntaxNode) -> Option<(String, Span, LeekTy)> {
    let cm = ClassMember::cast(cm.clone())?;
    if cm.has_method_body() {
        return None;
    }
    let ty = cm
        .leading_type_expr()
        .map(|te| leek_ty_from_type_expr(&te))
        .unwrap_or(LeekTy::Unknown);
    let name_tok = cm
        .syntax()
        .descendant_tokens()
        .into_iter()
        .filter(|t| t.kind_as::<K>() == Some(K::Ident))
        .last()?;
    Some((name_tok.text().to_string(), name_tok.text_range(), ty))
}
