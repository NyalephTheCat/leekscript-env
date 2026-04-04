use super::cfg_flags;
use super::lexer_rules;
#[cfg(not(feature = "grammar-v4-only"))]
use crate::parse::version::FLAG_V1;
use crate::parse::version::{
    FLAG_EXP_EXCEPTIONS, FLAG_EXP_GOTO, FLAG_EXP_LET, FLAG_EXP_LEXICAL_CONST, FLAG_EXP_MATCH,
    FLAG_EXP_MODULES, FLAG_V2, FLAG_V3, FLAG_V4,
};
use crate::syntax::kinds::K;
use sipha::prelude::*;

/// Java v3+ spellings that `LexicalParser` maps to dedicated tokens (not `STRING` identifiers).
///
/// **Note:** leekscript-java `WordCompiler` does not treat most of these as real syntax yet; they
/// are reserved at lex time only. Omits words that are only blocked as identifiers when the
/// corresponding experimental flag is set (see `ident_reserved_word_shape_*`). `let` stays in this
/// list so `let` is never an identifier under v3+; only [`FLAG_EXP_LET`](crate::parse::version::FLAG_EXP_LET)
/// enables the `let` keyword.
const EXP_IDENT_EXCEPTIONS: &[&[u8]] = &[b"catch", b"finally", b"throw", b"try"];

const EXP_IDENT_MODULES: &[&[u8]] = &[b"export", b"import", b"package"];

const V3_LEX_RESERVED: &[&[u8]] = &[
    b"abstract",
    b"and",
    b"any",
    b"Array",
    b"as",
    b"await",
    b"boolean",
    b"break",
    b"byte",
    b"case",
    b"char",
    b"class",
    b"Class",
    b"constructor",
    b"continue",
    b"default",
    b"do",
    b"double",
    b"else",
    b"enum",
    b"eval",
    b"extends",
    b"false",
    b"final",
    b"float",
    b"for",
    b"function",
    b"Function",
    b"global",
    b"if",
    b"implements",
    b"include",
    b"in",
    b"instanceof",
    b"int",
    b"integer",
    b"interface",
    b"is",
    b"let",
    b"long",
    b"Map",
    b"native",
    b"new",
    b"not",
    b"null",
    b"Object",
    b"or",
    b"private",
    b"protected",
    b"public",
    b"real",
    b"return",
    b"Set",
    b"short",
    b"static",
    b"string",
    b"super",
    b"switch",
    b"synchronized",
    b"this",
    b"throws",
    b"transient",
    b"true",
    b"typeof",
    b"var",
    b"void",
    b"volatile",
    b"while",
    b"with",
    b"xor",
    b"yield",
];

pub fn define(g: &mut GrammarBuilder) {
    // One `TRIVIA` syntax node wrapping each skipped run; children are `WS` / `LINE_COMMENT` /
    // `BLOCK_COMMENT` trivia tokens. (A plain `trivia(K::Trivia, …)` around `call("ws")` would
    // emit overlapping trivia leaves: inner `WS` then outer `TRIVIA` for the same span.)
    g.lexer_rule("trivia", |g| {
        g.optional(|g| {
            g.node(K::Trivia, |g| {
                g.one_or_more(|g| {
                    g.choice3(
                        |g| {
                            g.call("ws");
                        },
                        |g| {
                            g.call("line_comment");
                        },
                        |g| {
                            g.call("block_comment");
                        },
                    );
                });
            });
        });
    });

    g.lexer_rule("ws", |g| {
        g.trivia(K::Ws, |g| {
            g.one_or_more(|g| {
                // Java: ' ', '\r', '\n', '\t', and char 160 (NBSP).
                g.choice(
                    |g| {
                        g.class(classes::WHITESPACE);
                    },
                    |g| {
                        g.char('\u{00A0}');
                    },
                );
            });
        });
    });

    g.lexer_rule("line_comment", |g| {
        g.trivia(K::LineComment, |g| {
            g.literal(b"//");
            g.zero_or_more(|g| {
                g.neg_lookahead(|g| {
                    g.byte(b'\n');
                });
                g.any_char();
            });
            g.optional(|g| {
                g.byte(b'\n');
            });
        });
    });

    g.lexer_rule("block_comment", |g| {
        g.trivia(K::BlockComment, |g| {
            g.literal(b"/*");

            // LeekScript v1 special-case: `/*/` is accepted as an immediate comment close.
            #[cfg(feature = "grammar-v4-only")]
            {
                g.zero_or_more(|g| {
                    g.neg_lookahead(|g| {
                        g.literal(b"*/");
                    });
                    g.any_char();
                });
                g.literal(b"*/");
            }
            #[cfg(not(feature = "grammar-v4-only"))]
            {
                g.choice(
                    |g| {
                        g.if_flag(FLAG_V1);
                        g.byte(b'/');
                    },
                    |g| {
                        g.zero_or_more(|g| {
                            g.neg_lookahead(|g| {
                                g.literal(b"*/");
                            });
                            g.any_char();
                        });
                        g.literal(b"*/");
                    },
                );
            }
        });
    });

    // Keywords (see `lexer_rules::versioned_keyword`): v1/v2 use case-insensitive identifiers for
    // language version <= 2 (`wordEquals` / `charEquals`), and case-sensitive for v3+.
    // `ident` is registered last so keywords win when both could match.
    lexer_rules::versioned_keyword_rule(g, "kw_return", K::ReturnKw, b"return");
    lexer_rules::versioned_keyword_rule(g, "kw_var", K::VarKw, b"var");
    lexer_rules::keyword_rule_if_experimental(g, "kw_let", FLAG_V3, FLAG_EXP_LET, K::LetKw, b"let");
    lexer_rules::versioned_keyword_rule(g, "kw_function", K::FunctionKw, b"function");
    lexer_rules::versioned_keyword_rule(g, "kw_if", K::IfKw, b"if");
    lexer_rules::versioned_keyword_rule(g, "kw_else", K::ElseKw, b"else");
    lexer_rules::versioned_keyword_rule(g, "kw_for", K::ForKw, b"for");
    lexer_rules::versioned_keyword_rule(g, "kw_while", K::WhileKw, b"while");
    lexer_rules::versioned_keyword_rule(g, "kw_do", K::DoKw, b"do");
    lexer_rules::versioned_keyword_rule(g, "kw_break", K::BreakKw, b"break");
    lexer_rules::versioned_keyword_rule(g, "kw_continue", K::ContinueKw, b"continue");
    lexer_rules::versioned_keyword_rule(g, "kw_include", K::IncludeKw, b"include");
    // Not in Java `LexicalParser`. Only a keyword for leekscript-rs v4+ sources.
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_match",
        FLAG_V4,
        FLAG_EXP_MATCH,
        K::MatchKw,
        b"match",
    );
    lexer_rules::versioned_keyword_rule_if(g, "kw_class", FLAG_V2, K::ClassKw, b"class");
    lexer_rules::versioned_keyword_rule_if(g, "kw_new", FLAG_V2, K::NewKw, b"new");
    // `kw_in` uses an identifier boundary (see `lexer_kw_in`): plain `literal("in")` would win in
    // the lexer trie and split `integer` into `in` + `teger`. Longer `in*` keywords stay here for
    // registration order / clarity.
    lexer_rules::versioned_keyword_rule(g, "kw_as", K::AsKw, b"as");
    lexer_rules::versioned_keyword_rule_if(
        g,
        "kw_instanceof",
        FLAG_V2,
        K::InstanceofKw,
        b"instanceof",
    );
    lexer_rules::versioned_keyword_rule(g, "kw_xor", K::XorKw, b"xor");
    lexer_rules::versioned_keyword_rule(g, "kw_not", K::NotKw, b"not");
    lexer_rules::versioned_keyword_rule(g, "kw_true", K::TrueKw, b"true");
    lexer_rules::versioned_keyword_rule(g, "kw_false", K::FalseKw, b"false");
    lexer_rules::versioned_keyword_rule(g, "kw_null", K::NullKw, b"null");
    lexer_rules::versioned_keyword_rule_if(g, "kw_this", FLAG_V2, K::ThisKw, b"this");
    lexer_rules::versioned_keyword_rule_if(g, "kw_super", FLAG_V2, K::SuperKw, b"super");
    lexer_rules::keyword_rule_if(g, "kw_switch", FLAG_V3, K::SwitchKw, b"switch");
    lexer_rules::keyword_rule_if(g, "kw_case", FLAG_V3, K::CaseKw, b"case");
    lexer_rules::keyword_rule_if(g, "kw_default", FLAG_V3, K::DefaultKw, b"default");
    lexer_rules::versioned_keyword_rule(g, "kw_global", K::GlobalKw, b"global");
    lexer_rules::versioned_keyword_rule_if(g, "kw_extends", FLAG_V2, K::ExtendsKw, b"extends");
    lexer_rules::versioned_keyword_rule_if(g, "kw_public", FLAG_V2, K::PublicKw, b"public");
    lexer_rules::versioned_keyword_rule_if(g, "kw_private", FLAG_V2, K::PrivateKw, b"private");
    lexer_rules::versioned_keyword_rule_if(
        g,
        "kw_protected",
        FLAG_V2,
        K::ProtectedKw,
        b"protected",
    );
    lexer_rules::versioned_keyword_rule_if(g, "kw_static", FLAG_V2, K::StaticKw, b"static");
    lexer_rules::keyword_rule_if(g, "kw_final", FLAG_V3, K::FinalKw, b"final");
    lexer_rules::versioned_keyword_rule_if(
        g,
        "kw_constructor",
        FLAG_V2,
        K::ConstructorKw,
        b"constructor",
    );
    lexer_rules::versioned_keyword_rule(g, "kw_is", K::IsKw, b"is");

    // Type names (also reserved as identifiers in Java parity mode)
    lexer_rules::keyword_rule_if(g, "kw_void", FLAG_V3, K::VoidKw, b"void");
    lexer_rules::keyword_rule_if(g, "kw_boolean", FLAG_V3, K::BooleanKw, b"boolean");
    lexer_rules::keyword_rule_if(g, "kw_any", FLAG_V3, K::AnyKw, b"any");
    lexer_rules::keyword_rule_if(g, "kw_integer", FLAG_V3, K::IntegerKw, b"integer");
    // `int` must not be registered before / tried as a shorter match than `integer` for the same
    // prefix (sipha tries alternatives in registration order within a dispatch group).
    lexer_rules::keyword_rule_if(g, "kw_int", FLAG_V3, K::IntKw, b"int");
    lexer_rules::keyword_rule_if(g, "kw_real", FLAG_V3, K::RealKw, b"real");
    lexer_rules::keyword_rule_if(g, "kw_string_type", FLAG_V3, K::StringTypeKw, b"string");
    lexer_rules::versioned_keyword_rule_if(g, "kw_class_type", FLAG_V2, K::ClassTypeKw, b"Class");
    lexer_rules::versioned_keyword_rule_if(g, "kw_object", FLAG_V2, K::ObjectKw, b"Object");
    lexer_rules::versioned_keyword_rule_if(g, "kw_array", FLAG_V2, K::ArrayKw, b"Array");
    lexer_rules::versioned_keyword_rule_if(g, "kw_set_type", FLAG_V2, K::SetTypeKw, b"Set");
    lexer_rules::versioned_keyword_rule_if(g, "kw_map", FLAG_V2, K::MapKw, b"Map");
    lexer_rules::versioned_keyword_rule_if(
        g,
        "kw_function_type",
        FLAG_V2,
        K::FunctionTypeKw,
        b"Function",
    );
    lexer_rules::versioned_keyword_rule_if(
        g,
        "kw_interval_type",
        FLAG_V2,
        K::IntervalKw,
        b"Interval",
    );

    // Java `LexicalParser` v3-only reserved spellings (`TokenType` …). Most have no counterpart in
    // leekscript-java `WordCompiler` yet; they exist so the lexer matches Java and the CST can
    // represent sources that use these tokens.
    lexer_rules::keyword_rule_if(g, "kw_abstract", FLAG_V3, K::AbstractKw, b"abstract");
    lexer_rules::keyword_rule_if(g, "kw_await", FLAG_V3, K::AwaitKw, b"await");
    lexer_rules::keyword_rule_if(g, "kw_byte", FLAG_V3, K::ByteKw, b"byte");
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_catch",
        FLAG_V3,
        FLAG_EXP_EXCEPTIONS,
        K::CatchKw,
        b"catch",
    );
    lexer_rules::keyword_rule_if(g, "kw_char", FLAG_V3, K::CharKw, b"char");
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_const",
        FLAG_V3,
        FLAG_EXP_LEXICAL_CONST,
        K::ConstKw,
        b"const",
    );
    lexer_rules::keyword_rule_if(g, "kw_double", FLAG_V3, K::DoubleKw, b"double");
    lexer_rules::keyword_rule_if(g, "kw_enum", FLAG_V3, K::EnumKw, b"enum");
    lexer_rules::keyword_rule_if(g, "kw_eval", FLAG_V3, K::EvalKw, b"eval");
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_export",
        FLAG_V3,
        FLAG_EXP_MODULES,
        K::ExportKw,
        b"export",
    );
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_finally",
        FLAG_V3,
        FLAG_EXP_EXCEPTIONS,
        K::FinallyKw,
        b"finally",
    );
    lexer_rules::keyword_rule_if(g, "kw_float", FLAG_V3, K::FloatKw, b"float");
    lexer_rules::keyword_rule_if_experimental(g, "kw_goto", FLAG_V3, FLAG_EXP_GOTO, K::GotoKw, b"goto");
    lexer_rules::keyword_rule_if(g, "kw_implements", FLAG_V3, K::ImplementsKw, b"implements");
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_import",
        FLAG_V3,
        FLAG_EXP_MODULES,
        K::ImportKw,
        b"import",
    );
    lexer_rules::keyword_rule_if(g, "kw_interface", FLAG_V3, K::InterfaceKw, b"interface");
    // `in` must not match a prefix of `integer` / `instanceof` / `interface` / `int`. Plain
    // `g.keyword` is only a literal; require a non–identifier-continuation boundary (same idea as
    // `reserved_word_then_not_ident_cont` for v1/v2 idents).
    g.lexer_rule("kw_in", lexer_kw_in);
    lexer_rules::keyword_rule_if(g, "kw_long", FLAG_V3, K::LongKw, b"long");
    lexer_rules::keyword_rule_if(g, "kw_native", FLAG_V3, K::NativeKw, b"native");
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_package",
        FLAG_V3,
        FLAG_EXP_MODULES,
        K::PackageKw,
        b"package",
    );
    lexer_rules::keyword_rule_if(g, "kw_short", FLAG_V3, K::ShortKw, b"short");
    lexer_rules::keyword_rule_if(
        g,
        "kw_synchronized",
        FLAG_V3,
        K::SynchronizedKw,
        b"synchronized",
    );
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_throw",
        FLAG_V3,
        FLAG_EXP_EXCEPTIONS,
        K::ThrowKw,
        b"throw",
    );
    lexer_rules::keyword_rule_if(g, "kw_throws", FLAG_V3, K::ThrowsKw, b"throws");
    lexer_rules::keyword_rule_if(g, "kw_transient", FLAG_V3, K::TransientKw, b"transient");
    lexer_rules::keyword_rule_if_experimental(
        g,
        "kw_try",
        FLAG_V3,
        FLAG_EXP_EXCEPTIONS,
        K::TryKw,
        b"try",
    );
    lexer_rules::keyword_rule_if(g, "kw_typeof", FLAG_V3, K::TypeofKw, b"typeof");
    lexer_rules::keyword_rule_if(g, "kw_volatile", FLAG_V3, K::VolatileKw, b"volatile");
    lexer_rules::keyword_rule_if(g, "kw_with", FLAG_V3, K::WithKw, b"with");
    lexer_rules::keyword_rule_if(g, "kw_yield", FLAG_V3, K::YieldKw, b"yield");

    g.lexer_rule("op_at", |g| {
        g.token(K::Operator, |g| {
            g.byte(b'@');
        });
    });

    g.lexer_rule("op_tilde", |g| {
        g.token(K::Tilde, |g| {
            g.byte(b'~');
        });
    });

    g.lexer_rule("number", |g| {
        g.token(K::Number, |g| {
            // Keep radix / suffix / exponent characters after a proper numeric
            // prefix. Do **not** start with a letter: otherwise words like `new`,
            // `null`, or `combo` are swallowed as NUMBER and keywords / `!=` /
            // identifiers break (see formatter / `!=` lexing).
            //
            // Still avoid eating `..` (interval / dotdot) by forbidding a '.' that
            // is immediately followed by '.'.
            g.choice(
                |g| {
                    g.class(classes::DIGIT);
                    g.zero_or_more(|g| number_char(g));
                    number_literal_tail(g);
                },
                |g| {
                    // `.5` style (leading dot, digit required after)
                    g.byte(b'.');
                    g.class(classes::DIGIT);
                    g.zero_or_more(|g| number_char(g));
                    number_literal_tail(g);
                },
            );
        });
    });

    // `break` / `continue` optional numeric level only. The general `number` rule also
    // accepts letters (radix/exponent), which would otherwise lex `for` as NUMBER after
    // `break` / `continue`, breaking the next `for (var …)` statement.
    g.lexer_rule("break_continue_level", |g| {
        g.token(K::Number, |g| {
            g.one_or_more(|g| {
                g.class(classes::DIGIT);
            });
        });
    });

    g.lexer_rule("string", |g| {
        g.token(K::String, |g| {
            g.choice(|g| quoted_string(g, b'"'), |g| quoted_string(g, b'\''));
        });
    });

    g.lexer_rule("pi", |g| {
        g.token(K::Pi, |g| {
            g.char('π');
        });
    });

    g.lexer_rule("infinity", |g| {
        g.token(K::Infinity, |g| {
            g.char('∞');
        });
    });

    lexer_rules::token_byte_rule(g, "semi", K::Semi, b';');
    lexer_rules::token_byte_rule(g, "comma", K::Comma, b',');
    lexer_rules::token_byte_rule(g, "colon", K::Colon, b':');
    lexer_rules::token_literal_rule(g, "dotdot", K::DotDot, b"..");

    g.lexer_rule("dot", |g| {
        g.token(K::Dot, |g| {
            // Member access `.` exists in modern fixture code (v2+).
            // Our version flags are mutually exclusive, so "v2+" is "not v1".
            cfg_flags::not_v1(g);
            g.byte(b'.');
        });
    });

    g.lexer_rule("arrow", |g| {
        g.token(K::Arrow, |g| {
            g.choice(
                |g| {
                    g.literal(b"=>");
                },
                |g| {
                    g.literal(b"->");
                },
            );
        });
    });

    lexer_rules::token_byte_rule(g, "lparen", K::LParen, b'(');
    lexer_rules::token_byte_rule(g, "rparen", K::RParen, b')');
    lexer_rules::token_byte_rule(g, "lbracket", K::LBracket, b'[');
    lexer_rules::token_byte_rule(g, "rbracket", K::RBracket, b']');
    lexer_rules::token_byte_rule(g, "lbrace", K::LBrace, b'{');
    lexer_rules::token_byte_rule(g, "rbrace", K::RBrace, b'}');

    g.lexer_rule("op_question", |g| {
        g.token(K::Question, |g| {
            g.byte(b'?');
            g.neg_lookahead(|g| {
                g.byte(b'?');
            });
        });
    });

    lexer_rules::token_literal_rule(g, "op_coalesce_eq", K::CoalesceEq, b"??=");
    lexer_rules::token_literal_rule(g, "op_coalesce", K::Coalesce, b"??");
    lexer_rules::token_literal_rule(g, "op_star_star_eq", K::StarStarEq, b"**=");
    lexer_rules::token_literal_rule(g, "op_star_star", K::StarStar, b"**");
    lexer_rules::token_literal_rule(g, "op_backslash_eq", K::BackslashEq, b"\\=");
    g.lexer_rule("op_backslash", |g| {
        g.token(K::Backslash, |g| {
            g.byte(b'\\');
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });

    lexer_rules::token_literal_rule(g, "op_ushr_eq", K::UShrEq, b">>>=");
    // Java `LexicalParser` operator table includes `<<<=` / `<<<` before `<<=` / `<<`.
    lexer_rules::token_literal_rule(g, "op_triple_shl_eq", K::TripleShlEq, b"<<<=");
    lexer_rules::token_literal_rule(g, "op_triple_shl", K::TripleShl, b"<<<");
    lexer_rules::token_literal_rule(g, "op_shr_eq", K::ShrEq, b">>=");
    lexer_rules::token_literal_rule(g, "op_shl_eq", K::ShlEq, b"<<=");
    g.lexer_rule("op_shl", |g| {
        g.token(K::Shl, |g| {
            g.literal(b"<<");
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
            g.neg_lookahead(|g| {
                g.byte(b'<');
            });
        });
    });

    lexer_rules::token_literal_rule(g, "op_bitand_eq", K::BitAndEq, b"&=");
    lexer_rules::token_literal_rule(g, "op_bitor_eq", K::BitOrEq, b"|=");
    lexer_rules::token_literal_rule(g, "op_bitxor_eq", K::BitXorEq, b"^=");
    g.lexer_rule("op_bitand", |g| {
        g.token(K::BitAnd, |g| {
            g.byte(b'&');
            g.neg_lookahead(|g| {
                g.byte(b'&');
            });
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });
    g.lexer_rule("op_bitor", |g| {
        g.token(K::BitOr, |g| {
            g.byte(b'|');
            g.neg_lookahead(|g| {
                g.byte(b'|');
            });
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });
    g.lexer_rule("op_bitxor", |g| {
        g.token(K::BitXor, |g| {
            g.byte(b'^');
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });

    lexer_rules::token_byte_rule(g, "eq", K::Eq, b'=');

    // Common operators as dedicated tokens (helps expression grammar).
    lexer_rules::token_literal_rule(g, "op_or_or", K::OrOr, b"||");
    // LeekScript also allows word-operators `or` / `and` (java fixtures).
    lexer_rules::versioned_keyword_rule(g, "op_or_word", K::OrOr, b"or");
    lexer_rules::token_literal_rule(g, "op_and_and", K::AndAnd, b"&&");
    lexer_rules::versioned_keyword_rule(g, "op_and_word", K::AndAnd, b"and");
    lexer_rules::token_literal_rule(g, "op_eqeqeq", K::EqEqEq, b"===");
    lexer_rules::token_literal_rule(g, "op_noteqeq", K::NotEqEq, b"!==");
    lexer_rules::token_literal_rule(g, "op_eqeq", K::EqEq, b"==");
    lexer_rules::token_literal_rule(g, "op_noteq", K::NotEq, b"!=");
    lexer_rules::token_literal_rule(g, "op_lte", K::Lte, b"<=");
    lexer_rules::token_literal_rule(g, "op_gte", K::Gte, b">=");
    g.lexer_rule("op_lt", |g| {
        g.token(K::Lt, |g| {
            g.byte(b'<');
            g.neg_lookahead(|g| {
                g.byte(b'<');
            });
        })
    });
    g.lexer_rule("op_gt", |g| {
        g.token(K::Gt, |g| {
            g.byte(b'>');
        })
    });
    lexer_rules::token_literal_rule(g, "op_plus_eq", K::PlusEq, b"+=");
    lexer_rules::token_literal_rule(g, "op_minus_eq", K::MinusEq, b"-=");
    lexer_rules::token_literal_rule(g, "op_star_eq", K::StarEq, b"*=");
    lexer_rules::token_literal_rule(g, "op_slash_eq", K::SlashEq, b"/=");
    lexer_rules::token_literal_rule(g, "op_percent_eq", K::PercentEq, b"%=");
    lexer_rules::token_literal_rule(g, "op_plusplus", K::PlusPlus, b"++");
    lexer_rules::token_literal_rule(g, "op_minusminus", K::MinusMinus, b"--");
    lexer_rules::token_byte_rule(g, "op_plus", K::Plus, b'+');
    g.lexer_rule("op_minus", |g| {
        g.token(K::Minus, |g| {
            g.byte(b'-');
            // Don't steal the `->` arrow token.
            g.neg_lookahead(|g| {
                g.byte(b'>');
            });
        })
    });
    lexer_rules::token_byte_rule(g, "op_star", K::Star, b'*');
    lexer_rules::token_byte_rule(g, "op_slash", K::Slash, b'/');
    lexer_rules::token_byte_rule(g, "op_percent", K::Percent, b'%');
    g.lexer_rule("op_bang", |g| {
        g.token(K::Bang, |g| {
            g.byte(b'!');
            // Don't steal `!=` / `!==` (see `op_noteq` / `op_noteqeq`).
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        })
    });

    g.lexer_rule("ident", |g| {
        g.token(K::Ident, |g| {
            g.neg_lookahead(|g| {
                ident_reserved_word_shape(g);
            });
            ident_start(g);
            g.zero_or_more(|g| {
                ident_cont(g);
            });
        });
    });
}

#[cfg(not(feature = "grammar-v4-only"))]
fn reserved_word_then_not_ident_cont(g: &mut GrammarBuilder, word: &'static [u8]) {
    lexer_rules::ascii_ci_bytes(g, word);
    g.neg_lookahead(|g| {
        ident_cont(g);
    });
}

/// Blocks `ident` when the next code unit sequence is a reserved word (Java `LexicalParser`).
fn ident_reserved_word_shape(g: &mut GrammarBuilder) {
    #[cfg(feature = "grammar-v4-only")]
    ident_reserved_word_shape_v4(g);

    #[cfg(not(feature = "grammar-v4-only"))]
    ident_reserved_word_shape_dyn(g);
}

#[cfg(feature = "grammar-v4-only")]
fn ident_reserved_word_shape_v4(g: &mut GrammarBuilder) {
    sipha::choices!(
        g,
        |g| {
            g.choice_literals(V3_LEX_RESERVED);
        },
        |g| {
            g.if_flag(FLAG_EXP_EXCEPTIONS);
            g.choice_literals(EXP_IDENT_EXCEPTIONS);
        },
        |g| {
            g.if_flag(FLAG_EXP_MODULES);
            g.choice_literals(EXP_IDENT_MODULES);
        },
        |g| {
            g.if_flag(FLAG_EXP_LEXICAL_CONST);
            g.literal(b"const");
        },
        |g| {
            g.if_flag(FLAG_EXP_GOTO);
            g.literal(b"goto");
        },
        |g| {
            g.if_flag(FLAG_EXP_MATCH);
            g.literal(b"match");
        },
    );
    g.neg_lookahead(|g| {
        ident_cont(g);
    });
}

#[cfg(not(feature = "grammar-v4-only"))]
fn ident_reserved_word_shape_dyn(g: &mut GrammarBuilder) {
    g.choice3(
        |g| {
            g.if_flag(FLAG_V4);
            sipha::choices!(
                g,
                |g| {
                    g.choice_literals(V3_LEX_RESERVED);
                },
                |g| {
                    g.if_flag(FLAG_EXP_EXCEPTIONS);
                    g.choice_literals(EXP_IDENT_EXCEPTIONS);
                },
                |g| {
                    g.if_flag(FLAG_EXP_MODULES);
                    g.choice_literals(EXP_IDENT_MODULES);
                },
                |g| {
                    g.if_flag(FLAG_EXP_LEXICAL_CONST);
                    g.literal(b"const");
                },
                |g| {
                    g.if_flag(FLAG_EXP_GOTO);
                    g.literal(b"goto");
                },
                |g| {
                    g.if_flag(FLAG_EXP_MATCH);
                    g.literal(b"match");
                },
            );
            g.neg_lookahead(|g| {
                ident_cont(g);
            });
        },
        |g| {
            g.if_flag(FLAG_V3);
            g.if_not_flag(FLAG_V4);
            g.choice_literals(V3_LEX_RESERVED);
            g.neg_lookahead(|g| {
                ident_cont(g);
            });
        },
        |g| {
            g.if_not_flag(FLAG_V3);
            sipha::choices!(
                g,
                |g| {
                    reserved_word_then_not_ident_cont(g, b"and");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"as");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"break");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"continue");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"do");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"else");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"false");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"for");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"function");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"global");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"if");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"in");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"not");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"null");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"or");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"return");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"true");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"var");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"while");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"xor");
                },
                |g| {
                    reserved_word_then_not_ident_cont(g, b"include");
                },
            );
        },
    );
}

/// Continuation after the initial digit / `.\d` prefix in the `number` lexer rule.
fn number_literal_tail(g: &mut GrammarBuilder) {
    g.zero_or_more(|g| {
        g.choice(
            |g| {
                g.byte(b'.');
                g.neg_lookahead(|g| {
                    g.byte(b'.');
                });
            },
            |g| number_char(g),
        );
    });
}

fn number_char(g: &mut GrammarBuilder) {
    g.choice6(
        |g| {
            g.class(classes::DIGIT);
        },
        |g| {
            g.byte(b'_');
        },
        |g| {
            // 0x, 0b, exponents, etc. (ASCII a-z/A-Z)
            g.choice(
                |g| {
                    g.char_range('A', 'Z');
                },
                |g| {
                    g.char_range('a', 'z');
                },
            );
        },
        |g| {
            // Common suffix used in fixtures (e.g. `12$`).
            g.byte(b'$');
        },
        |g| {
            // Sign for exponent parts (we don't validate placement here).
            g.choice(
                |g| {
                    g.byte(b'+');
                },
                |g| {
                    g.byte(b'-');
                },
            );
        },
        |g| {
            leek_identifier_letters(g);
        },
    );
}

fn ident_start(g: &mut GrammarBuilder) {
    g.choice3(
        |g| {
            g.class(classes::IDENT_START);
        },
        |g| {
            g.char_range('A', 'Z');
        },
        |g| {
            leek_identifier_letters(g);
        },
    );
}

/// Supplementary letters allowed in Java `LexicalParser` identifiers and numeric literals.
fn leek_identifier_letters(g: &mut GrammarBuilder) {
    g.choice6(
        |g| {
            g.char_range('À', 'Ö');
        },
        |g| {
            g.char_range('à', 'ö');
        },
        |g| {
            g.char_range('Ø', 'Ý');
        },
        |g| {
            g.char_range('ø', 'ý');
        },
        |g| {
            g.char_range('Œ', 'œ');
        },
        |g| {
            g.char('ÿ');
        },
    );
}

fn ident_cont(g: &mut GrammarBuilder) {
    g.choice3(
        |g| ident_start(g),
        |g| {
            g.class(classes::DIGIT);
        },
        |g| {
            g.byte(b'_');
        },
    );
}

/// `in` keyword with identifier boundary so `integer` does not lex as `in` + `teger`.
fn lexer_kw_in(g: &mut GrammarBuilder) {
    #[cfg(feature = "grammar-v4-only")]
    {
        g.token(K::InKw, |g| {
            g.literal(b"in");
            g.not_followed_by(|g| ident_cont(g));
        });
    }
    #[cfg(not(feature = "grammar-v4-only"))]
    {
        // Same as `versioned_keyword`, plus a boundary so `in` is not a prefix of longer idents.
        g.choice(
            |g| {
                g.if_flag(FLAG_V3);
                g.token(K::InKw, |g| {
                    g.literal(b"in");
                    g.not_followed_by(|g| ident_cont(g));
                });
            },
            |g| {
                g.if_not_flag(FLAG_V3);
                g.token(K::InKw, |g| {
                    lexer_rules::ascii_ci_bytes(g, b"in");
                    g.not_followed_by(|g| ident_cont(g));
                });
            },
        );
    }
}

fn quoted_string(g: &mut GrammarBuilder, quote: u8) {
    g.byte(quote);
    g.zero_or_more(|g| {
        g.choice(
            |g| {
                g.byte(b'\\');
                g.any_char();
            },
            |g| {
                g.neg_lookahead(|g| {
                    g.byte(quote);
                });
                g.any_char();
            },
        );
    });
    g.byte(quote);
}
