//! Keywords, operators, literals, and identifier shapes (lexer + shared tokens).
//!
//! Registration order is **semantic**: sipha’s lexer trie and alternative ordering depend on it
//! (e.g. `integer` before `int`; longer `op_*` / `kw_*` literals before shorter prefixes).
//!
//! ## Keyword rules (`kw_*`)
//!
//! Spelling is the UTF-8 bytes passed to sipha (ASCII for all current keywords). Version columns:
//! **V** = [`versioned_keyword_rule`](super::lexer_rules::versioned_keyword_rule) (v1/v2
//! case-insensitive, v3+ case-sensitive); **V2** = only from v2; **V3** = only from v3; **EXP** =
//! experimental flag on v3 (see [`super::lexer_keyword_batch`]);
//! **M** = v4 + `FLAG_EXP_MATCH` for `match`.
//!
//! | Rule | Word | Kind |
//! |------|------|------|
//! | `kw_return` | `return` | V |
//! | `kw_var` | `var` | V |
//! | `kw_let` | `let` | V3 + `FLAG_EXP_LET` |
//! | `kw_function` … `kw_include` | … | V |
//! | `kw_match` | `match` | M |
//! | `kw_class` … `kw_new` | … | V2 |
//! | `kw_as`, `kw_xor` … `kw_null` | … | V / V2 where marked in source |
//! | `kw_switch` … `kw_yield` | … | V3 / V3+EXP ([`super::lexer_keyword_batch`]) |
//! | `kw_in` | `in` | custom boundary (see [`lexer_kw_in`]) |
//!
//! ## Symbol rules (punctuation + `op_*`)
//!
//! Registration order is in [`define_lexer_punctuation`] and [`define_lexer_operators`]. Rules that
//! need `neg_lookahead` / `not_followed_by` stay as explicit `g.lexer_rule` bodies in source.
//!
//! ```text
//! semi ;     comma ,     colon :     dotdot ..     dot .     arrow => | ->
//! lparen (   rparen )    lbracket [  rbracket ]    lbrace {  rbrace }
//! op_question ?   op_coalesce ??  op_coalesce_eq ??=   op_star **  op_star_eq **=
//! op_backslash \  op_backslash_eq \=   op_ushr >>>=   op_triple_shl <<< / <<<=   op_shr >>=
//! op_shl << (custom)   op_bit*   eq =   || && === !== == != <= >= < >  (and word or/and)
//! += -= *= /= %= ++ -- + - * / % !  (custom minus/bang)
//! ```
use super::GRule;
use super::cfg_flags;
use super::lexer_keyword_batch;
use super::lexer_rules;
use crate::parse::version::{
    FLAG_EXP_EXCEPTIONS, FLAG_EXP_GOTO, FLAG_EXP_LEXICAL_CONST, FLAG_EXP_MATCH, FLAG_EXP_MODULES,
    FLAG_V1, FLAG_V3, FLAG_V4,
};
use crate::syntax::kinds::{Lex, Node};
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
    define_lexer_trivia(g);
    define_lexer_keywords(g);
    define_lexer_misc_single_byte_ops(g);
    define_lexer_numbers_and_strings(g);
    define_lexer_punctuation(g);
    define_lexer_operators(g);
    define_lexer_ident(g);
}

fn define_lexer_trivia(g: &mut GrammarBuilder) {
    // One `TRIVIA` syntax node wrapping each skipped run; children are `WS` / `LINE_COMMENT` /
    // `BLOCK_COMMENT` trivia tokens. (A plain `trivia(Node::Trivia, …)` around `call("ws")` would
    // emit overlapping trivia leaves: inner `WS` then outer `TRIVIA` for the same span.)
    g.lexer_rule(GRule::Trivia.as_str(), |g| {
        g.optional(|g| {
            g.node(Node::Trivia, |g| {
                g.one_or_more(|g| {
                    g.choice3(
                        |g| {
                            g.call_rule(GRule::Ws);
                        },
                        |g| {
                            g.call_rule(GRule::LineComment);
                        },
                        |g| {
                            g.call_rule(GRule::BlockComment);
                        },
                    );
                });
            });
        });
    });

    g.lexer_rule(GRule::Ws.as_str(), |g| {
        g.trivia(Lex::Ws, |g| {
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

    g.lexer_rule(GRule::LineComment.as_str(), |g| {
        g.trivia(Lex::LineComment, |g| {
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

    g.lexer_rule(GRule::BlockComment.as_str(), |g| {
        g.trivia(Lex::BlockComment, |g| {
            g.literal(b"/*");

            // LeekScript v1 special-case: `/*/` is accepted as an immediate comment close.
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
        });
    });
}

/// Keywords: v1/v2 case-insensitive, v3+ case-sensitive ([`lexer_rules::versioned_keyword_rule`]).
/// `ident` is registered last so keywords win when both could match.
fn define_lexer_keywords(g: &mut GrammarBuilder) {
    lexer_kw_versioned!(g;
        (GRule::KwReturn, Lex::ReturnKw, b"return"),
        (GRule::KwVar, Lex::VarKw, b"var"),
    );
    g.lexer_rule_keywords_batch(lexer_keyword_batch::KW_LET);
    lexer_kw_versioned!(g;
        (GRule::KwFunction, Lex::FunctionKw, b"function"),
        (GRule::KwIf, Lex::IfKw, b"if"),
        (GRule::KwElse, Lex::ElseKw, b"else"),
        (GRule::KwFor, Lex::ForKw, b"for"),
        (GRule::KwWhile, Lex::WhileKw, b"while"),
        (GRule::KwDo, Lex::DoKw, b"do"),
        (GRule::KwBreak, Lex::BreakKw, b"break"),
        (GRule::KwContinue, Lex::ContinueKw, b"continue"),
        (GRule::KwInclude, Lex::IncludeKw, b"include"),
    );
    // Not in Java `LexicalParser`. Only a keyword for leekscript-rs v4+ sources.
    g.lexer_rule_keywords_batch(lexer_keyword_batch::KW_MATCH);
    lexer_kw_v2_if!(g;
        (GRule::KwClass, Lex::ClassKw, b"class"),
        (GRule::KwNew, Lex::NewKw, b"new"),
    );
    // `kw_in` uses an identifier boundary (see `lexer_kw_in`): plain `literal("in")` would win in
    // the lexer trie and split `integer` into `in` + `teger`. Longer `in*` keywords stay above.
    lexer_kw_versioned!(g;
        (GRule::KwAs, Lex::AsKw, b"as"),
    );
    lexer_kw_v2_if!(g;
        (GRule::KwInstanceof, Lex::InstanceofKw, b"instanceof"),
    );
    lexer_kw_versioned!(g;
        (GRule::KwXor, Lex::XorKw, b"xor"),
        (GRule::KwNot, Lex::NotKw, b"not"),
        (GRule::KwTrue, Lex::TrueKw, b"true"),
        (GRule::KwFalse, Lex::FalseKw, b"false"),
        (GRule::KwNull, Lex::NullKw, b"null"),
    );
    lexer_kw_v2_if!(g;
        (GRule::KwThis, Lex::ThisKw, b"this"),
        (GRule::KwSuper, Lex::SuperKw, b"super"),
    );
    g.lexer_rule_keywords_batch(lexer_keyword_batch::PLAIN_V3_SWITCH);
    lexer_kw_versioned!(g;
        (GRule::KwGlobal, Lex::GlobalKw, b"global"),
    );
    lexer_kw_v2_if!(g;
        (GRule::KwExtends, Lex::ExtendsKw, b"extends"),
        (GRule::KwPublic, Lex::PublicKw, b"public"),
        (GRule::KwPrivate, Lex::PrivateKw, b"private"),
        (GRule::KwProtected, Lex::ProtectedKw, b"protected"),
        (GRule::KwStatic, Lex::StaticKw, b"static"),
    );
    g.lexer_rule_keywords_batch(lexer_keyword_batch::PLAIN_V3_FINAL);
    lexer_kw_v2_if!(g;
        (GRule::KwConstructor, Lex::ConstructorKw, b"constructor"),
    );
    lexer_kw_versioned!(g;
        (GRule::KwIs, Lex::IsKw, b"is"),
    );

    // Type names (also reserved as identifiers in Java parity mode). `integer` before `int`.
    g.lexer_rule_keywords_batch(lexer_keyword_batch::PLAIN_V3_TYPE_NAMES);
    lexer_kw_v2_if!(g;
        (GRule::KwClassType, Lex::ClassTypeKw, b"Class"),
        (GRule::KwObject, Lex::ObjectKw, b"Object"),
        (GRule::KwArray, Lex::ArrayKw, b"Array"),
        (GRule::KwSetType, Lex::SetTypeKw, b"Set"),
        (GRule::KwMap, Lex::MapKw, b"Map"),
        (GRule::KwFunctionType, Lex::FunctionTypeKw, b"Function"),
        (GRule::KwIntervalType, Lex::IntervalKw, b"Interval"),
    );

    // Java `LexicalParser` v3-only reserved spellings (`TokenType` …). Order matches Java tables
    // where experimental keywords are interleaved (registration order matters).
    g.lexer_rule_keywords_batch(lexer_keyword_batch::PLAIN_V3_JAVA_BEFORE_KW_IN);
    // `in` must not match a prefix of `integer` / `instanceof` / `interface` / `int`.
    g.lexer_rule(GRule::KwIn.as_str(), lexer_kw_in);
    g.lexer_rule_keywords_batch(lexer_keyword_batch::PLAIN_V3_JAVA_AFTER_KW_IN);
}

fn define_lexer_misc_single_byte_ops(g: &mut GrammarBuilder) {
    g.lexer_rule(GRule::OpAt.as_str(), |g| {
        g.token(Lex::Operator, |g| {
            g.byte(b'@');
        });
    });

    g.lexer_rule(GRule::OpTilde.as_str(), |g| {
        g.token(Lex::Tilde, |g| {
            g.byte(b'~');
        });
    });
}

fn define_lexer_numbers_and_strings(g: &mut GrammarBuilder) {
    g.lexer_rule(GRule::Number.as_str(), |g| {
        g.token(Lex::Number, |g| {
            // Keep radix / suffix / exponent characters after a proper numeric
            // prefix. Do **not** start with a letter: otherwise words like `new`,
            // `null`, or `combo` are swallowed as NUMBER and keywords / `!=` /
            // identifiers break (see formatter / `!=` lexing).
            //
            // `+` / `-` are **not** part of the mantissa: `1+2` and `1+','+2` must
            // tokenize as separate operators (Java / LeekScript).
            //
            // Still avoid eating `..` (interval / dotdot) by forbidding a '.' that
            // is immediately followed by '.'.
            g.choice4(
                |g| {
                    // `0x` / `0X` hexadecimal (includes `e`/`E` as hex digits).
                    g.byte(b'0');
                    g.choice(
                        |g| {
                            g.byte(b'x');
                        },
                        |g| {
                            g.byte(b'X');
                        },
                    );
                    g.one_or_more(|g| hex_digit(g));
                    g.optional(|g| {
                        g.byte(b'.');
                        g.zero_or_more(|g| hex_digit(g));
                    });
                    g.optional(|g| {
                        g.choice(
                            |g| {
                                g.byte(b'p');
                            },
                            |g| {
                                g.byte(b'P');
                            },
                        );
                        g.optional(|g| {
                            g.choice(
                                |g| {
                                    g.byte(b'+');
                                },
                                |g| {
                                    g.byte(b'-');
                                },
                            );
                        });
                        g.one_or_more(|g| {
                            g.choice(
                                |g| {
                                    g.class(classes::DIGIT);
                                },
                                |g| {
                                    g.byte(b'_');
                                },
                            );
                        });
                    });
                },
                |g| {
                    // `0b` / `0B` binary.
                    g.byte(b'0');
                    g.choice(
                        |g| {
                            g.byte(b'b');
                        },
                        |g| {
                            g.byte(b'B');
                        },
                    );
                    g.one_or_more(|g| {
                        // Accept any digits here so invalid binary literals (`0b...7...`)
                        // are tokenized as a single NUMBER and rejected later (Java parity).
                        g.choice4(
                            |g| {
                                g.class(classes::DIGIT);
                            },
                            |g| {
                                g.byte(b'_');
                            },
                            |g| {
                                g.byte(b'.');
                            },
                            |g| {
                                g.class(classes::IDENT_START);
                            },
                        );
                    });
                },
                |g| {
                    g.class(classes::DIGIT);
                    g.zero_or_more(|g| decimal_mantissa_char(g));
                    decimal_number_literal_tail(g);
                    optional_decimal_exponent(g);
                },
                |g| {
                    // `.5` style (leading dot, digit required after)
                    g.byte(b'.');
                    g.class(classes::DIGIT);
                    g.zero_or_more(|g| decimal_mantissa_char(g));
                    decimal_number_literal_tail(g);
                    optional_decimal_exponent(g);
                },
            );
        });
    });

    // `break` / `continue` optional numeric level only. The general `number` rule also
    // accepts letters (radix/exponent), which would otherwise lex `for` as NUMBER after
    // `break` / `continue`, breaking the next `for (var …)` statement.
    g.lexer_rule(GRule::BreakContinueLevel.as_str(), |g| {
        g.token(Lex::Number, |g| {
            g.one_or_more(|g| {
                g.class(classes::DIGIT);
            });
        });
    });

    g.lexer_rule(GRule::String.as_str(), |g| {
        g.token(Lex::String, |g| {
            g.choice(|g| quoted_string(g, b'"'), |g| quoted_string(g, b'\''));
        });
    });

    g.lexer_rule(GRule::Pi.as_str(), |g| {
        g.token(Lex::Pi, |g| {
            g.char('π');
        });
    });

    g.lexer_rule(GRule::Infinity.as_str(), |g| {
        g.token(Lex::Infinity, |g| {
            g.char('∞');
        });
    });
}

fn define_lexer_punctuation(g: &mut GrammarBuilder) {
    lexer_token_byte!(g;
        (GRule::Semi, Lex::Semi, b';'),
        (GRule::Comma, Lex::Comma, b','),
        (GRule::Colon, Lex::Colon, b':'),
    );
    lexer_rules::token_literal_rule(g, GRule::Dotdot.as_str(), Lex::DotDot, b"..");

    g.lexer_rule(GRule::Dot.as_str(), |g| {
        g.token(Lex::Dot, |g| {
            // Member access `.` exists in modern fixture code (v2+).
            // Our version flags are mutually exclusive, so "v2+" is "not v1".
            cfg_flags::not_v1(g);
            g.byte(b'.');
        });
    });

    g.lexer_rule(GRule::Arrow.as_str(), |g| {
        g.token(Lex::Arrow, |g| {
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

    lexer_token_byte!(g;
        (GRule::Lparen, Lex::LParen, b'('),
        (GRule::Rparen, Lex::RParen, b')'),
        (GRule::Lbracket, Lex::LBracket, b'['),
        (GRule::Rbracket, Lex::RBracket, b']'),
        (GRule::Lbrace, Lex::LBrace, b'{'),
        (GRule::Rbrace, Lex::RBrace, b'}'),
    );
}

fn define_lexer_operators(g: &mut GrammarBuilder) {
    g.lexer_rule(GRule::OpQuestion.as_str(), |g| {
        g.token(Lex::Question, |g| {
            g.byte(b'?');
            g.neg_lookahead(|g| {
                g.byte(b'?');
            });
        });
    });

    lexer_token_literal!(g;
        (GRule::OpCoalesceEq, Lex::CoalesceEq, b"??="),
        (GRule::OpCoalesce, Lex::Coalesce, b"??"),
        (GRule::OpStarStarEq, Lex::StarStarEq, b"**="),
        (GRule::OpStarStar, Lex::StarStar, b"**"),
        (GRule::OpBackslashEq, Lex::BackslashEq, b"\\="),
    );
    g.lexer_rule(GRule::OpBackslash.as_str(), |g| {
        g.token(Lex::Backslash, |g| {
            g.byte(b'\\');
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });

    lexer_token_literal!(g;
        (GRule::OpUshrEq, Lex::UShrEq, b">>>="),
        // Java `LexicalParser` operator table includes `<<<=` / `<<<` before `<<=` / `<<`.
        (GRule::OpTripleShlEq, Lex::TripleShlEq, b"<<<="),
        (GRule::OpTripleShl, Lex::TripleShl, b"<<<"),
        (GRule::OpShrEq, Lex::ShrEq, b">>="),
        (GRule::OpShlEq, Lex::ShlEq, b"<<="),
    );
    g.lexer_rule(GRule::OpShl.as_str(), |g| {
        g.token(Lex::Shl, |g| {
            g.literal(b"<<");
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
            g.neg_lookahead(|g| {
                g.byte(b'<');
            });
        });
    });

    lexer_token_literal!(g;
        (GRule::OpBitandEq, Lex::BitAndEq, b"&="),
        (GRule::OpBitorEq, Lex::BitOrEq, b"|="),
        (GRule::OpBitxorEq, Lex::BitXorEq, b"^="),
    );
    g.lexer_rule(GRule::OpBitand.as_str(), |g| {
        g.token(Lex::BitAnd, |g| {
            g.byte(b'&');
            g.neg_lookahead(|g| {
                g.byte(b'&');
            });
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });
    g.lexer_rule(GRule::OpBitor.as_str(), |g| {
        g.token(Lex::BitOr, |g| {
            g.byte(b'|');
            g.neg_lookahead(|g| {
                g.byte(b'|');
            });
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });
    g.lexer_rule(GRule::OpBitxor.as_str(), |g| {
        g.token(Lex::BitXor, |g| {
            g.byte(b'^');
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        });
    });

    lexer_rules::token_byte_rule(g, GRule::Eq.as_str(), Lex::Eq, b'=');

    // Common operators as dedicated tokens (helps expression grammar).
    lexer_rules::token_literal_rule(g, GRule::OpOrOr.as_str(), Lex::OrOr, b"||");
    // LeekScript also allows word-operators `or` / `and` (java fixtures).
    lexer_kw_versioned!(g;
        (GRule::OpOrWord, Lex::OrOr, b"or"),
    );
    lexer_rules::token_literal_rule(g, GRule::OpAndAnd.as_str(), Lex::AndAnd, b"&&");
    lexer_kw_versioned!(g;
        (GRule::OpAndWord, Lex::AndAnd, b"and"),
    );
    lexer_token_literal!(g;
        (GRule::OpEqeqeq, Lex::EqEqEq, b"==="),
        (GRule::OpNoteqeq, Lex::NotEqEq, b"!=="),
        (GRule::OpEqeq, Lex::EqEq, b"=="),
        (GRule::OpNoteq, Lex::NotEq, b"!="),
        (GRule::OpLte, Lex::Lte, b"<="),
        (GRule::OpGte, Lex::Gte, b">="),
    );
    g.lexer_rule(GRule::OpLt.as_str(), |g| {
        g.token(Lex::Lt, |g| {
            g.byte(b'<');
            g.neg_lookahead(|g| {
                g.byte(b'<');
            });
        })
    });
    g.lexer_rule(GRule::OpGt.as_str(), |g| {
        g.token(Lex::Gt, |g| {
            g.byte(b'>');
        })
    });
    lexer_token_literal!(g;
        (GRule::OpPlusEq, Lex::PlusEq, b"+="),
        (GRule::OpMinusEq, Lex::MinusEq, b"-="),
        (GRule::OpStarEq, Lex::StarEq, b"*="),
        (GRule::OpSlashEq, Lex::SlashEq, b"/="),
        (GRule::OpPercentEq, Lex::PercentEq, b"%="),
        (GRule::OpPlusplus, Lex::PlusPlus, b"++"),
        (GRule::OpMinusminus, Lex::MinusMinus, b"--"),
    );
    lexer_token_byte!(g;
        (GRule::OpPlus, Lex::Plus, b'+'),
    );
    g.lexer_rule(GRule::OpMinus.as_str(), |g| {
        g.token(Lex::Minus, |g| {
            g.byte(b'-');
            // Don't steal the `->` arrow token.
            g.neg_lookahead(|g| {
                g.byte(b'>');
            });
        })
    });
    lexer_token_byte!(g;
        (GRule::OpStar, Lex::Star, b'*'),
        (GRule::OpSlash, Lex::Slash, b'/'),
        (GRule::OpPercent, Lex::Percent, b'%'),
    );
    g.lexer_rule(GRule::OpBang.as_str(), |g| {
        g.token(Lex::Bang, |g| {
            g.byte(b'!');
            // Don't steal `!=` / `!==` (see `op_noteq` / `op_noteqeq`).
            g.neg_lookahead(|g| {
                g.byte(b'=');
            });
        })
    });
}

fn define_lexer_ident(g: &mut GrammarBuilder) {
    g.lexer_rule(GRule::Ident.as_str(), |g| {
        g.token(Lex::Ident, |g| {
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

fn reserved_word_then_not_ident_cont(g: &mut GrammarBuilder, word: &'static [u8]) {
    lexer_rules::ascii_ci_bytes(g, word);
    g.neg_lookahead(|g| {
        ident_cont(g);
    });
}

/// Blocks `ident` when the next code unit sequence is a reserved word (Java `LexicalParser`).
fn ident_reserved_word_shape(g: &mut GrammarBuilder) {
    ident_reserved_word_shape_dyn(g);
}

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

fn hex_digit(g: &mut GrammarBuilder) {
    g.choice4(
        |g| {
            g.class(classes::DIGIT);
        },
        |g| {
            g.char_range('a', 'f');
        },
        |g| {
            g.char_range('A', 'F');
        },
        |g| {
            g.byte(b'_');
        },
    );
}

/// Mantissa / fractional characters for **decimal** literals: never `e`/`E` (exponent) and never
/// bare `+`/`-` (binary operators).
fn decimal_mantissa_char(g: &mut GrammarBuilder) {
    g.choice5(
        |g| {
            g.class(classes::DIGIT);
        },
        |g| {
            g.byte(b'_');
        },
        |g| {
            g.choice4(
                |g| {
                    g.char_range('A', 'D');
                },
                |g| {
                    g.char_range('F', 'Z');
                },
                |g| {
                    g.char_range('a', 'd');
                },
                |g| {
                    g.char_range('f', 'z');
                },
            );
        },
        |g| {
            // Common suffix used in fixtures (e.g. `12$`).
            g.byte(b'$');
        },
        |g| {
            leek_identifier_letters(g);
        },
    );
}

fn decimal_number_literal_tail(g: &mut GrammarBuilder) {
    g.zero_or_more(|g| {
        g.choice(
            |g| {
                g.byte(b'.');
                g.neg_lookahead(|g| {
                    g.byte(b'.');
                });
            },
            |g| decimal_mantissa_char(g),
        );
    });
}

/// Decimal scientific notation: `1e+3`, `1e-3`, `1E10` (not hex `0x…e…`).
fn optional_decimal_exponent(g: &mut GrammarBuilder) {
    g.optional(|g| {
        g.choice(
            |g| {
                g.byte(b'e');
            },
            |g| {
                g.byte(b'E');
            },
        );
        g.optional(|g| {
            g.choice(
                |g| {
                    g.byte(b'+');
                },
                |g| {
                    g.byte(b'-');
                },
            );
        });
        g.one_or_more(|g| {
            g.choice(
                |g| {
                    g.class(classes::DIGIT);
                },
                |g| {
                    g.byte(b'_');
                },
            );
        });
    });
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
    // Same as `versioned_keyword`, plus a boundary so `in` is not a prefix of longer idents.
    g.choice(
        |g| {
            g.if_flag(FLAG_V3);
            g.token(Lex::InKw, |g| {
                g.literal(b"in");
                g.not_followed_by(|g| ident_cont(g));
            });
        },
        |g| {
            g.if_not_flag(FLAG_V3);
            g.token(Lex::InKw, |g| {
                lexer_rules::ascii_ci_bytes(g, b"in");
                g.not_followed_by(|g| ident_cont(g));
            });
        },
    );
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
