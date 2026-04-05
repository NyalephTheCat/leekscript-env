use crate::ast::types::{TypeExpr, TypePrimaryType};
use crate::syntax::kinds::Lex;
use sipha::tree::ast::AstNode;

use crate::scope::leek_ty::LeekTy;

#[inline]
fn template_list_contains(template_names: &[String], id: &str) -> bool {
    template_names.iter().any(|n| n == id)
}

/// Map a parsed type syntax node to [`LeekTy`].
#[must_use]
pub fn leek_ty_from_type_expr(te: &TypeExpr) -> LeekTy {
    leek_ty_from_type_expr_with_templates(te, &[])
}

/// Like [`leek_ty_from_type_expr`], but treats identifiers in `template_names` as [`LeekTy::TypeParam`].
#[must_use]
pub(crate) fn leek_ty_from_type_expr_with_templates(
    te: &TypeExpr,
    template_names: &[String],
) -> LeekTy {
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
            .map(|p| leek_ty_from_primary(&p, template_names))
            .unwrap_or(LeekTy::Unknown);
        let t = if m.is_optional() {
            LeekTy::Nullable(Box::new(inner))
        } else {
            inner
        };
        return t.normalize_null_in_union();
    }
    LeekTy::Union(
        members
            .iter()
            .map(|m| {
                let mut t = m
                    .primary()
                    .map(|p| leek_ty_from_primary(&p, template_names))
                    .unwrap_or(LeekTy::Unknown);
                if m.is_optional() {
                    t = LeekTy::Nullable(Box::new(t));
                }
                t
            })
            .collect(),
    )
    .normalize_null_in_union()
}

pub(super) fn leek_ty_from_primary(p: &TypePrimaryType, template_names: &[String]) -> LeekTy {
    if let Some(id) = p.ident_text() {
        if template_list_contains(template_names, &id) {
            return LeekTy::TypeParam(id);
        }
        return LeekTy::Class(id);
    }
    for t in p.syntax().child_tokens() {
        match t.kind_as::<Lex>() {
            Some(Lex::VoidKw) => return LeekTy::Void,
            Some(Lex::BooleanKw) => return LeekTy::Boolean,
            Some(Lex::AnyKw) => return LeekTy::Any,
            Some(Lex::IntegerKw) => return LeekTy::Integer,
            Some(Lex::RealKw) => return LeekTy::Real,
            Some(Lex::StringTypeKw) => return LeekTy::String,
            Some(Lex::NullKw) => return LeekTy::Null,
            Some(Lex::ObjectKw) => return LeekTy::Any,
            Some(Lex::ClassTypeKw) => return LeekTy::Class("Class".into()),
            Some(Lex::ArrayKw) => {
                let args = p.generic_argument_roots();
                let el = args
                    .first()
                    .map(|a| leek_ty_from_type_expr_with_templates(a, template_names))
                    .unwrap_or(LeekTy::Any);
                return LeekTy::Array(Box::new(el));
            }
            Some(Lex::SetTypeKw) => {
                let args = p.generic_argument_roots();
                let el = args
                    .first()
                    .map(|a| leek_ty_from_type_expr_with_templates(a, template_names))
                    .unwrap_or(LeekTy::Any);
                return LeekTy::Array(Box::new(el));
            }
            Some(Lex::MapKw) => {
                let args = p.generic_argument_roots();
                let k = args
                    .get(0)
                    .map(|a| leek_ty_from_type_expr_with_templates(a, template_names))
                    .unwrap_or(LeekTy::Any);
                let v = args
                    .get(1)
                    .map(|a| leek_ty_from_type_expr_with_templates(a, template_names))
                    .unwrap_or(LeekTy::Any);
                return LeekTy::Map(Box::new(k), Box::new(v));
            }
            Some(Lex::IntervalKw) => {
                let args = p.generic_argument_roots();
                let inner = args
                    .first()
                    .map(|a| leek_ty_from_type_expr_with_templates(a, template_names))
                    .unwrap_or(LeekTy::Unknown);
                return LeekTy::Interval(Box::new(LeekTy::interval_inner(inner)));
            }
            Some(Lex::FunctionTypeKw) => {
                let args = p.generic_argument_roots();
                let ret = args
                    .last()
                    .map(|a| leek_ty_from_type_expr_with_templates(a, template_names))
                    .unwrap_or(LeekTy::Any);
                let params = if args.len() <= 1 {
                    Vec::new()
                } else {
                    args[..args.len() - 1]
                        .iter()
                        .map(|a| leek_ty_from_type_expr_with_templates(a, template_names))
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
