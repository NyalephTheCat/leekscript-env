//! Whether to insert a space between two consecutive non-trivia tokens.

use crate::format::options::FormatOptions;
use crate::syntax::kinds::K;

#[inline]
fn is_ident_or_literal(k: K) -> bool {
    matches!(
        k,
        K::Ident | K::Number | K::String | K::Pi | K::Infinity | K::TrueKw | K::FalseKw | K::NullKw
    )
}

#[inline]
fn is_class_member_modifier(k: K) -> bool {
    matches!(
        k,
        K::PublicKw | K::PrivateKw | K::ProtectedKw | K::StaticKw | K::FinalKw
    )
}

#[inline]
fn is_type_keyword(k: K) -> bool {
    matches!(
        k,
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
            | K::FunctionTypeKw
            | K::IntervalKw
            | K::VoidKw
    )
}

#[inline]
fn is_keyword_word(k: K) -> bool {
    matches!(
        k,
        // Operator-ish keywords
        K::InstanceofKw | K::XorKw | K::NotKw | K::InKw | K::AsKw | K::IsKw
            // Literal keywords
            | K::TrueKw
            | K::FalseKw
            | K::NullKw
            // Language keywords
            | K::VarKw
            | K::LetKw
            | K::BreakKw
            | K::ContinueKw
            | K::DoKw
            | K::ReturnKw
            | K::FunctionKw
            | K::IfKw
            | K::ElseKw
            | K::ForKw
            | K::WhileKw
            | K::IncludeKw
            | K::MatchKw
            | K::ClassKw
            | K::NewKw
            | K::ThisKw
            | K::SuperKw
            | K::SwitchKw
            | K::CaseKw
            | K::DefaultKw
            | K::GlobalKw
            | K::ExtendsKw
            | K::PublicKw
            | K::PrivateKw
            | K::ProtectedKw
            | K::StaticKw
            | K::FinalKw
            | K::ConstructorKw
            // Type keywords / reserved spellings
            | K::VoidKw
            | K::BooleanKw
            | K::AnyKw
            | K::IntegerKw
            | K::RealKw
            | K::StringTypeKw
            | K::ClassTypeKw
            | K::ObjectKw
            | K::ArrayKw
            | K::SetTypeKw
            | K::MapKw
            | K::FunctionTypeKw
            // Java v3 reserved words present in lexer
            | K::AbstractKw
            | K::AwaitKw
            | K::ByteKw
            | K::CatchKw
            | K::CharKw
            | K::ConstKw
            | K::DoubleKw
            | K::EnumKw
            | K::EvalKw
            | K::ExportKw
            | K::FinallyKw
            | K::FloatKw
            | K::GotoKw
            | K::ImplementsKw
            | K::ImportKw
            | K::IntKw
            | K::InterfaceKw
            | K::LongKw
            | K::NativeKw
            | K::PackageKw
            | K::ShortKw
            | K::SynchronizedKw
            | K::ThrowKw
            | K::ThrowsKw
            | K::TransientKw
            | K::TryKw
            | K::TypeofKw
            | K::VolatileKw
            | K::WithKw
            | K::YieldKw
    )
}

#[inline]
fn is_wordish(k: K) -> bool {
    k == K::Ident
        || k == K::Number
        || k == K::String
        || k == K::Pi
        || k == K::Infinity
        || is_keyword_word(k)
}

#[inline]
fn is_unary_prefix(k: K) -> bool {
    matches!(
        k,
        K::Bang | K::Tilde | K::Plus | K::Minus | K::PlusPlus | K::MinusMinus
    )
}

#[inline]
fn closes_expr_atom(p: K) -> bool {
    matches!(
        p,
        K::RParen | K::RBracket | K::RBrace | K::String | K::Number | K::Ident
    )
}

#[inline]
fn keyword_before_paren(p: K) -> bool {
    matches!(
        p,
        K::IfKw | K::WhileKw | K::ForKw | K::SwitchKw | K::CatchKw | K::FunctionTypeKw
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
///
/// When `in_type_syntax` is true (inside type-syntax CST nodes such as [`K::TypeExpr`],
/// [`K::TypeUnionType`], [`K::TypeNullableType`], [`K::TypePrimaryType`], or
/// [`K::BuiltinTypeNameExpr`], [`K::TemplateParams`]), spacing of
/// `|`, `<`, and `>` follows [`FormatOptions::space_around_type_operators`] (`false` →
/// `integer|real`, `Array<number>`; `true` → `integer | real`, `Array < number >`).
#[must_use]
pub fn needs_space_between(
    prev: Option<K>,
    next: K,
    opts: &FormatOptions,
    in_type_syntax: bool,
) -> bool {
    let Some(p) = prev else {
        return false;
    };
    if p == next && matches!(p, K::PlusPlus | K::MinusMinus) {
        return false;
    }

    // No space before closers and separators (prefix `++`/`--` falls through — e.g. `for (;; ++i)`).
    if matches!(
        next,
        K::Semi
            | K::Comma
            | K::RParen
            | K::RBracket
            | K::RBrace
            | K::Dot
            | K::PlusPlus
            | K::MinusMinus
    ) {
        if matches!(next, K::PlusPlus | K::MinusMinus) {
            if closes_expr_atom(p) {
                // `x ++` postfix — no space before ++
                return false;
            }
        } else if next == K::Dot {
            return false;
        } else {
            return false;
        }
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

    // `,` then next token — `a, b` / `f(a, b)` (not `,)` / `,,`).
    if p == K::Comma && opts.space_after_comma {
        return !matches!(
            next,
            K::Comma | K::Semi | K::RParen | K::RBracket | K::RBrace
        );
    }

    // `;` then next — `x; y`, `for (init; cond; step)`; not `;;`, `;)`, `;]`, `;}`.
    if p == K::Semi {
        return !matches!(next, K::Semi | K::RParen | K::RBracket | K::RBrace);
    }

    // Type / generic punctuation — not comparisons (`a < b`) or bitwise or (`x | y`), which are not
    // formatted under TypeExpr / BuiltinTypeNameExpr.
    if in_type_syntax
        && (matches!(p, K::BitOr | K::Lt | K::Gt) || matches!(next, K::BitOr | K::Lt | K::Gt))
    {
        return opts.space_around_type_operators;
    }

    // Nullable `T?` then name (`Map? get(...)`) or ternary `a ? b` — not `Map?get` / `a?b`.
    // `?.` optional chaining stays glued; `?|` / `? <` / `? >` use the type-operator rule above.
    if p == K::Question {
        if next == K::Dot {
            return false;
        }
        return true;
    }

    // `else` / `catch` / `finally` after `}`
    if p == K::RBrace && matches!(next, K::ElseKw | K::CatchKw | K::FinallyKw) {
        return opts.newline_before_else_catch_finally;
    }

    // `do { ... } while (...)` requires a space after `}`.
    if p == K::RBrace && next == K::WhileKw {
        return true;
    }

    // `)continue`, `)return`, `)fullDangerMap[…] = …` — single-line `if (…) stmt` / expression
    if p == K::RParen {
        if next == K::LBrace {
            return true;
        }
        if matches!(next, K::ContinueKw | K::BreakKw | K::ReturnKw | K::ThrowKw) {
            return true;
        }
        if is_ident_or_literal(next) || matches!(next, K::ThisKw | K::SuperKw) {
            return true;
        }
    }

    // Keyword + `(` spacing
    if next == K::LParen {
        if p == K::FunctionKw {
            return opts.space_before_function_decl_paren;
        }
        if keyword_before_paren(p) {
            return opts.space_after_keyword_before_paren;
        }
        // Operators followed by a parenthesized RHS: `a + (b)`, `a instanceof (b)`, `x = (y)`.
        // The generic binary-op rule below would normally cover this, but `(` is handled here.
        if is_binary_op(p) {
            return opts.space_around_binary_ops;
        }
        if p == K::Eq {
            return opts.space_around_assign;
        }
        // `return (e)`, `throw (e)`, etc. — `(` block must not glue to the keyword (see LParen branch
        // above: without this, we fall through to `return false` before the generic keyword rule runs).
        if matches!(
            p,
            K::ReturnKw | K::ThrowKw | K::TypeofKw | K::VoidKw | K::CaseKw | K::GlobalKw
        ) {
            return true;
        }
        // Calls: `foo(`
        if p == K::Ident || p == K::RParen {
            return false;
        }
        return false;
    }

    // Keyword + `{` spacing (e.g. `do {}`, `else {}`, `try {}`).
    if next == K::LBrace
        && matches!(
            p,
            K::DoKw | K::ElseKw | K::TryKw | K::CatchKw | K::FinallyKw
        )
    {
        return true;
    }

    // `class Foo {`, type / name before block body (`foo {` is rare; ternary `:` spacing needs AST).
    if next == K::LBrace && p == K::Ident {
        return true;
    }

    // `function f() => Map {` — built-in / keyword types are tokens like `MapKw`, not `Ident`.
    if next == K::LBrace && is_type_keyword(p) {
        return true;
    }

    // `!=` split across two tokens (`!` + `=`) must not use assignment spacing.
    if p == K::Bang && next == K::Eq {
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
            | K::FunctionTypeKw
            | K::IntervalKw
    ) && matches!(next, K::Ident)
    {
        return true;
    }

    // Class member modifiers should be spaced: `private static final foo() {}`.
    if is_class_member_modifier(p)
        && (is_class_member_modifier(next)
            || matches!(next, K::Ident | K::ConstructorKw)
            || is_type_keyword(next))
    {
        return true;
    }

    // `var` / `let` / `const` before name
    if matches!(p, K::VarKw | K::LetKw | K::ConstKw) && matches!(next, K::Ident) {
        return true;
    }

    // Any two word-like tokens must not run together (changes meaning): `else if`, `do while`, etc.
    if is_wordish(p) && is_wordish(next) {
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
