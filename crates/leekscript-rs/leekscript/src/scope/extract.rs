//! Pull parameter lists and similar shapes from the CST.

use crate::ast::types::{TypeExpr, TypePrimaryType};
use crate::ast::FunctionDecl;
use crate::syntax::kinds::K;
use crate::syntax::syntax_el_is_trivia;
use crate::Span;
use sipha::tree::ast::AstNode;
use sipha::tree::red::{SyntaxElement, SyntaxNode};
use sipha::types::IntoSyntaxKind;

use super::leek_ty::LeekTy;

/// Map a parsed type syntax node to [`LeekTy`].
#[must_use]
pub fn leek_ty_from_type_expr(te: &TypeExpr) -> LeekTy {
    let Some(u) = te.union_type() else {
        return LeekTy::Unknown;
    };
    let members = u.nullable_members();
    if members.is_empty() {
        return LeekTy::Unknown;
    }
    if members.len() == 1 {
        let m = &members[0];
        let inner = m
            .primary()
            .map(|p| leek_ty_from_primary(&p))
            .unwrap_or(LeekTy::Unknown);
        return if m.is_optional() {
            LeekTy::Nullable(Box::new(inner))
        } else {
            inner
        };
    }
    LeekTy::Union(
        members
            .iter()
            .map(|m| {
                let mut t = m
                    .primary()
                    .map(|p| leek_ty_from_primary(&p))
                    .unwrap_or(LeekTy::Unknown);
                if m.is_optional() {
                    t = LeekTy::Nullable(Box::new(t));
                }
                t
            })
            .collect(),
    )
}

fn leek_ty_from_primary(p: &TypePrimaryType) -> LeekTy {
    if let Some(id) = p.ident_text() {
        return LeekTy::Class(id);
    }
    for t in p.syntax().child_tokens() {
        match t.kind_as::<K>() {
            Some(K::VoidKw) => return LeekTy::Void,
            Some(K::BooleanKw) => return LeekTy::Boolean,
            Some(K::AnyKw) => return LeekTy::Any,
            Some(K::IntegerKw) => return LeekTy::Integer,
            Some(K::RealKw) => return LeekTy::Real,
            Some(K::StringTypeKw) => return LeekTy::String,
            Some(K::NullKw) => return LeekTy::Null,
            Some(K::ObjectKw) => return LeekTy::Any,
            Some(K::ClassTypeKw) => return LeekTy::Class("Class".into()),
            Some(K::ArrayKw) => {
                let args = p.generic_argument_roots();
                let el = args
                    .first()
                    .map(|a| leek_ty_from_type_expr(&a))
                    .unwrap_or(LeekTy::Any);
                return LeekTy::Array(Box::new(el));
            }
            Some(K::SetTypeKw) => {
                let args = p.generic_argument_roots();
                let el = args
                    .first()
                    .map(|a| leek_ty_from_type_expr(&a))
                    .unwrap_or(LeekTy::Any);
                return LeekTy::Array(Box::new(el));
            }
            Some(K::MapKw) => {
                let args = p.generic_argument_roots();
                let k = args
                    .get(0)
                    .map(leek_ty_from_type_expr)
                    .unwrap_or(LeekTy::Any);
                let v = args
                    .get(1)
                    .map(leek_ty_from_type_expr)
                    .unwrap_or(LeekTy::Any);
                return LeekTy::Map(Box::new(k), Box::new(v));
            }
            Some(K::IntervalKw) => {
                let args = p.generic_argument_roots();
                let inner = args
                    .first()
                    .map(leek_ty_from_type_expr)
                    .unwrap_or(LeekTy::Unknown);
                return LeekTy::Interval(Box::new(LeekTy::interval_inner(inner)));
            }
            Some(K::FunctionTypeKw) => {
                let args = p.generic_argument_roots();
                let ret = args
                    .last()
                    .map(leek_ty_from_type_expr)
                    .unwrap_or(LeekTy::Any);
                let params = if args.len() <= 1 {
                    Vec::new()
                } else {
                    args[..args.len() - 1]
                        .iter()
                        .map(leek_ty_from_type_expr)
                        .collect()
                };
                return LeekTy::Function {
                    params,
                    ret: Box::new(ret),
                };
            }
            _ => {}
        }
    }
    LeekTy::Unknown
}

/// Parameter list for a top-level [`FunctionDecl`].
#[must_use]
pub fn extract_function_params(fd: &FunctionDecl) -> Vec<(Option<TypeExpr>, String, Span)> {
    extract_param_list_after_first_paren(fd.syntax())
}

/// Extract `( … )` parameter lists (comma-separated `fn_param` items).
#[must_use]
pub fn extract_param_list_after_first_paren(node: &SyntaxNode) -> Vec<(Option<TypeExpr>, String, Span)> {
    let mut out = Vec::new();
    let mut in_params = false;
    let mut depth = 0i32;
    let mut current: Vec<SyntaxElement> = Vec::new();

    for el in node.children() {
        if syntax_el_is_trivia(&el) {
            continue;
        }
        if let Some(t) = el.as_token() {
            let k = t.kind();
            if k == K::LParen.into_syntax_kind() {
                if !in_params {
                    in_params = true;
                    depth = 1;
                } else {
                    depth += 1;
                    if in_params {
                        current.push(el.clone());
                    }
                }
                continue;
            }
            if k == K::RParen.into_syntax_kind() {
                if in_params {
                    depth -= 1;
                    if depth == 0 {
                        flush_param_segment(&mut out, &current);
                        break;
                    }
                }
                continue;
            }
            if in_params && depth == 1 && k == K::Comma.into_syntax_kind() {
                flush_param_segment(&mut out, &current);
                current.clear();
                continue;
            }
        }
        if in_params && depth == 1 {
            current.push(el.clone());
        }
    }
    out
}

fn flush_param_segment(out: &mut Vec<(Option<TypeExpr>, String, Span)>, elems: &[SyntaxElement]) {
    let mut ty = None;
    let mut name: Option<String> = None;
    let mut name_span: Option<Span> = None;
    for el in elems {
        if syntax_el_is_trivia(el) {
            continue;
        }
        if let SyntaxElement::Node(n) = el {
            if let Some(te) = TypeExpr::cast(n.clone()) {
                ty = Some(te);
            }
        }
        if let SyntaxElement::Token(t) = el {
            if t.kind_as::<K>() == Some(K::Ident) {
                name = Some(t.text().to_string());
                name_span = Some(t.text_range());
            }
        }
    }
    if let (Some(n), Some(sp)) = (name, name_span) {
        out.push((ty, n, sp));
    }
}

/// Method or constructor body member (`foo() {}`, `constructor() {}`).
#[must_use]
pub fn try_extract_class_method(cm: &SyntaxNode, class_name: &str) -> Option<ClassMethodHead> {
    if cm.kind_as::<K>() != Some(K::ClassMember) {
        return None;
    }
    cm.child_nodes()
        .find(|n| n.kind_as::<K>() == Some(K::Block))?;
    let is_ctor = cm
        .descendant_tokens()
        .iter()
        .any(|t| t.kind_as::<K>() == Some(K::ConstructorKw));

    let children: Vec<SyntaxElement> = cm
        .children()
        .filter(|e| !syntax_el_is_trivia(e))
        .collect();
    let block_idx = children.iter().position(|e| {
        e.as_node()
            .is_some_and(|n| n.kind_as::<K>() == Some(K::Block))
    })?;
    let before_block = &children[..block_idx];
    let params = extract_param_list_from_prefix(before_block);
    let (name, name_span) = if is_ctor {
        let span = cm
            .descendant_tokens()
            .iter()
            .find(|t| t.kind_as::<K>() == Some(K::ConstructorKw))
            .map(|t| t.text_range())
            .unwrap_or_else(|| Span::new(0, 0));
        (class_name.to_string(), span)
    } else {
        last_ident_before_lparen(before_block)?
    };
    Some(ClassMethodHead {
        name,
        name_span,
        params,
        is_constructor: is_ctor,
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

fn last_ident_before_lparen(before_block: &[SyntaxElement]) -> Option<(String, Span)> {
    let lparen_idx = before_block.iter().position(|e| {
        e.as_token()
            .is_some_and(|t| t.kind() == K::LParen.into_syntax_kind())
    })?;
    let mut out = None;
    for el in &before_block[..lparen_idx] {
        if let Some(t) = el.as_token() {
            if t.kind_as::<K>() == Some(K::Ident) {
                out = Some((t.text().to_string(), t.text_range()));
            }
        }
    }
    out
}

fn extract_param_list_from_prefix(before_block: &[SyntaxElement]) -> Vec<(Option<TypeExpr>, String, Span)> {
    let mut tmp = Vec::new();
    let mut in_params = false;
    let mut depth = 0i32;
    let mut current: Vec<SyntaxElement> = Vec::new();
    for el in before_block {
        if let Some(t) = el.as_token() {
            let k = t.kind();
            if k == K::LParen.into_syntax_kind() {
                if !in_params {
                    in_params = true;
                    depth = 1;
                } else {
                    depth += 1;
                    current.push((*el).clone());
                }
                continue;
            }
            if k == K::RParen.into_syntax_kind() {
                if in_params {
                    depth -= 1;
                    if depth == 0 {
                        flush_param_segment(&mut tmp, &current);
                        current.clear();
                        in_params = false;
                    }
                }
                continue;
            }
            if in_params && depth == 1 && k == K::Comma.into_syntax_kind() {
                flush_param_segment(&mut tmp, &current);
                current.clear();
                continue;
            }
        }
        if in_params && depth == 1 {
            current.push((*el).clone());
        }
    }
    tmp
}

/// Field declaration without method body (`integer x;`, `T name = expr`).
#[must_use]
pub fn try_extract_class_field(cm: &SyntaxNode) -> Option<(String, Span, LeekTy)> {
    if cm.kind_as::<K>() != Some(K::ClassMember) {
        return None;
    }
    if cm
        .child_nodes()
        .any(|n| n.kind_as::<K>() == Some(K::Block))
    {
        return None;
    }
    let ty = cm
        .child_nodes()
        .find_map(|n| TypeExpr::cast(n))
        .map(|te| leek_ty_from_type_expr(&te))
        .unwrap_or(LeekTy::Unknown);
    let name_tok = cm
        .descendant_tokens()
        .into_iter()
        .filter(|t| t.kind_as::<K>() == Some(K::Ident))
        .last()?;
    Some((name_tok.text().to_string(), name_tok.text_range(), ty))
}
