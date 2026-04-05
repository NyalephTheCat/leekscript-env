//! Whether to insert a space between two consecutive non-trivia tokens.

use crate::format::options::FormatOptions;
use crate::syntax::kinds::Lex;

#[inline]
fn is_ident_or_literal(k: Lex) -> bool {
    matches!(
        k,
        Lex::Ident | Lex::Number | Lex::String | Lex::Pi | Lex::Infinity | Lex::TrueKw | Lex::FalseKw | Lex::NullKw
    )
}

#[inline]
fn is_class_member_modifier(k: Lex) -> bool {
    matches!(
        k,
        Lex::PublicKw | Lex::PrivateKw | Lex::ProtectedKw | Lex::StaticKw | Lex::FinalKw
    )
}

#[inline]
fn is_type_keyword(k: Lex) -> bool {
    matches!(
        k,
        Lex::IntegerKw
            | Lex::RealKw
            | Lex::StringTypeKw
            | Lex::BooleanKw
            | Lex::AnyKw
            | Lex::ClassTypeKw
            | Lex::ObjectKw
            | Lex::ArrayKw
            | Lex::SetTypeKw
            | Lex::MapKw
            | Lex::FunctionTypeKw
            | Lex::IntervalKw
            | Lex::VoidKw
    )
}

#[inline]
fn is_keyword_word(k: Lex) -> bool {
    matches!(
        k,
        // Operator-ish keywords
        Lex::InstanceofKw | Lex::XorKw | Lex::NotKw | Lex::InKw | Lex::AsKw | Lex::IsKw
            // Literal keywords
            | Lex::TrueKw
            | Lex::FalseKw
            | Lex::NullKw
            // Language keywords
            | Lex::VarKw
            | Lex::LetKw
            | Lex::BreakKw
            | Lex::ContinueKw
            | Lex::DoKw
            | Lex::ReturnKw
            | Lex::FunctionKw
            | Lex::IfKw
            | Lex::ElseKw
            | Lex::ForKw
            | Lex::WhileKw
            | Lex::IncludeKw
            | Lex::MatchKw
            | Lex::ClassKw
            | Lex::NewKw
            | Lex::ThisKw
            | Lex::SuperKw
            | Lex::SwitchKw
            | Lex::CaseKw
            | Lex::DefaultKw
            | Lex::GlobalKw
            | Lex::ExtendsKw
            | Lex::PublicKw
            | Lex::PrivateKw
            | Lex::ProtectedKw
            | Lex::StaticKw
            | Lex::FinalKw
            | Lex::ConstructorKw
            // Type keywords / reserved spellings
            | Lex::VoidKw
            | Lex::BooleanKw
            | Lex::AnyKw
            | Lex::IntegerKw
            | Lex::RealKw
            | Lex::StringTypeKw
            | Lex::ClassTypeKw
            | Lex::ObjectKw
            | Lex::ArrayKw
            | Lex::SetTypeKw
            | Lex::MapKw
            | Lex::FunctionTypeKw
            // Java v3 reserved words present in lexer
            | Lex::AbstractKw
            | Lex::AwaitKw
            | Lex::ByteKw
            | Lex::CatchKw
            | Lex::CharKw
            | Lex::ConstKw
            | Lex::DoubleKw
            | Lex::EnumKw
            | Lex::EvalKw
            | Lex::ExportKw
            | Lex::FinallyKw
            | Lex::FloatKw
            | Lex::GotoKw
            | Lex::ImplementsKw
            | Lex::ImportKw
            | Lex::IntKw
            | Lex::InterfaceKw
            | Lex::LongKw
            | Lex::NativeKw
            | Lex::PackageKw
            | Lex::ShortKw
            | Lex::SynchronizedKw
            | Lex::ThrowKw
            | Lex::ThrowsKw
            | Lex::TransientKw
            | Lex::TryKw
            | Lex::TypeofKw
            | Lex::VolatileKw
            | Lex::WithKw
            | Lex::YieldKw
    )
}

#[inline]
fn is_wordish(k: Lex) -> bool {
    k == Lex::Ident
        || k == Lex::Number
        || k == Lex::String
        || k == Lex::Pi
        || k == Lex::Infinity
        || is_keyword_word(k)
}

#[inline]
fn is_unary_prefix(k: Lex) -> bool {
    matches!(
        k,
        Lex::Bang | Lex::Tilde | Lex::Plus | Lex::Minus | Lex::PlusPlus | Lex::MinusMinus
    )
}

#[inline]
fn closes_expr_atom(p: Lex) -> bool {
    matches!(
        p,
        Lex::RParen | Lex::RBracket | Lex::RBrace | Lex::String | Lex::Number | Lex::Ident
    )
}

#[inline]
fn keyword_before_paren(p: Lex) -> bool {
    matches!(
        p,
        Lex::IfKw | Lex::WhileKw | Lex::ForKw | Lex::SwitchKw | Lex::CatchKw | Lex::FunctionTypeKw
    )
}

#[inline]
fn is_binary_op(k: Lex) -> bool {
    matches!(
        k,
        Lex::Plus
            | Lex::Minus
            | Lex::Star
            | Lex::Slash
            | Lex::Percent
            | Lex::Eq
            | Lex::EqEq
            | Lex::NotEq
            | Lex::EqEqEq
            | Lex::NotEqEq
            | Lex::Lt
            | Lex::Lte
            | Lex::Gt
            | Lex::Gte
            | Lex::AndAnd
            | Lex::OrOr
            | Lex::Coalesce
            | Lex::CoalesceEq
            | Lex::StarStar
            | Lex::StarStarEq
            | Lex::Shl
            | Lex::Shr
            | Lex::UShr
            | Lex::ShlEq
            | Lex::ShrEq
            | Lex::UShrEq
            | Lex::TripleShl
            | Lex::TripleShlEq
            | Lex::BitAnd
            | Lex::BitOr
            | Lex::BitXor
            | Lex::BitAndEq
            | Lex::BitOrEq
            | Lex::BitXorEq
            | Lex::PlusEq
            | Lex::MinusEq
            | Lex::StarEq
            | Lex::SlashEq
            | Lex::PercentEq
            | Lex::Arrow
            | Lex::InstanceofKw
            | Lex::InKw
            | Lex::IsKw
            | Lex::AsKw
            | Lex::XorKw
            | Lex::NotKw
    )
}

/// Insert a space between `prev` and `next` when both are semantic tokens.
///
/// When `in_type_syntax` is true (inside type-syntax CST nodes such as [`Node::TypeExpr`],
/// [`Node::TypeUnionType`], [`Node::TypeNullableType`], [`Node::TypePrimaryType`], or
/// [`Node::BuiltinTypeNameExpr`], [`Node::TemplateParams`]), spacing of
/// `|`, `<`, and `>` follows [`FormatOptions::space_around_type_operators`] (`false` →
/// `integer|real`, `Array<number>`; `true` → `integer | real`, `Array < number >`).
#[must_use]
pub fn needs_space_between(
    prev: Option<Lex>,
    next: Lex,
    opts: &FormatOptions,
    in_type_syntax: bool,
) -> bool {
    let Some(p) = prev else {
        return false;
    };
    if p == next && matches!(p, Lex::PlusPlus | Lex::MinusMinus) {
        return false;
    }

    // No space before closers and separators (prefix `++`/`--` falls through — e.g. `for (;; ++i)`).
    if matches!(
        next,
        Lex::Semi
            | Lex::Comma
            | Lex::RParen
            | Lex::RBracket
            | Lex::RBrace
            | Lex::Dot
            | Lex::PlusPlus
            | Lex::MinusMinus
    ) {
        if matches!(next, Lex::PlusPlus | Lex::MinusMinus) {
            if closes_expr_atom(p) {
                // `x ++` postfix — no space before ++
                return false;
            }
        } else if next == Lex::Dot {
            return false;
        } else {
            return false;
        }
    }

    // No space immediately after `(` `[` except option for inside parens
    if matches!(p, Lex::LParen | Lex::LBracket) {
        if next == Lex::RParen || next == Lex::RBracket {
            return opts.space_inside_parens;
        }
        return opts.space_inside_parens;
    }

    if p == Lex::LBrace {
        if next == Lex::RBrace {
            return opts.space_inside_parens;
        }
        // `{` before statement — layout is handled in the block printer, not a space.
        return false;
    }

    // No space after `.` (member access)
    if p == Lex::Dot {
        return false;
    }

    // `,` then next token — `a, b` / `f(a, b)` (not `,)` / `,,`).
    if p == Lex::Comma && opts.space_after_comma {
        return !matches!(
            next,
            Lex::Comma | Lex::Semi | Lex::RParen | Lex::RBracket | Lex::RBrace
        );
    }

    // `;` then next — `x; y`, `for (init; cond; step)`; not `;;`, `;)`, `;]`, `;}`.
    if p == Lex::Semi {
        return !matches!(next, Lex::Semi | Lex::RParen | Lex::RBracket | Lex::RBrace);
    }

    // Type / generic punctuation — not comparisons (`a < b`) or bitwise or (`x | y`), which are not
    // formatted under TypeExpr / BuiltinTypeNameExpr.
    if in_type_syntax
        && (matches!(p, Lex::BitOr | Lex::Lt | Lex::Gt) || matches!(next, Lex::BitOr | Lex::Lt | Lex::Gt))
    {
        return opts.space_around_type_operators;
    }

    // Nullable `T?` then name (`Map? get(...)`) or ternary `a ? b` — not `Map?get` / `a?b`.
    // `?.` optional chaining stays glued; `?|` / `? <` / `? >` use the type-operator rule above.
    if p == Lex::Question {
        if next == Lex::Dot {
            return false;
        }
        return true;
    }

    // `else` / `catch` / `finally` after `}`
    if p == Lex::RBrace && matches!(next, Lex::ElseKw | Lex::CatchKw | Lex::FinallyKw) {
        return opts.newline_before_else_catch_finally;
    }

    // `do { ... } while (...)` requires a space after `}`.
    if p == Lex::RBrace && next == Lex::WhileKw {
        return true;
    }

    // `)continue`, `)return`, `)fullDangerMap[…] = …` — single-line `if (…) stmt` / expression
    if p == Lex::RParen {
        if next == Lex::LBrace {
            return true;
        }
        if matches!(next, Lex::ContinueKw | Lex::BreakKw | Lex::ReturnKw | Lex::ThrowKw) {
            return true;
        }
        if is_ident_or_literal(next) || matches!(next, Lex::ThisKw | Lex::SuperKw) {
            return true;
        }
    }

    // Keyword + `(` spacing
    if next == Lex::LParen {
        if p == Lex::FunctionKw {
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
        if p == Lex::Eq {
            return opts.space_around_assign;
        }
        // `return (e)`, `throw (e)`, etc. — `(` block must not glue to the keyword (see LParen branch
        // above: without this, we fall through to `return false` before the generic keyword rule runs).
        if matches!(
            p,
            Lex::ReturnKw | Lex::ThrowKw | Lex::TypeofKw | Lex::VoidKw | Lex::CaseKw | Lex::GlobalKw
        ) {
            return true;
        }
        // Calls: `foo(`
        if p == Lex::Ident || p == Lex::RParen {
            return false;
        }
        return false;
    }

    // Keyword + `{` spacing (e.g. `do {}`, `else {}`, `try {}`).
    if next == Lex::LBrace
        && matches!(
            p,
            Lex::DoKw | Lex::ElseKw | Lex::TryKw | Lex::CatchKw | Lex::FinallyKw
        )
    {
        return true;
    }

    // `class Foo {`, type / name before block body (`foo {` is rare; ternary `:` spacing needs AST).
    if next == Lex::LBrace && p == Lex::Ident {
        return true;
    }

    // `function f() => Map {` — built-in / keyword types are tokens like `MapKw`, not `Ident`.
    if next == Lex::LBrace && is_type_keyword(p) {
        return true;
    }

    // `!=` split across two tokens (`!` + `=`) must not use assignment spacing.
    if p == Lex::Bang && next == Lex::Eq {
        return false;
    }

    // Assignment `=` (not `==`)
    if next == Lex::Eq {
        return opts.space_around_assign;
    }
    if p == Lex::Eq {
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
            Lex::LParen | Lex::LBracket | Lex::LBrace | Lex::Semi | Lex::Comma | Lex::Colon
        ) {
            return false;
        }
        if is_binary_op(p) || p == Lex::Eq {
            return opts.space_around_binary_ops;
        }
        return false;
    }

    // `return` / `throw` / `typeof` / `void` / `new` style
    if matches!(
        p,
        Lex::ReturnKw | Lex::ThrowKw | Lex::TypeofKw | Lex::VoidKw | Lex::NewKw | Lex::CaseKw | Lex::GlobalKw
    ) {
        return next != Lex::Semi && next != Lex::RBrace;
    }

    // Types before ident in declarations: `integer x`
    if matches!(
        p,
        Lex::IntegerKw
            | Lex::RealKw
            | Lex::StringTypeKw
            | Lex::BooleanKw
            | Lex::AnyKw
            | Lex::ClassTypeKw
            | Lex::ObjectKw
            | Lex::ArrayKw
            | Lex::SetTypeKw
            | Lex::MapKw
            | Lex::FunctionTypeKw
            | Lex::IntervalKw
    ) && matches!(next, Lex::Ident)
    {
        return true;
    }

    // Class member modifiers should be spaced: `private static final foo() {}`.
    if is_class_member_modifier(p)
        && (is_class_member_modifier(next)
            || matches!(next, Lex::Ident | Lex::ConstructorKw)
            || is_type_keyword(next))
    {
        return true;
    }

    // `var` / `let` / `const` before name
    if matches!(p, Lex::VarKw | Lex::LetKw | Lex::ConstKw) && matches!(next, Lex::Ident) {
        return true;
    }

    // Any two word-like tokens must not run together (changes meaning): `else if`, `do while`, etc.
    if is_wordish(p) && is_wordish(next) {
        return true;
    }

    // Two word-like tokens: `let x`, `return foo`, `3.14`, actually number+ident rare
    if (is_ident_or_literal(p) || matches!(p, Lex::ThisKw | Lex::SuperKw))
        && (is_ident_or_literal(next) || matches!(next, Lex::ThisKw | Lex::SuperKw))
    {
        return true;
    }

    // Keyword followed by wordish
    if matches!(
        p,
        Lex::FunctionKw | Lex::ClassKw | Lex::ExtendsKw | Lex::ImplementsKw | Lex::PackageKw | Lex::ImportKw
    ) && (is_ident_or_literal(next) || next == Lex::LBrace)
    {
        return next != Lex::LParen;
    }

    if p == Lex::GotoKw && next == Lex::Ident {
        return true;
    }

    // Default: no extra space between punctuation runs like `]);`
    false
}
