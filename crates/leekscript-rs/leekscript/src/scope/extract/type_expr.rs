use crate::ast::types::{TypeExpr, TypePrimaryType};
use crate::syntax::kinds::K;
use sipha::tree::ast::AstNode;

use crate::scope::leek_ty::LeekTy;

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

pub(super) fn leek_ty_from_primary(p: &TypePrimaryType) -> LeekTy {
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
