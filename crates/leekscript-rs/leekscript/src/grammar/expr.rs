//! Expressions: precedence levels, primaries, lambdas, and literals.
use super::GRule;
use super::cfg_flags;
use crate::parse::version::{FLAG_IN_CALL_ARG_LIST, FLAG_IN_SET_LITERAL};
use crate::syntax::kinds::Node;
use sipha::parse::expr as sipha_expr;
use sipha::prelude::parse::GrammarChoiceFn;
use sipha::prelude::*;

pub fn define(g: &mut GrammarBuilder) {
    // `>>` / `>>>` are ambiguous with nested generic type closers (`Map<..., Map<...>>`).
    // We lex `>` always, then parse shift ops as sequences so types can use `>>` naturally.
    g.parser_rule(GRule::OpShr.as_str(), |g| {
        g.call_rule(GRule::OpGt);
        g.call_rule(GRule::OpGt);
    });
    g.parser_rule(GRule::OpUshr.as_str(), |g| {
        g.call_rule(GRule::OpGt);
        g.call_rule(GRule::OpGt);
        g.call_rule(GRule::OpGt);
    });

    g.parser_rule(GRule::Expr.as_str(), |g| {
        g.node(Node::Expr, |g| {
            g.call_rule(GRule::Assign);
        });
    });

    // Precedence matches `leekscript-java` `Operators.getPriority` (higher number = tighter binding).
    // Outermost levels bind loosest: assign (0) … power (12) … unary/postfix.
    //
    // assign := ternary ( assign_op expr )?
    g.parser_rule(GRule::Assign.as_str(), |g| {
        g.call_rule(GRule::Ternary);
        g.optional(|g| {
            g.call_rule(GRule::AssignOp);
            g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                g.call_rule(GRule::Expr);
            });
        });
    });

    // ternary := or_coalesce ("?" expr ":" expr)?
    g.parser_rule(GRule::Ternary.as_str(), |g| {
        g.call_rule(GRule::OrCoalesce);
        g.optional(|g| {
            g.node(Node::TernaryExpr, |g| {
                g.call_rule(GRule::OpQuestion);
                g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                    g.call_rule(GRule::Expr);
                });
                g.call_rule(GRule::Colon);
                g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                    g.call_rule(GRule::Expr);
                });
            });
        });
    });

    // `*=` / `**=` share a leading `*`; `<<=` / `<<<=` share `<`; `>>=` / `>>>=` share `>`.
    g.parser_rule(GRule::AssignOpStar.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::OpStarStarEq);
            },
            |g| {
                g.call_rule(GRule::OpStarEq);
            },
        );
    });
    g.parser_rule(GRule::AssignOpLt.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::OpTripleShlEq);
            },
            |g| {
                g.call_rule(GRule::OpShlEq);
            },
        );
    });
    g.parser_rule(GRule::AssignOpGt.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::OpShrEq);
            },
            |g| {
                g.call_rule(GRule::OpUshrEq);
            },
        );
    });

    g.parser_rule(GRule::AssignOp.as_str(), |g| {
        g.byte_dispatch(
            vec![
                (
                    CharClass::from_byte(b'='),
                    Box::new(|g| {
                        g.call_rule(GRule::Eq);
                    }),
                ),
                (
                    CharClass::from_byte(b'?'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpCoalesceEq);
                    }),
                ),
                (
                    CharClass::from_byte(b'*'),
                    Box::new(|g| {
                        g.call_rule(GRule::AssignOpStar);
                    }),
                ),
                (
                    CharClass::from_byte(b'+'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpPlusEq);
                    }),
                ),
                (
                    CharClass::from_byte(b'-'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpMinusEq);
                    }),
                ),
                (
                    CharClass::from_byte(b'/'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpSlashEq);
                    }),
                ),
                (
                    CharClass::from_byte(b'%'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpPercentEq);
                    }),
                ),
                (
                    CharClass::from_byte(b'\\'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpBackslashEq);
                    }),
                ),
                (
                    CharClass::from_byte(b'<'),
                    Box::new(|g| {
                        g.call_rule(GRule::AssignOpLt);
                    }),
                ),
                (
                    CharClass::from_byte(b'>'),
                    Box::new(|g| {
                        g.call_rule(GRule::AssignOpGt);
                    }),
                ),
                (
                    CharClass::from_byte(b'&'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpBitandEq);
                    }),
                ),
                (
                    CharClass::from_byte(b'|'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpBitorEq);
                    }),
                ),
                (
                    CharClass::from_byte(b'^'),
                    Box::new(|g| {
                        g.call_rule(GRule::OpBitxorEq);
                    }),
                ),
            ],
            None,
        );
    });

    // or / ?? (both precedence 2 in Java)
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::OrCoalesce.as_str(),
            lower_level_name: GRule::LogicalXor.as_str(),
            ops: &[
                GRule::OpOrOr.as_str(),
                GRule::OpOrWord.as_str(),
                GRule::OpCoalesce.as_str(),
            ],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::LogicalXor.as_str(),
            lower_level_name: GRule::LogicalAnd.as_str(),
            ops: &[GRule::KwXor.as_str()],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // logical_and = bitwise_or ( "&&" bitwise_or )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::LogicalAnd.as_str(),
            lower_level_name: GRule::BitwiseOr.as_str(),
            ops: &[GRule::OpAndAnd.as_str(), GRule::OpAndWord.as_str()],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::BitwiseOr.as_str(),
            lower_level_name: GRule::BitwiseXor.as_str(),
            ops: &[GRule::OpBitor.as_str()],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::BitwiseXor.as_str(),
            lower_level_name: GRule::BitwiseBitand.as_str(),
            ops: &[GRule::OpBitxor.as_str()],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::BitwiseBitand.as_str(),
            lower_level_name: GRule::Equality.as_str(),
            ops: &[GRule::OpBitand.as_str()],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    g.parser_rule(GRule::NotIn.as_str(), |g| {
        g.call_rule(GRule::KwNot);
        g.call_rule(GRule::KwIn);
    });

    // relational = shift ( (<|>|<=|>=|instanceof|in|not in) shift )*
    // `as` is handled as `postfix_with_as` (type cast) like leekscript-java.
    //
    // Inside set literals (`< … >`), [`FLAG_IN_SET_LITERAL`] is set so a bare `>` ends the literal
    // instead of parsing as greater-than (e.g. `var i = <1, 2> setPut(i, 3)`).
    g.parser_rule(GRule::Relational.as_str(), |g| {
        g.call_rule(GRule::Shift);
        g.zero_or_more(|g| {
            g.node(Node::BinaryExpr, |g| {
                sipha::choices!(
                    g,
                    |g| {
                        g.call_rule(GRule::OpLte);
                        g.call_rule(GRule::Shift);
                    },
                    |g| {
                        g.call_rule(GRule::OpGte);
                        g.call_rule(GRule::Shift);
                    },
                    |g| {
                        g.call_rule(GRule::OpLt);
                        g.call_rule(GRule::Shift);
                    },
                    |g| {
                        g.if_not_flag(FLAG_IN_SET_LITERAL);
                        g.call_rule(GRule::OpGt);
                        g.call_rule(GRule::Shift);
                    },
                    |g| {
                        g.call_rule(GRule::KwInstanceof);
                        g.call_rule(GRule::Shift);
                    },
                    |g| {
                        g.call_rule(GRule::KwIn);
                        g.call_rule(GRule::Shift);
                    },
                    |g| {
                        g.call_rule(GRule::NotIn);
                        g.call_rule(GRule::Shift);
                    },
                );
            });
        });
    });

    // `is` / `is not` — between relational and `==` (Java word-operators).
    g.parser_rule(GRule::IsCompare.as_str(), |g| {
        g.call_rule(GRule::Relational);
        g.zero_or_more(|g| {
            g.node(Node::BinaryExpr, |g| {
                g.call_rule(GRule::KwIs);
                g.optional(|g| {
                    g.call_rule(GRule::KwNot);
                });
                g.call_rule(GRule::Relational);
            });
        });
    });

    // equality = is_compare ( (===|!==|==|!=) is_compare )*
    //
    // Use Sipha's [`left_assoc_infix_level`](sipha::parse::expr::left_assoc_infix_level) only — do
    // **not** encode equality with a left-recursive `Equality → … Equality …` rule: that can make
    // the parse engine/graph build diverge or thrash. The CST uses sibling `BinaryExpr` nodes (op +
    // rhs); the VM lowers that shape in [`CompileCtx::try_compile_infix_chain_on_parts`].
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::Equality.as_str(),
            lower_level_name: GRule::IsCompare.as_str(),
            ops: &[
                GRule::OpEqeqeq.as_str(),
                GRule::OpNoteqeq.as_str(),
                GRule::OpEqeq.as_str(),
                GRule::OpNoteq.as_str(),
            ],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // shift = additive ( (<<|>>|>>>) additive )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::Shift.as_str(),
            lower_level_name: GRule::Additive.as_str(),
            ops: &[
                GRule::OpTripleShl.as_str(),
                GRule::OpShl.as_str(),
                GRule::OpShr.as_str(),
                GRule::OpUshr.as_str(),
            ],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // additive = multiplicative ( (+|-) multiplicative )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::Additive.as_str(),
            lower_level_name: GRule::Multiplicative.as_str(),
            ops: &[GRule::OpPlus.as_str(), GRule::OpMinus.as_str()],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // multiplicative = power ( (*|/|%|\) power )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: GRule::Multiplicative.as_str(),
            lower_level_name: GRule::Power.as_str(),
            ops: &[
                GRule::OpStar.as_str(),
                GRule::OpSlash.as_str(),
                GRule::OpPercent.as_str(),
                GRule::OpBackslash.as_str(),
            ],
            node_kind: &Node::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::right_assoc_infix_level(
        g,
        GRule::Power.as_str(),
        GRule::Unary.as_str(),
        GRule::OpStarStar.as_str(),
        &Node::BinaryExpr,
        None,
        None,
    );

    // Prefer `++` / `--` over unary `+` / `-` so `++i` is pre-increment, not `+(+i)`.
    g.parser_rule(GRule::UnaryPlusPrefixed.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::OpPlusplus);
            },
            |g| {
                g.call_rule(GRule::OpPlus);
            },
        );
    });
    g.parser_rule(GRule::UnaryMinusPrefixed.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::OpMinusminus);
            },
            |g| {
                g.call_rule(GRule::OpMinus);
            },
        );
    });

    g.parser_rule(GRule::Unary.as_str(), |g| {
        g.choice(
            |g| {
                g.node(Node::UnaryExpr, |g| {
                    g.byte_dispatch(
                        vec![
                            (
                                CharClass::from_byte(b'!'),
                                Box::new(|g| {
                                    g.call_rule(GRule::OpBang);
                                }),
                            ),
                            (
                                CharClass::from_byte(b'n').union(CharClass::from_byte(b'N')),
                                Box::new(|g| {
                                    g.call_rule(GRule::KwNot);
                                }),
                            ),
                            (
                                CharClass::from_byte(b'+'),
                                Box::new(|g| {
                                    g.call_rule(GRule::UnaryPlusPrefixed);
                                }),
                            ),
                            (
                                CharClass::from_byte(b'-'),
                                Box::new(|g| {
                                    g.call_rule(GRule::UnaryMinusPrefixed);
                                }),
                            ),
                            (
                                CharClass::from_byte(b'~'),
                                Box::new(|g| {
                                    g.call_rule(GRule::OpTilde);
                                }),
                            ),
                            (
                                CharClass::from_byte(b't'),
                                Box::new(|g| {
                                    cfg_flags::v3(g);
                                    g.call_rule(GRule::KwTypeof);
                                }),
                            ),
                            (
                                CharClass::from_byte(b'a'),
                                Box::new(|g| {
                                    cfg_flags::v3(g);
                                    g.call_rule(GRule::KwAwait);
                                }),
                            ),
                            (
                                CharClass::from_byte(b'y'),
                                Box::new(|g| {
                                    cfg_flags::v3(g);
                                    g.call_rule(GRule::KwYield);
                                }),
                            ),
                        ],
                        None,
                    );
                    g.call_rule(GRule::Unary);
                });
            },
            |g| {
                g.call_rule(GRule::PostfixWithAs);
            },
        );
    });

    // postfix := primary postfix_suffix*
    sipha_expr::postfix_chain(
        g,
        GRule::Postfix.as_str(),
        GRule::Primary.as_str(),
        GRule::PostfixSuffix.as_str(),
    );

    // Postfix chain, then `as Type` casts (Java `AS` precedence).
    g.parser_rule(GRule::PostfixWithAs.as_str(), |g| {
        g.call_rule(GRule::Postfix);
        g.zero_or_more(|g| {
            g.node(Node::CastExpr, |g| {
                g.call_rule(GRule::KwAs);
                g.call_rule(GRule::LsType);
            });
        });
    });

    g.parser_rule(GRule::PostfixSuffix.as_str(), |g| {
        g.byte_dispatch(
            vec![
                (
                    CharClass::from_byte(b'('),
                    Box::new(|g| {
                        g.call_rule(GRule::CallExprSuffix);
                    }),
                ),
                (
                    CharClass::from_byte(b'['),
                    Box::new(|g| {
                        g.call_rule(GRule::IndexExprSuffix);
                    }),
                ),
                (
                    CharClass::from_byte(b'.'),
                    Box::new(|g| {
                        g.call_rule(GRule::MemberExprSuffix);
                    }),
                ),
                (
                    CharClass::from_byte(b'+'),
                    Box::new(|g| {
                        g.call_rule(GRule::PostfixIncrSuffix);
                    }),
                ),
                (
                    CharClass::from_byte(b'-'),
                    Box::new(|g| {
                        g.call_rule(GRule::PostfixIncrSuffix);
                    }),
                ),
                (
                    CharClass::from_byte(b'!'),
                    Box::new(|g| {
                        cfg_flags::v3(g);
                        g.node(Node::UnaryExpr, |g| {
                            g.call_rule(GRule::OpBang);
                        });
                    }),
                ),
            ],
            None,
        );
    });

    // `(` arg_list_opt `)` — function/method call suffix.
    g.parser_rule(GRule::CallExprSuffix.as_str(), |g| {
        g.node(Node::CallExpr, |g| {
            g.call_rule(GRule::Lparen);
            g.call_rule(GRule::ArgListOpt);
            g.call_rule(GRule::Rparen);
        });
    });

    // `[` ... `]` — indexing / slice entry.
    g.parser_rule(GRule::IndexExprSuffix.as_str(), |g| {
        g.node(Node::IndexExpr, |g| {
            g.call_rule(GRule::Lbracket);
            g.optional(|g| {
                g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                    // Either: expr
                    // Or: expr? ':' expr? (':' expr?)?   (slice)
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.optional(|g| {
                        g.call_rule(GRule::Colon);
                        g.optional(|g| {
                            g.call_rule(GRule::Expr);
                        });
                        g.optional(|g| {
                            cfg_flags::v4(g);
                            g.call_rule(GRule::Colon);
                            g.optional(|g| {
                                g.call_rule(GRule::Expr);
                            });
                        });
                    });
                });
            });
            g.call_rule(GRule::Rbracket);
        });
    });

    // `.` ident | `class` | `super` (leekscript-java).
    g.parser_rule(GRule::MemberExprSuffix.as_str(), |g| {
        g.node(Node::MemberExpr, |g| {
            g.call_rule(GRule::Dot);
            g.choice3(
                |g| {
                    g.call_rule(GRule::Ident);
                },
                |g| {
                    g.call_rule(GRule::KwClass);
                },
                |g| {
                    g.call_rule(GRule::KwSuper);
                },
            );
        });
    });

    // Postfix ++/-- (common in `for` loops).
    g.parser_rule(GRule::PostfixIncrSuffix.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::OpPlusplus);
            },
            |g| {
                g.call_rule(GRule::OpMinusminus);
            },
        );
    });

    // arg_list_opt := arg_list?
    g.parser_rule(GRule::ArgListOpt.as_str(), |g| {
        g.optional(|g| {
            g.call_rule(GRule::ArgList);
        });
    });

    // arg_list := expr ("," expr)* ","?
    g.parser_rule(GRule::ArgList.as_str(), |g| {
        g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
            g.with_flags(&[FLAG_IN_CALL_ARG_LIST], &[], |g| {
                g.call_rule(GRule::Expr);
                g.zero_or_more(|g| {
                    g.call_rule(GRule::Comma);
                    g.call_rule(GRule::Expr);
                });
                g.optional(|g| {
                    g.call_rule(GRule::Comma);
                });
            });
        });
    });

    g.parser_rule(GRule::ParenExpr.as_str(), |g| {
        g.node(Node::ParenExpr, |g| {
            g.call_rule(GRule::Lparen);
            g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                g.call_rule(GRule::Expr);
            });
            g.call_rule(GRule::Rparen);
        });
    });

    // Arrow lambda: `ident => expr`, `ident => real expr` (return type), or `(…) => block`.
    //
    // This overlaps with `ident`, so we use `lookahead` in `primary` to pick it
    // only when an `arrow` follows.
    // Untyped `x` must win over type named `x` (see `a.filter(x -> …)`).
    // Typed body: `=> T expr` uses `lambda_return_type` (no bare `ident` as `T`) so
    // `dp => dp["x"]` still parses as untyped lambda.
    // Typed params before bare names so `(V x) =>` is `V x`, not param `V` then junk `x`.
    g.parser_rule(GRule::LambdaParam.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::LsType);
                g.call_rule(GRule::Ident);
            },
            |g| {
                g.call_rule(GRule::Ident);
            },
        );
    });

    g.parser_rule(GRule::LambdaHead.as_str(), |g| {
        sipha::choices!(
            g,
            |g| {
                // Java suite allows `x, y -> ...` without parentheses; disabled in call arg lists so
                // `f(a, x -> y)` is two arguments (see [`FLAG_IN_CALL_ARG_LIST`]).
                g.if_not_flag(FLAG_IN_CALL_ARG_LIST);
                g.lookahead(|g| {
                    g.call_rule(GRule::LambdaParam);
                    g.call_rule(GRule::Comma);
                });
                g.call_rule(GRule::LambdaParam);
                g.zero_or_more(|g| {
                    g.call_rule(GRule::Comma);
                    g.call_rule(GRule::LambdaParam);
                });
                g.optional(|g| {
                    g.call_rule(GRule::Comma);
                });
            },
            |g| {
                g.call_rule(GRule::LambdaParam);
            },
            |g| {
                g.call_rule(GRule::Lparen);
                g.optional(|g| {
                    g.call_rule(GRule::LambdaParam);
                    g.zero_or_more(|g| {
                        g.call_rule(GRule::Comma);
                        g.call_rule(GRule::LambdaParam);
                    });
                    g.optional(|g| {
                        g.call_rule(GRule::Comma);
                    });
                });
                g.call_rule(GRule::Rparen);
            }
        );
    });

    g.parser_rule(GRule::LambdaExpr.as_str(), |g| {
        g.node(Node::LambdaExpr, |g| {
            g.optional(|g| {
                g.call_rule(GRule::LambdaHead);
            });
            g.call_rule(GRule::Arrow);
            g.choice(
                |g| {
                    g.call_rule(GRule::LambdaReturnType);
                    g.choice(
                        |g| {
                            g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                                g.call_rule(GRule::Expr);
                            });
                        },
                        |g| {
                            g.call_rule(GRule::Block);
                        },
                    );
                },
                |g| {
                    g.choice(
                        |g| {
                            g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                                g.call_rule(GRule::Expr);
                            });
                        },
                        |g| {
                            g.call_rule(GRule::Block);
                        },
                    );
                },
            );
        });
    });

    // `]` or `[` — interval endpoint (Java `LeekInterval` open/closed bounds).
    g.parser_rule(GRule::IntervalClosingBracket.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::Rbracket);
            },
            |g| {
                g.call_rule(GRule::Lbracket);
            },
        );
    });

    // Array / map contents only (no `..` intervals — those use `bracket_interval_body`).
    g.parser_rule(GRule::BracketListOrMapInner.as_str(), |g| {
        g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
            g.call_rule(GRule::Expr);
            g.optional(|g| {
                g.choice(
                    |g| {
                        // Bracket-map literals (`[k: v]`) exist before v4 in the Java suite.
                        cfg_flags::v2(g);
                        g.call_rule(GRule::Colon);
                        g.node(Node::BracketMapExpr, |g| {
                            g.call_rule(GRule::Expr);
                            g.zero_or_more(|g| {
                                g.call_rule(GRule::Comma);
                                g.call_rule(GRule::Expr);
                                g.call_rule(GRule::Colon);
                                g.call_rule(GRule::Expr);
                            });
                            g.optional(|g| {
                                g.call_rule(GRule::Comma);
                            });
                        });
                    },
                    |g| {
                        // Java fixtures allow missing commas in arrays (`[1 2 3]`).
                        g.optional(|g| {
                            g.call_rule(GRule::Comma);
                        });
                        g.call_rule(GRule::Expr);
                        g.zero_or_more(|g| {
                            g.optional(|g| {
                                g.call_rule(GRule::Comma);
                            });
                            g.call_rule(GRule::Expr);
                        });
                        g.optional(|g| {
                            g.call_rule(GRule::Comma);
                        });
                    },
                );
            });
        });
    });

    // After `[`: `[..]`, `[..2]`, `[1..2]`, `[1..2[`, … (aligned with `readArrayOrMapOrInterval` /
    // `readInterval` in leekscript-java).
    g.parser_rule(GRule::BracketIntervalBody.as_str(), |g| {
        g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
            sipha::choices!(
                g,
                |g| {
                    g.call_rule(GRule::Dotdot);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                },
                |g| {
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::Dotdot);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                },
            );
        });
    });

    g.parser_rule(GRule::ArrayExpr.as_str(), |g| {
        g.call_rule(GRule::Lbracket);
        sipha::choices!(
            g,
            |g| {
                // Empty bracket-map literal (`[:]`) exists before v4 in the Java suite.
                cfg_flags::v2(g);
                g.node(Node::BracketMapExpr, |g| {
                    g.call_rule(GRule::Colon);
                });
                g.call_rule(GRule::Rbracket);
            },
            |g| {
                g.node(Node::IntervalExpr, |g| {
                    g.call_rule(GRule::BracketIntervalBody);
                    g.call_rule(GRule::IntervalClosingBracket);
                });
            },
            |g| {
                g.optional(|g| {
                    g.call_rule(GRule::BracketListOrMapInner);
                });
                g.call_rule(GRule::Rbracket);
            },
        );
    });

    g.parser_rule(GRule::AnonFunctionExpr.as_str(), |g| {
        g.node(Node::AnonFunctionExpr, |g| {
            g.call_rule(GRule::KwFunction);
            g.optional(|g| {
                cfg_flags::exp_templates(g);
                g.call_rule(GRule::DeclTemplateParams);
            });
            g.call_rule(GRule::Lparen);
            g.optional(|g| {
                g.call_rule(GRule::FunctionFnParam);
                g.zero_or_more(|g| {
                    g.call_rule(GRule::Comma);
                    g.call_rule(GRule::FunctionFnParam);
                });
                g.optional(|g| {
                    g.call_rule(GRule::Comma);
                });
            });
            g.call_rule(GRule::Rparen);
            g.optional(|g| {
                g.call_rule(GRule::Arrow);
                g.call_rule(GRule::LsType);
            });
            g.call_rule(GRule::Block);
        });
    });

    // `]..[`, `]..2]`, `]1..2[`, … (aligned with `BRACKET_RIGHT` + `DOT_DOT` in leekscript-java).
    g.parser_rule(GRule::IntervalRbracketExpr.as_str(), |g| {
        g.node(Node::IntervalExpr, |g| {
            g.call_rule(GRule::Rbracket);
            sipha::choices!(
                g,
                |g| {
                    g.call_rule(GRule::Dotdot);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                },
                |g| {
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::Dotdot);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                },
            );
            g.call_rule(GRule::IntervalClosingBracket);
        });
    });

    // object := "{" (ident ":" expr ((","?) ident ":" expr)* ","?)? "}"
    g.parser_rule(GRule::ObjectExpr.as_str(), |g| {
        g.node(Node::ObjectExpr, |g| {
            g.call_rule(GRule::Lbrace);
            g.optional(|g| {
                g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                    g.call_rule(GRule::Ident);
                    g.call_rule(GRule::Colon);
                    g.call_rule(GRule::Expr);
                    g.zero_or_more(|g| {
                        // Java fixtures allow missing commas between fields.
                        g.optional(|g| {
                            g.call_rule(GRule::Comma);
                        });
                        g.call_rule(GRule::Ident);
                        g.call_rule(GRule::Colon);
                        g.call_rule(GRule::Expr);
                    });
                    g.optional(|g| {
                        g.call_rule(GRule::Comma);
                    });
                });
            });
            g.call_rule(GRule::Rbrace);
        });
    });

    // set := "<" (expr ("," expr)* ","?)? ">"
    g.parser_rule(GRule::SetExpr.as_str(), |g| {
        g.node(Node::SetExpr, |g| {
            g.call_rule(GRule::OpLt);
            g.optional(|g| {
                g.with_flags(&[FLAG_IN_SET_LITERAL], &[], |g| {
                    g.call_rule(GRule::Expr);
                    g.zero_or_more(|g| {
                        g.call_rule(GRule::Comma);
                        g.call_rule(GRule::Expr);
                    });
                    g.optional(|g| {
                        g.call_rule(GRule::Comma);
                    });
                });
            });
            g.call_rule(GRule::OpGt);
        });
    });

    // `(` … `)` — parenthesized expr or lambda; disambiguate via `=>` before falling back to
    // [`paren_expr`].
    g.parser_rule(GRule::PrimaryLparen.as_str(), |g| {
        g.choice(
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::LambdaHead);
                    g.call_rule(GRule::Arrow);
                });
                g.call_rule(GRule::LambdaExpr);
            },
            |g| {
                g.call_rule(GRule::ParenExpr);
            },
        );
    });

    // Alternatives not covered by [`primary`]'s leading-byte dispatch. Kept as its own
    // [`parser_rule`] so bytecode is emitted with trivia auto-skip enabled; an inlined fallback
    // inside [`byte_dispatch`] would omit `skip()` before `call`s (breaking `=> real` lambdas).
    g.parser_rule(GRule::PrimaryFallback.as_str(), |g| {
        sipha::choices!(
            g,
            |g| {
                g.lookahead(|g| {
                    sipha::choices!(
                        g,
                        |g| {
                            g.call_rule(GRule::LambdaHead);
                            g.call_rule(GRule::Arrow);
                        },
                        |g| {
                            // Allow zero-arg lambdas: `-> expr`
                            g.call_rule(GRule::Arrow);
                        }
                    );
                });
                g.call_rule(GRule::LambdaExpr);
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwFunction);
                    g.optional(|g| {
                        cfg_flags::exp_templates(g);
                        g.call_rule(GRule::DeclTemplateParams);
                    });
                    g.call_rule(GRule::Lparen);
                });
                g.call_rule(GRule::AnonFunctionExpr);
            },
            // `eval(...)`: lexer `EVAL` in Java; not a dedicated form in leekscript-java `WordCompiler`.
            |g| {
                cfg_flags::v3(g);
                g.lookahead(|g| {
                    g.call_rule(GRule::KwEval);
                    g.call_rule(GRule::Lparen);
                });
                g.node(Node::CallExpr, |g| {
                    g.call_rule(GRule::KwEval);
                    g.call_rule(GRule::Lparen);
                    g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.call_rule(GRule::Rparen);
                });
            },
            |g| {
                g.call_rule(GRule::Pi);
            },
            |g| {
                g.call_rule(GRule::Infinity);
            },
            |g| {
                g.call_rule(GRule::KwTrue);
            },
            |g| {
                g.call_rule(GRule::KwFalse);
            },
            |g| {
                g.call_rule(GRule::KwNull);
            },
            |g| {
                g.call_rule(GRule::KwThis);
            },
            |g| {
                cfg_flags::v2(g);
                g.node(Node::SuperExpr, |g| {
                    g.call_rule(GRule::KwSuper);
                });
            },
            |g| {
                cfg_flags::v2(g);
                g.node(Node::ClassRefExpr, |g| {
                    g.call_rule(GRule::KwClass);
                });
            },
            // `instanceof Class` / `instanceof Array` — type keywords are not `ident`.
            |g| {
                cfg_flags::v2(g);
                g.node(Node::BuiltinTypeNameExpr, |g| {
                    g.call_rule(GRule::KwClassType);
                });
            },
            |g| {
                cfg_flags::v2(g);
                g.node(Node::BuiltinTypeNameExpr, |g| {
                    g.call_rule(GRule::KwArray);
                    g.optional(|g| {
                        g.call_rule(GRule::GenericTypeArgs);
                    });
                });
            },
            |g| {
                cfg_flags::v2(g);
                g.node(Node::BuiltinTypeNameExpr, |g| {
                    g.call_rule(GRule::KwObject);
                    g.optional(|g| {
                        g.call_rule(GRule::GenericTypeArgs);
                    });
                });
            },
            |g| {
                cfg_flags::v2(g);
                g.node(Node::BuiltinTypeNameExpr, |g| {
                    g.call_rule(GRule::KwMap);
                    g.optional(|g| {
                        g.call_rule(GRule::GenericTypeArgs);
                    });
                });
            },
            |g| {
                cfg_flags::v3(g);
                g.node(Node::BuiltinStringifyExpr, |g| {
                    g.call_rule(GRule::KwStringType);
                });
            },
            |g| {
                // Reference expression: `@x` / `@(expr)` / `@[...]` (used by Java suite for by-ref semantics).
                g.node(Node::RefExpr, |g| {
                    g.call_rule(GRule::OpAt);
                    sipha::choices!(
                        g,
                        |g| {
                            g.call_rule(GRule::Ident);
                        },
                        |g| {
                            g.call_rule(GRule::ParenExpr);
                        },
                        |g| {
                            g.call_rule(GRule::ArrayExpr);
                        },
                    );
                });
            },
            |g| {
                g.call_rule(GRule::IfExpr);
            },
            |g| {
                g.call_rule(GRule::NewExpr);
            },
            |g| {
                g.call_rule(GRule::Ident);
            },
        );
    });

    // primary := literals | ident | paren | array | object | set | lambda | …
    //
    // Fast-path [`byte_dispatch`](GrammarBuilder::byte_dispatch) on leading byte (after trivia)
    // for common starters; everything else goes through [`primary_fallback`].
    g.parser_rule(GRule::Primary.as_str(), |g| {
        let digit_or_dot = CharClass::EMPTY.with_range(b'0', b'9').with_byte(b'.');

        let mut arms: Vec<(CharClass, GrammarChoiceFn)> = Vec::with_capacity(9);
        arms.push((
            digit_or_dot,
            Box::new(|g| {
                g.call_rule(GRule::Number);
            }),
        ));
        arms.push((
            CharClass::from_byte(b'"'),
            Box::new(|g| {
                g.call_rule(GRule::String);
            }),
        ));
        arms.push((
            CharClass::from_byte(b'\''),
            Box::new(|g| {
                g.call_rule(GRule::String);
            }),
        ));
        arms.push((
            CharClass::from_byte(b'['),
            Box::new(|g| {
                g.node(Node::ArrayExpr, |g| {
                    g.call_rule(GRule::ArrayExpr);
                });
            }),
        ));
        arms.push((
            CharClass::from_byte(b']'),
            Box::new(|g| {
                g.call_rule(GRule::IntervalRbracketExpr);
            }),
        ));
        arms.push((
            CharClass::from_byte(b'{'),
            Box::new(|g| {
                g.call_rule(GRule::ObjectExpr);
            }),
        ));
        arms.push((
            CharClass::from_byte(b'<'),
            Box::new(|g| {
                g.call_rule(GRule::SetExpr);
            }),
        ));
        arms.push((
            CharClass::from_byte(b'('),
            Box::new(|g| {
                g.call_rule(GRule::PrimaryLparen);
            }),
        ));

        g.byte_dispatch(
            arms,
            Some(Box::new(|g| {
                g.call_rule(GRule::PrimaryFallback);
            })),
        );
    });

    g.parser_rule(GRule::IfExpr.as_str(), |g| {
        g.node(Node::IfExpr, |g| {
            g.call_rule(GRule::KwIf);
            // Accept both `if (cond)` and `if cond` in expression position.
            g.choice(
                |g| {
                    g.call_rule(GRule::Lparen);
                    g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.call_rule(GRule::Rparen);
                },
                |g| {
                    g.with_flags(&[], &[FLAG_IN_SET_LITERAL], |g| {
                        g.call_rule(GRule::Expr);
                    });
                },
            );
            // Inline the block shape here so we don't accidentally fall into
            // `{ ... }` as an object literal in expression contexts.
            g.node(Node::Block, |g| {
                g.call_rule(GRule::Lbrace);
                g.zero_or_more(|g| {
                    g.call_rule(GRule::Stmt);
                });
                g.call_rule(GRule::Rbrace);
            });
            g.optional(|g| {
                g.call_rule(GRule::KwElse);
                g.node(Node::Block, |g| {
                    g.call_rule(GRule::Lbrace);
                    g.zero_or_more(|g| {
                        g.call_rule(GRule::Stmt);
                    });
                    g.call_rule(GRule::Rbrace);
                });
            });
        });
    });

    g.parser_rule(GRule::NewExpr.as_str(), |g| {
        g.node(Node::NewExpr, |g| {
            g.call_rule(GRule::KwNew);
            // `new Array` / `new Array()` — `Array` is the `ArrayKw` token, not `ident`.
            sipha::choices!(
                g,
                |g| {
                    g.call_rule(GRule::Ident);
                },
                |g| {
                    cfg_flags::v2(g);
                    g.call_rule(GRule::KwArray);
                },
                |g| {
                    cfg_flags::v2(g);
                    g.call_rule(GRule::KwObject);
                },
                |g| {
                    cfg_flags::v2(g);
                    g.call_rule(GRule::KwMap);
                },
            );
            g.optional(|g| {
                g.call_rule(GRule::CallExprSuffix);
            });
        });
    });
}
