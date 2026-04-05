use super::cfg_flags;
use crate::syntax::kinds::K;
use sipha::parse::expr as sipha_expr;
use sipha::prelude::*;

pub fn define(g: &mut GrammarBuilder) {
    // `>>` / `>>>` are ambiguous with nested generic type closers (`Map<..., Map<...>>`).
    // We lex `>` always, then parse shift ops as sequences so types can use `>>` naturally.
    g.parser_rule("op_shr", |g| {
        g.call("op_gt");
        g.call("op_gt");
    });
    g.parser_rule("op_ushr", |g| {
        g.call("op_gt");
        g.call("op_gt");
        g.call("op_gt");
    });

    g.parser_rule("expr", |g| {
        g.node(K::Expr, |g| {
            g.call("assign");
        });
    });

    // Precedence matches `leekscript-java` `Operators.getPriority` (higher number = tighter binding).
    // Outermost levels bind loosest: assign (0) … power (12) … unary/postfix.
    //
    // assign := ternary ( assign_op expr )?
    g.parser_rule("assign", |g| {
        g.call("ternary");
        g.optional(|g| {
            g.call("assign_op");
            g.call("expr");
        });
    });

    // ternary := or_coalesce ("?" expr ":" expr)?
    g.parser_rule("ternary", |g| {
        g.call("or_coalesce");
        g.optional(|g| {
            g.node(K::TernaryExpr, |g| {
                g.call("op_question");
                g.call("expr");
                g.call("colon");
                g.call("expr");
            });
        });
    });

    g.parser_rule("assign_op", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.call("eq");
            }),
            Box::new(|g| {
                g.call("op_coalesce_eq");
            }),
            Box::new(|g| {
                g.call("op_star_star_eq");
            }),
            Box::new(|g| {
                g.call("op_plus_eq");
            }),
            Box::new(|g| {
                g.call("op_minus_eq");
            }),
            Box::new(|g| {
                g.call("op_star_eq");
            }),
            Box::new(|g| {
                g.call("op_slash_eq");
            }),
            Box::new(|g| {
                g.call("op_percent_eq");
            }),
            Box::new(|g| {
                g.call("op_backslash_eq");
            }),
            Box::new(|g| {
                g.call("op_triple_shl_eq");
            }),
            Box::new(|g| {
                g.call("op_shl_eq");
            }),
            Box::new(|g| {
                g.call("op_shr_eq");
            }),
            Box::new(|g| {
                g.call("op_ushr_eq");
            }),
            Box::new(|g| {
                g.call("op_bitand_eq");
            }),
            Box::new(|g| {
                g.call("op_bitor_eq");
            }),
            Box::new(|g| {
                g.call("op_bitxor_eq");
            }),
        ]);
    });

    // or / ?? (both precedence 2 in Java)
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "or_coalesce",
            lower_level_name: "logical_xor",
            ops: &["op_or_or", "op_or_word", "op_coalesce"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "logical_xor",
            lower_level_name: "logical_and",
            ops: &["kw_xor"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // logical_and = bitwise_or ( "&&" bitwise_or )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "logical_and",
            lower_level_name: "bitwise_or",
            ops: &["op_and_and", "op_and_word"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "bitwise_or",
            lower_level_name: "bitwise_xor",
            ops: &["op_bitor"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "bitwise_xor",
            lower_level_name: "bitwise_bitand",
            ops: &["op_bitxor"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "bitwise_bitand",
            lower_level_name: "equality",
            ops: &["op_bitand"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    g.parser_rule("not_in", |g| {
        g.call("kw_not");
        g.call("kw_in");
    });

    // relational = shift ( (<|>|<=|>=|instanceof|in|not in) shift )*
    // `as` is handled as `postfix_with_as` (type cast) like leekscript-java.
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "relational",
            lower_level_name: "shift",
            ops: &[
                "op_lte",
                "op_gte",
                "op_lt",
                "op_gt",
                "kw_instanceof",
                "kw_in",
                "not_in",
            ],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // `is` / `is not` — between relational and `==` (Java word-operators).
    g.parser_rule("is_compare", |g| {
        g.call("relational");
        g.zero_or_more(|g| {
            g.node(K::BinaryExpr, |g| {
                g.call("kw_is");
                g.optional(|g| {
                    g.call("kw_not");
                });
                g.call("relational");
            });
        });
    });

    // equality = is_compare ( (===|!==|==|!=) is_compare )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "equality",
            lower_level_name: "is_compare",
            ops: &["op_eqeqeq", "op_noteqeq", "op_eqeq", "op_noteq"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // shift = additive ( (<<|>>|>>>) additive )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "shift",
            lower_level_name: "additive",
            ops: &["op_triple_shl", "op_shl", "op_shr", "op_ushr"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // additive = multiplicative ( (+|-) multiplicative )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "additive",
            lower_level_name: "multiplicative",
            ops: &["op_plus", "op_minus"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    // multiplicative = power ( (*|/|%|\) power )*
    sipha_expr::left_assoc_infix_level(
        g,
        &sipha_expr::LeftAssocInfixLevel {
            level_name: "multiplicative",
            lower_level_name: "power",
            ops: &["op_star", "op_slash", "op_percent", "op_backslash"],
            node_kind: &K::BinaryExpr,
            wrapper_kind: None,
            rhs_field: None,
            rhs_wrapper_kind: None,
        },
    );

    sipha_expr::right_assoc_infix_level(
        g,
        "power",
        "unary",
        "op_star_star",
        &K::BinaryExpr,
        None,
        None,
    );

    g.parser_rule("unary", |g| {
        g.choice(
            |g| {
                g.node(K::UnaryExpr, |g| {
                    g.choices(vec![
                        Box::new(|g| {
                            g.call("op_bang");
                        }),
                        Box::new(|g| {
                            g.call("kw_not");
                        }),
                        // Prefer `++` / `--` over unary `+` / `-` so `++i` is pre-increment, not `+(+i)`.
                        Box::new(|g| {
                            g.call("op_plusplus");
                        }),
                        Box::new(|g| {
                            g.call("op_minusminus");
                        }),
                        Box::new(|g| {
                            g.call("op_plus");
                        }),
                        Box::new(|g| {
                            g.call("op_minus");
                        }),
                        Box::new(|g| {
                            g.call("op_tilde");
                        }),
                        // `typeof` / `await` / `yield`: Java lexer tokens only — not handled in
                        // leekscript-java `WordCompiler` / expression parser.
                        Box::new(|g| {
                            cfg_flags::v3(g);
                            g.call("kw_typeof");
                        }),
                        Box::new(|g| {
                            cfg_flags::v3(g);
                            g.call("kw_await");
                        }),
                        Box::new(|g| {
                            cfg_flags::v3(g);
                            g.call("kw_yield");
                        }),
                    ]);
                    g.call("unary");
                });
            },
            |g| {
                g.call("postfix_with_as");
            },
        );
    });

    // postfix := primary postfix_suffix*
    sipha_expr::postfix_chain(g, "postfix", "primary", "postfix_suffix");

    // Postfix chain, then `as Type` casts (Java `AS` precedence).
    g.parser_rule("postfix_with_as", |g| {
        g.call("postfix");
        g.zero_or_more(|g| {
            g.node(K::CastExpr, |g| {
                g.call("kw_as");
                g.call("ls_type");
            });
        });
    });

    g.parser_rule("postfix_suffix", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.call("call_expr_suffix");
            }),
            Box::new(|g| {
                g.call("index_expr_suffix");
            }),
            Box::new(|g| {
                g.call("member_expr_suffix");
            }),
            Box::new(|g| {
                g.call("postfix_incr_suffix");
            }),
            Box::new(|g| {
                cfg_flags::v3(g);
                g.node(K::UnaryExpr, |g| {
                    g.call("op_bang");
                });
            }),
        ]);
    });

    // `(` arg_list_opt `)` — function/method call suffix.
    g.parser_rule("call_expr_suffix", |g| {
        g.node(K::CallExpr, |g| {
            g.call("lparen");
            g.call("arg_list_opt");
            g.call("rparen");
        });
    });

    // `[` ... `]` — indexing / slice entry.
    g.parser_rule("index_expr_suffix", |g| {
        g.node(K::IndexExpr, |g| {
            g.call("lbracket");
            g.optional(|g| {
                // Either: expr
                // Or: expr? ':' expr? (':' expr?)?   (slice)
                g.optional(|g| {
                    g.call("expr");
                });
                g.optional(|g| {
                    g.call("colon");
                    g.optional(|g| {
                        g.call("expr");
                    });
                    g.optional(|g| {
                        cfg_flags::v4(g);
                        g.call("colon");
                        g.optional(|g| {
                            g.call("expr");
                        });
                    });
                });
            });
            g.call("rbracket");
        });
    });

    // `.` ident | `class` | `super` (leekscript-java).
    g.parser_rule("member_expr_suffix", |g| {
        g.node(K::MemberExpr, |g| {
            g.call("dot");
            g.choice3(
                |g| {
                    g.call("ident");
                },
                |g| {
                    g.call("kw_class");
                },
                |g| {
                    g.call("kw_super");
                },
            );
        });
    });

    // Postfix ++/-- (common in `for` loops).
    g.parser_rule("postfix_incr_suffix", |g| {
        g.choice(
            |g| {
                g.call("op_plusplus");
            },
            |g| {
                g.call("op_minusminus");
            },
        );
    });

    // arg_list_opt := arg_list?
    g.parser_rule("arg_list_opt", |g| {
        g.optional(|g| {
            g.call("arg_list");
        });
    });

    // arg_list := expr ("," expr)* ","?
    g.parser_rule("arg_list", |g| {
        g.call("expr");
        g.zero_or_more(|g| {
            g.call("comma");
            g.call("expr");
        });
        g.optional(|g| {
            g.call("comma");
        });
    });

    g.parser_rule("paren_expr", |g| {
        g.node(K::ParenExpr, |g| {
            g.call("lparen");
            g.call("expr");
            g.call("rparen");
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
    g.parser_rule("lambda_param", |g| {
        g.choice(
            |g| {
                g.call("ls_type");
                g.call("ident");
            },
            |g| {
                g.call("ident");
            },
        );
    });

    g.parser_rule("lambda_head", |g| {
        g.choice(
            |g| {
                g.call("lambda_param");
            },
            |g| {
                g.call("lparen");
                g.optional(|g| {
                    g.call("lambda_param");
                    g.zero_or_more(|g| {
                        g.call("comma");
                        g.call("lambda_param");
                    });
                    g.optional(|g| {
                        g.call("comma");
                    });
                });
                g.call("rparen");
            },
        );
    });

    g.parser_rule("lambda_expr", |g| {
        g.node(K::LambdaExpr, |g| {
            g.call("lambda_head");
            g.call("arrow");
            g.choice(
                |g| {
                    g.call("lambda_return_type");
                    g.choice(
                        |g| {
                            g.call("expr");
                        },
                        |g| {
                            g.call("block");
                        },
                    );
                },
                |g| {
                    g.choice(
                        |g| {
                            g.call("expr");
                        },
                        |g| {
                            g.call("block");
                        },
                    );
                },
            );
        });
    });

    // `]` or `[` — interval endpoint (Java `LeekInterval` open/closed bounds).
    g.parser_rule("interval_closing_bracket", |g| {
        g.choice(
            |g| {
                g.call("rbracket");
            },
            |g| {
                g.call("lbracket");
            },
        );
    });

    // Array / map contents only (no `..` intervals — those use `bracket_interval_body`).
    g.parser_rule("bracket_list_or_map_inner", |g| {
        g.call("expr");
        g.optional(|g| {
            g.choice(
                |g| {
                    cfg_flags::v4(g);
                    g.call("colon");
                    g.node(K::BracketMapExpr, |g| {
                        g.call("expr");
                        g.zero_or_more(|g| {
                            g.call("comma");
                            g.call("expr");
                            g.call("colon");
                            g.call("expr");
                        });
                        g.optional(|g| {
                            g.call("comma");
                        });
                    });
                },
                |g| {
                    g.call("comma");
                    g.call("expr");
                    g.zero_or_more(|g| {
                        g.call("comma");
                        g.call("expr");
                    });
                    g.optional(|g| {
                        g.call("comma");
                    });
                },
            );
        });
    });

    // After `[`: `[..]`, `[..2]`, `[1..2]`, `[1..2[`, … (aligned with `readArrayOrMapOrInterval` /
    // `readInterval` in leekscript-java).
    g.parser_rule("bracket_interval_body", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.call("dotdot");
                g.optional(|g| {
                    g.call("expr");
                });
            }),
            Box::new(|g| {
                g.call("expr");
                g.call("dotdot");
                g.optional(|g| {
                    g.call("expr");
                });
            }),
        ]);
    });

    g.parser_rule("array_expr", |g| {
        g.call("lbracket");
        g.choices(vec![
            Box::new(|g| {
                cfg_flags::v4(g);
                g.node(K::BracketMapExpr, |g| {
                    g.call("colon");
                });
                g.call("rbracket");
            }),
            Box::new(|g| {
                g.node(K::IntervalExpr, |g| {
                    g.call("bracket_interval_body");
                    g.call("interval_closing_bracket");
                });
            }),
            Box::new(|g| {
                g.optional(|g| {
                    g.call("bracket_list_or_map_inner");
                });
                g.call("rbracket");
            }),
        ]);
    });

    g.parser_rule("anon_function_expr", |g| {
        g.node(K::AnonFunctionExpr, |g| {
            g.call("kw_function");
            g.optional(|g| {
                cfg_flags::exp_templates(g);
                g.call("decl_template_params");
            });
            g.call("lparen");
            g.optional(|g| {
                g.call("function_fn_param");
                g.zero_or_more(|g| {
                    g.call("comma");
                    g.call("function_fn_param");
                });
                g.optional(|g| {
                    g.call("comma");
                });
            });
            g.call("rparen");
            g.optional(|g| {
                g.call("arrow");
                g.call("ls_type");
            });
            g.call("block");
        });
    });

    // `]..[`, `]..2]`, `]1..2[`, … (aligned with `BRACKET_RIGHT` + `DOT_DOT` in leekscript-java).
    g.parser_rule("interval_rbracket_expr", |g| {
        g.node(K::IntervalExpr, |g| {
            g.call("rbracket");
            g.choices(vec![
                Box::new(|g| {
                    g.call("dotdot");
                    g.optional(|g| {
                        g.call("expr");
                    });
                }),
                Box::new(|g| {
                    g.call("expr");
                    g.call("dotdot");
                    g.optional(|g| {
                        g.call("expr");
                    });
                }),
            ]);
            g.call("interval_closing_bracket");
        });
    });

    // object := "{" (ident ":" expr ("," ident ":" expr)* ","?)? "}"
    g.parser_rule("object_expr", |g| {
        g.node(K::ObjectExpr, |g| {
            g.call("lbrace");
            g.optional(|g| {
                g.call("ident");
                g.call("colon");
                g.call("expr");
                g.zero_or_more(|g| {
                    g.call("comma");
                    g.call("ident");
                    g.call("colon");
                    g.call("expr");
                });
                g.optional(|g| {
                    g.call("comma");
                });
            });
            g.call("rbrace");
        });
    });

    // set := "<" (expr ("," expr)* ","?)? ">"
    g.parser_rule("set_expr", |g| {
        g.node(K::SetExpr, |g| {
            g.call("op_lt");
            g.optional(|g| {
                g.call("expr");
                g.zero_or_more(|g| {
                    g.call("comma");
                    g.call("expr");
                });
                g.optional(|g| {
                    g.call("comma");
                });
            });
            g.call("op_gt");
        });
    });

    // primary := literals | ident | paren | array | object | set | lambda | …
    g.parser_rule("primary", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.lookahead(|g| {
                    g.call("lambda_head");
                    g.call("arrow");
                });
                g.call("lambda_expr");
            }),
            Box::new(|g| {
                g.lookahead(|g| {
                    g.call("kw_function");
                    g.optional(|g| {
                        cfg_flags::exp_templates(g);
                        g.call("decl_template_params");
                    });
                    g.call("lparen");
                });
                g.call("anon_function_expr");
            }),
            // `eval(...)`: lexer `EVAL` in Java; not a dedicated form in leekscript-java `WordCompiler`.
            Box::new(|g| {
                cfg_flags::v3(g);
                g.lookahead(|g| {
                    g.call("kw_eval");
                    g.call("lparen");
                });
                g.node(K::CallExpr, |g| {
                    g.call("kw_eval");
                    g.call("lparen");
                    g.call("expr");
                    g.call("rparen");
                });
            }),
            Box::new(|g| {
                g.call("number");
            }),
            Box::new(|g| {
                g.call("string");
            }),
            Box::new(|g| {
                g.call("pi");
            }),
            Box::new(|g| {
                g.call("infinity");
            }),
            Box::new(|g| {
                g.call("kw_true");
            }),
            Box::new(|g| {
                g.call("kw_false");
            }),
            Box::new(|g| {
                g.call("kw_null");
            }),
            Box::new(|g| {
                g.call("kw_this");
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.node(K::SuperExpr, |g| {
                    g.call("kw_super");
                });
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.node(K::ClassRefExpr, |g| {
                    g.call("kw_class");
                });
            }),
            // `instanceof Class` / `instanceof Array` — type keywords are not `ident`.
            Box::new(|g| {
                cfg_flags::v2(g);
                g.node(K::BuiltinTypeNameExpr, |g| {
                    g.call("kw_class_type");
                });
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.node(K::BuiltinTypeNameExpr, |g| {
                    g.call("kw_array");
                    g.optional(|g| {
                        g.call("generic_type_args");
                    });
                });
            }),
            Box::new(|g| {
                cfg_flags::v3(g);
                g.node(K::BuiltinStringifyExpr, |g| {
                    g.call("kw_string_type");
                });
            }),
            Box::new(|g| {
                g.call("interval_rbracket_expr");
            }),
            Box::new(|g| {
                g.call("if_expr");
            }),
            Box::new(|g| {
                g.call("new_expr");
            }),
            Box::new(|g| {
                g.call("paren_expr");
            }),
            Box::new(|g| {
                g.node(K::ArrayExpr, |g| {
                    g.call("array_expr");
                });
            }),
            Box::new(|g| {
                g.call("object_expr");
            }),
            Box::new(|g| {
                g.call("set_expr");
            }),
            Box::new(|g| {
                g.call("ident");
            }),
        ]);
    });

    g.parser_rule("if_expr", |g| {
        g.node(K::IfExpr, |g| {
            g.call("kw_if");
            // Accept both `if (cond)` and `if cond` in expression position.
            g.choice(
                |g| {
                    g.call("lparen");
                    g.call("expr");
                    g.call("rparen");
                },
                |g| {
                    g.call("expr");
                },
            );
            // Inline the block shape here so we don't accidentally fall into
            // `{ ... }` as an object literal in expression contexts.
            g.node(K::Block, |g| {
                g.call("lbrace");
                g.zero_or_more(|g| {
                    g.call("stmt");
                });
                g.call("rbrace");
            });
            g.optional(|g| {
                g.call("kw_else");
                g.node(K::Block, |g| {
                    g.call("lbrace");
                    g.zero_or_more(|g| {
                        g.call("stmt");
                    });
                    g.call("rbrace");
                });
            });
        });
    });

    g.parser_rule("new_expr", |g| {
        g.node(K::NewExpr, |g| {
            g.call("kw_new");
            // `new Array` / `new Array()` — `Array` is the `ArrayKw` token, not `ident`.
            g.choice(
                |g| {
                    g.call("ident");
                },
                |g| {
                    cfg_flags::v2(g);
                    g.call("kw_array");
                },
            );
            g.optional(|g| {
                g.call("call_expr_suffix");
            });
        });
    });
}
