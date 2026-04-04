use crate::Span;
use crate::ast::ClassMember;
use crate::ast::types::TypeExpr;
use crate::syntax::kinds::K;
use sipha::tree::ast::AstNode;
use sipha::tree::red::SyntaxNode;

use super::type_expr::leek_ty_from_type_expr_with_templates;
use crate::scope::leek_ty::LeekTy;

/// When the lexer emits a builtin type spelling as [`K::Ident`] (should not happen, but avoids
/// mis-extracting `cellId integer` as a field named `integer`).
#[must_use]
pub(crate) fn leek_ty_from_builtin_type_ident_text(s: &str) -> Option<LeekTy> {
    Some(match s {
        "integer" | "int" | "byte" | "short" | "long" => LeekTy::Integer,
        "real" | "float" | "double" => LeekTy::Real,
        "string" => LeekTy::String,
        "boolean" => LeekTy::Boolean,
        "any" => LeekTy::Any,
        "void" => LeekTy::Void,
        _ => return None,
    })
}

fn leek_ty_from_type_name_keyword(k: K) -> Option<LeekTy> {
    Some(match k {
        K::StringTypeKw => LeekTy::String,
        K::IntegerKw | K::IntKw | K::ByteKw | K::ShortKw | K::LongKw => LeekTy::Integer,
        K::RealKw | K::FloatKw | K::DoubleKw => LeekTy::Real,
        K::BooleanKw => LeekTy::Boolean,
        K::AnyKw => LeekTy::Any,
        K::VoidKw => LeekTy::Void,
        K::ClassTypeKw => LeekTy::Class("Class".to_string()),
        K::ObjectKw => LeekTy::Class("Object".to_string()),
        K::ArrayKw => LeekTy::Array(Box::new(LeekTy::Unknown)),
        K::SetTypeKw => LeekTy::Array(Box::new(LeekTy::Unknown)),
        K::MapKw => LeekTy::Map(Box::new(LeekTy::Unknown), Box::new(LeekTy::Unknown)),
        K::FunctionTypeKw => LeekTy::Function {
            params: vec![],
            ret: Box::new(LeekTy::Unknown),
        },
        K::IntervalKw => LeekTy::Interval(Box::new(LeekTy::Unknown)),
        _ => return None,
    })
}

/// `id string`, `count integer`, `T value`, … — field name before type keyword / user type.
fn try_extract_name_first_field(
    cm: &ClassMember,
    class_templates: &[String],
) -> Option<(String, Span, LeekTy)> {
    let tokens: Vec<_> = cm
        .syntax()
        .descendant_tokens()
        .into_iter()
        .filter(|t| !t.is_trivia())
        .collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].kind_as::<K>() {
            Some(
                K::PrivateKw | K::PublicKw | K::ProtectedKw | K::StaticKw | K::FinalKw,
            ) => i += 1,
            _ => break,
        }
    }
    if i + 1 >= tokens.len() {
        return None;
    }
    let t0 = &tokens[i];
    let t1 = &tokens[i + 1];
    if t0.kind_as::<K>() != Some(K::Ident) {
        return None;
    }
    if let Some(ty) = t1.kind_as::<K>().and_then(leek_ty_from_type_name_keyword) {
        return Some((t0.text().to_string(), t0.text_range(), ty));
    }
    if t1.kind_as::<K>() == Some(K::Ident) {
        if let Some(ty) = leek_ty_from_builtin_type_ident_text(t1.text()) {
            return Some((t0.text().to_string(), t0.text_range(), ty));
        }
        // `Cell id`, `Entity target` — user type (leading ident) then field name.
        let type_name = t0.text();
        if type_name
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase())
        {
            let ty = if class_templates.iter().any(|n| n == type_name) {
                LeekTy::TypeParam(type_name.to_string())
            } else {
                LeekTy::Class(type_name.to_string())
            };
            return Some((t1.text().to_string(), t1.text_range(), ty));
        }
    }
    None
}

/// Java-style `integer x` / `Array<T> items`: first field-name ident after the leading type span.
fn try_extract_type_first_field(
    cm: &ClassMember,
    class_templates: &[String],
) -> Option<(String, Span, LeekTy)> {
    let te = cm.leading_type_expr()?;
    let ty = leek_ty_from_type_expr_with_templates(&te, class_templates);
    let type_end = te.syntax().text_range().end;
    let name_tok = cm
        .syntax()
        .descendant_semantic_tokens()
        .into_iter()
        .find(|t| t.offset() >= type_end && t.kind_as::<K>() == Some(K::Ident))?;
    Some((
        name_tok.text().to_string(),
        name_tok.text_range(),
        ty,
    ))
}

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

/// Field declaration without method body (`integer x;`, `id string;`, `T name = expr`).
#[must_use]
pub fn try_extract_class_field(
    cm: &SyntaxNode,
    class_templates: &[String],
) -> Option<(String, Span, LeekTy)> {
    let cm = ClassMember::cast(cm.clone())?;
    if cm.has_method_body() {
        return None;
    }
    if let Some(triple) = try_extract_name_first_field(&cm, class_templates) {
        return Some(triple);
    }
    if let Some(triple) = try_extract_type_first_field(&cm, class_templates) {
        return Some(triple);
    }
    let ty = cm
        .leading_type_expr()
        .map(|te| leek_ty_from_type_expr_with_templates(&te, class_templates))
        .unwrap_or(LeekTy::Unknown);
    let name_tok = cm
        .syntax()
        .descendant_tokens()
        .into_iter()
        .filter(|t| t.kind_as::<K>() == Some(K::Ident))
        .last()?;
    Some((name_tok.text().to_string(), name_tok.text_range(), ty))
}

