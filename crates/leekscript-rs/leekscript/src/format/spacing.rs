//! Whether to insert a space between two consecutive non-trivia tokens.

use crate::format::options::FormatOptions;
use crate::syntax::kinds::K;

#[inline]
fn is_ident_or_literal(k: K) -> bool {
    matches!(
        k,
        K::Ident
            | K::Number
            | K::String
            | K::Pi
            | K::Infinity
            | K::TrueKw
            | K::FalseKw
            | K::NullKw
    )
}

#[inline]
fn is_unary_prefix(k: K) -> bool {
    matches!(k, K::Bang | K::Tilde | K::Plus | K::Minus | K::PlusPlus | K::MinusMinus)
}

#[inline]
fn closes_expr_atom(p: K) -> bool {
    matches!(p, K::RParen | K::RBracket | K::RBrace | K::String | K::Number | K::Ident)
}

#[inline]
fn keyword_before_paren(p: K) -> bool {
    matches!(
        p,
        K::IfKw
            | K::WhileKw
            | K::ForKw
            | K::SwitchKw
            | K::CatchKw
            | K::FunctionTypeKw
    )
}

#[inline]
fn is_binary_op(k: K) -> bool {
    matches!(
        k,
        K::Plus
            | K::Minus
            | K::Star
            | K::Slash
            | K::Percent
            | K::Eq
            | K::EqEq
            | K::NotEq
            | K::EqEqEq
            | K::NotEqEq
            | K::Lt
            | K::Lte
            | K::Gt
            | K::Gte
            | K::AndAnd
            | K::OrOr
            | K::Coalesce
            | K::CoalesceEq
            | K::StarStar
            | K::StarStarEq
            | K::Shl
            | K::Shr
            | K::UShr
            | K::ShlEq
            | K::ShrEq
            | K::UShrEq
            | K::TripleShl
            | K::TripleShlEq
            | K::BitAnd
            | K::BitOr
            | K::BitXor
            | K::BitAndEq
            | K::BitOrEq
            | K::BitXorEq
            | K::PlusEq
            | K::MinusEq
            | K::StarEq
            | K::SlashEq
            | K::PercentEq
            | K::Arrow
            | K::InstanceofKw
            | K::InKw
            | K::IsKw
            | K::AsKw
            | K::XorKw
            | K::NotKw
    )
}

/// Insert a space between `prev` and `next` when both are semantic tokens.
#[must_use]
pub fn needs_space_between(prev: Option<K>, next: K, opts: &FormatOptions) -> bool {
    let Some(p) = prev else {
        return false;
    };
    if p == next && matches!(p, K::PlusPlus | K::MinusMinus) {
        return false;
    }

    // No space before closers and separators
    if matches!(
        next,
        K::Semi | K::Comma | K::RParen | K::RBracket | K::RBrace | K::Dot | K::PlusPlus | K::MinusMinus
    ) {
        // `x ++` postfix — no space before ++
        if matches!(next, K::PlusPlus | K::MinusMinus) && closes_expr_atom(p) {
            return false;
        }
        if next == K::Dot {
            return false;
        }
        return false;
    }

    // No space immediately after `(` `[` except option for inside parens
    if matches!(p, K::LParen | K::LBracket) {
        if next == K::RParen || next == K::RBracket {
            return opts.space_inside_parens;
        }
        return opts.space_inside_parens;
    }

    if p == K::LBrace {
        if next == K::RBrace {
            return opts.space_inside_parens;
        }
        // `{` before statement — layout is handled in the block printer, not a space.
        return false;
    }

    // No space after `.` (member access)
    if p == K::Dot {
        return false;
    }

    // `else` / `catch` / `finally` after `}`
    if p == K::RBrace && matches!(next, K::ElseKw | K::CatchKw | K::FinallyKw) {
        return opts.newline_before_else_catch_finally;
    }

    // Keyword + `(` spacing
    if next == K::LParen {
        if p == K::FunctionKw {
            return opts.space_before_function_decl_paren;
        }
        if keyword_before_paren(p) {
            return opts.space_after_keyword_before_paren;
        }
        // Calls: `foo(`
        if p == K::Ident || p == K::RParen {
            return false;
        }
        return false;
    }

    // Assignment `=` (not `==`)
    if next == K::Eq {
        return opts.space_around_assign;
    }
    if p == K::Eq {
        return opts.space_around_assign;
    }

    // Generic binary ops
    if is_binary_op(next) || is_binary_op(p) {
        return opts.space_around_binary_ops;
    }

    // Unary prefix after opener or op
    if is_unary_prefix(next) {
        if matches!(
            p,
            K::LParen | K::LBracket | K::LBrace | K::Semi | K::Comma | K::Colon
        ) {
            return false;
        }
        if is_binary_op(p) || p == K::Eq {
            return opts.space_around_binary_ops;
        }
        return false;
    }

    // `return` / `throw` / `typeof` / `void` / `new` style
    if matches!(
        p,
        K::ReturnKw | K::ThrowKw | K::TypeofKw | K::VoidKw | K::NewKw | K::CaseKw | K::GlobalKw
    ) {
        return next != K::Semi && next != K::RBrace;
    }

    // Types before ident in declarations: `integer x`
    if matches!(
        p,
        K::IntegerKw
            | K::RealKw
            | K::StringTypeKw
            | K::BooleanKw
            | K::AnyKw
            | K::ClassTypeKw
            | K::ObjectKw
            | K::ArrayKw
            | K::SetTypeKw
            | K::MapKw
    ) && matches!(next, K::Ident)
    {
        return true;
    }

    // `var` / `let` / `const` before name
    if matches!(p, K::VarKw | K::LetKw | K::ConstKw) && matches!(next, K::Ident) {
        return true;
    }

    // Two word-like tokens: `let x`, `return foo`, `3.14`, actually number+ident rare
    if (is_ident_or_literal(p) || matches!(p, K::ThisKw | K::SuperKw))
        && (is_ident_or_literal(next) || matches!(next, K::ThisKw | K::SuperKw))
    {
        return true;
    }

    // Keyword followed by wordish
    if matches!(
        p,
        K::FunctionKw | K::ClassKw | K::ExtendsKw | K::ImplementsKw | K::PackageKw | K::ImportKw
    ) && (is_ident_or_literal(next) || next == K::LBrace)
    {
        return next != K::LParen;
    }

    if p == K::GotoKw && next == K::Ident {
        return true;
    }

    // Default: no extra space between punctuation runs like `]);`
    false
}
