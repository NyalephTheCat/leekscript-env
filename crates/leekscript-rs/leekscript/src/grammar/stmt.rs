use super::cfg_flags;
use crate::parse::version::FLAG_PARSE_RECOVERY;
use crate::syntax::kinds::K;
use sipha::prelude::parse::GrammarChoiceFn;
use sipha::prelude::*;

pub fn define(g: &mut GrammarBuilder) {
    // Start rule must be rule 0.
    g.parser_rule("start", |g| {
        g.node(K::Root, |g| {
            g.zero_or_more(|g| {
                g.choice(
                    |g| {
                        g.if_flag(FLAG_PARSE_RECOVERY);
                        g.recover_until("top_level_sync", |g| {
                            g.call("stmt");
                        });
                    },
                    |g| {
                        g.if_not_flag(FLAG_PARSE_RECOVERY);
                        g.call("stmt");
                    },
                );
            });
            g.skip();
        });
        g.end_of_input();
        g.accept();
    });

    g.parser_rule("stmt", |g| {
        let alts: Vec<GrammarChoiceFn> = vec![
            Box::new(|g: &mut GrammarBuilder| {
                g.call("include_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("return_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("break_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("continue_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("global_decl");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("else_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                g.call("switch_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("var_decl");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("function_decl");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("class_decl");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("if_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("for_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("do_while_stmt");
            }),
            // --- CST-only vs leekscript-java: the following statement shapes are NOT implemented
            // in `WordCompiler` today. Java only recognizes these spellings in `LexicalParser` /
            // `TokenType` (reserved words). We parse them here for token-stream / syntax-tree parity.
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                cfg_flags::vnext(g);
                g.call("try_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                cfg_flags::vnext(g);
                g.call("throw_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                cfg_flags::vnext(g);
                g.call("import_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                cfg_flags::vnext(g);
                g.call("export_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                cfg_flags::vnext(g);
                g.call("goto_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                cfg_flags::vnext(g);
                g.call("package_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                cfg_flags::vnext(g);
                g.call("const_decl");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("while_stmt");
            }),
            // `match` is not a Java lexer keyword; this is a leekscript-rs extension (v4+).
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v4(g);
                cfg_flags::vnext(g);
                g.call("match_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.node(K::EmptyStmt, |g| {
                    g.call("semi");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                // Expression statement fallback.
                g.node(K::Stmt, |g| {
                    g.call("expr");
                    g.optional(|g| {
                        g.call("semi");
                    });
                });
            }),
        ];
        g.choices(alts);
    });

    // Statement-boundary sync for `recover_until` at module scope: trivia, then `;` (consumed) or a
    // keyword that can start a top-level statement. Keywords use [`GrammarBuilder::lookahead`] so
    // we do not eat the keyword — the following `stmt` parse must see it.
    g.parser_rule("top_level_sync", |g| {
        g.skip();
        let alts: Vec<GrammarChoiceFn> = vec![
            Box::new(|g: &mut GrammarBuilder| {
                g.call("semi");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_function");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_class");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_var");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::vnext(g);
                    g.call("kw_let");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_global");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_if");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_for");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_while");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_do");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_return");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_break");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_continue");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_include");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    g.call("kw_else");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    g.call("kw_switch");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::vnext(g);
                    g.call("kw_try");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::vnext(g);
                    g.call("kw_throw");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::vnext(g);
                    g.call("kw_import");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::vnext(g);
                    g.call("kw_export");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::vnext(g);
                    g.call("kw_goto");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::vnext(g);
                    g.call("kw_package");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::vnext(g);
                    g.call("kw_const");
                });
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.lookahead(|g| {
                    cfg_flags::v4(g);
                    cfg_flags::vnext(g);
                    g.call("kw_match");
                });
            }),
        ];
        g.choices(alts);
    });

    g.parser_rule("class_decl", |g| {
        g.node(K::ClassDecl, |g| {
            g.call("kw_class");
            g.call("ident");
            g.optional(|g| {
                g.call("kw_extends");
                g.call("ident");
            });
            g.call("class_body");
        });
    });

    g.parser_rule("class_body", |g| {
        g.node(K::Block, |g| {
            g.call("lbrace");
            g.zero_or_more(|g| {
                g.choice(
                    |g| {
                        g.call("class_member");
                    },
                    |g| {
                        g.call("stmt");
                    },
                );
            });
            g.call("rbrace");
        });
    });

    g.parser_rule("access_modifier", |g| {
        g.choice3(
            |g| {
                g.call("kw_public");
            },
            |g| {
                g.call("kw_private");
            },
            |g| {
                g.call("kw_protected");
            },
        );
    });

    // Some LeekScript code (including the AI fixtures) uses type-keywords as identifiers,
    // e.g. `string string() { ... }`. Accept those keywords in identifier positions where
    // the Java parser is permissive.
    g.parser_rule("name", |g| {
        g.choice(
            |g| {
                g.call("ident");
            },
            |g| {
                g.choices(vec![
                    Box::new(|g| {
                        g.call("kw_string_type");
                    }),
                    Box::new(|g| {
                        g.call("kw_integer");
                    }),
                    Box::new(|g| {
                        g.call("kw_real");
                    }),
                    Box::new(|g| {
                        g.call("kw_boolean");
                    }),
                    Box::new(|g| {
                        g.call("kw_any");
                    }),
                    Box::new(|g| {
                        g.call("kw_void");
                    }),
                ]);
            },
        );
    });

    g.parser_rule("class_member", |g| {
        g.node(K::ClassMember, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.optional(|g| {
                        g.call("access_modifier");
                    });
                    g.call("kw_constructor");
                    g.call("lparen");
                    g.optional(|g| {
                        g.call("fn_param");
                        g.zero_or_more(|g| {
                            g.call("comma");
                            g.call("fn_param");
                        });
                        g.optional(|g| {
                            g.call("comma");
                        });
                    });
                    g.call("rparen");
                    g.call("block");
                }),
                Box::new(|g| {
                    g.optional(|g| {
                        g.call("access_modifier");
                    });
                    g.optional(|g| {
                        g.call("kw_static");
                    });
                    g.optional(|g| {
                        g.call("kw_final");
                    });
                    // Support both typed and untyped members:
                    // - `boolean foo(...) {}` / `SomeType bar = ...`
                    // - `foo(...) {}` (implicit return type)
                    g.choice(
                        |g| {
                            g.call("ls_type");
                            g.call("name");
                        },
                        |g| {
                            g.call("name");
                        },
                    );
                    g.choices(vec![
                        Box::new(|g| {
                            g.call("eq");
                            g.call("expr");
                            g.optional(|g| {
                                g.call("semi");
                            });
                        }),
                        Box::new(|g| {
                            g.call("lparen");
                            g.optional(|g| {
                                g.call("fn_param");
                                g.zero_or_more(|g| {
                                    g.call("comma");
                                    g.call("fn_param");
                                });
                                g.optional(|g| {
                                    g.call("comma");
                                });
                            });
                            g.call("rparen");
                            g.call("block");
                        }),
                        // Allow class fields without an initializer, e.g. `private Foo bar`
                        // (common in the AI scripts). This must be last so `ident (...) {}` still
                        // parses as a method and `ident = expr` parses as an assignment.
                        Box::new(|g| {
                            g.optional(|g| {
                                g.call("semi");
                            });
                        }),
                    ]);
                }),
            ]);
        });
    });

    g.parser_rule("block", |g| {
        g.node(K::Block, |g| {
            g.call("lbrace");
            g.zero_or_more(|g| {
                g.call("stmt");
            });
            g.call("rbrace");
        });
    });

    g.parser_rule("stmt_or_block", |g| {
        g.choice(
            |g| {
                g.call("block");
            },
            |g| {
                g.node(K::Stmt, |g| {
                    g.call("stmt");
                });
            },
        );
    });

    g.parser_rule("return_stmt", |g| {
        g.node(K::ReturnStmt, |g| {
            g.call("kw_return");
            g.optional(|g| {
                g.call("op_question");
            });
            // Do not parse `return for (…)` as `return` + expr: permissive `number` can lex
            // `for` / `var` as NUMBER, yielding a bogus call parse and hiding the real `for` stmt.
            g.optional(|g| {
                g.neg_lookahead(|g| {
                    g.call("kw_for");
                });
                g.call("expr");
            });
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    g.parser_rule("global_decl", |g| {
        g.node(K::GlobalDecl, |g| {
            g.call("kw_global");
            g.optional(|g| {
                g.call("ls_type");
            });
            g.call("ident");
            g.optional(|g| {
                g.call("eq");
                g.call("expr");
            });
            g.zero_or_more(|g| {
                g.call("comma");
                g.call("ident");
                g.optional(|g| {
                    g.call("eq");
                    g.call("expr");
                });
            });
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    g.parser_rule("else_stmt", |g| {
        g.node(K::ElseStmt, |g| {
            g.call("kw_else");
            g.call("stmt_or_block");
        });
    });

    g.parser_rule("switch_stmt", |g| {
        g.node(K::SwitchStmt, |g| {
            g.call("kw_switch");
            g.call("lparen");
            g.call("expr");
            g.call("rparen");
            g.call("lbrace");
            g.zero_or_more(|g| {
                g.call("switch_arm");
            });
            g.call("rbrace");
        });
    });

    g.parser_rule("switch_arm", |g| {
        g.node(K::SwitchArm, |g| {
            g.one_or_more(|g| {
                g.choice(
                    |g| {
                        g.call("kw_case");
                        g.call("expr");
                        g.call("colon");
                    },
                    |g| {
                        g.call("kw_default");
                        g.call("colon");
                    },
                );
            });
            g.zero_or_more(|g| {
                g.call("stmt");
            });
        });
    });

    g.parser_rule("break_stmt", |g| {
        g.node(K::BreakStmt, |g| {
            g.call("kw_break");
            // `break 2` is VNext only; without it, reject a digit level so it is not parsed as
            // `break;` + expression statement `2`.
            g.choice(
                |g| {
                    cfg_flags::vnext(g);
                    g.optional(|g| {
                        g.call("break_continue_level");
                    });
                },
                |g| {
                    cfg_flags::not_vnext(g);
                    g.neg_lookahead(|g| {
                        g.call("break_continue_level");
                    });
                },
            );
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    g.parser_rule("continue_stmt", |g| {
        g.node(K::ContinueStmt, |g| {
            g.call("kw_continue");
            g.choice(
                |g| {
                    cfg_flags::vnext(g);
                    g.optional(|g| {
                        g.call("break_continue_level");
                    });
                },
                |g| {
                    cfg_flags::not_vnext(g);
                    g.neg_lookahead(|g| {
                        g.call("break_continue_level");
                    });
                },
            );
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    g.parser_rule("include_stmt", |g| {
        g.node(K::IncludeStmt, |g| {
            g.call("kw_include");
            g.call("lparen");
            g.call("string");
            g.call("rparen");
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    g.parser_rule("var_decl", |g| {
        g.node(K::VarDecl, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.call("kw_var");
                    g.call("var_decl_items");
                }),
                Box::new(|g| {
                    g.call("kw_let");
                    g.call("var_decl_items");
                }),
                Box::new(|g| {
                    // Java / LeekScript v2+: `Map<K, V> m = [:]` without `var`/`let`.
                    cfg_flags::v2(g);
                    g.call("ls_type");
                    g.call("typed_var_decl_items");
                }),
            ]);
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    // `ident (= expr)? ( , ident (= expr)? )*`
    g.parser_rule("var_decl_items", |g| {
        g.call("ident");
        g.optional(|g| {
            g.call("assign_op");
            g.call("expr");
        });
        g.zero_or_more(|g| {
            g.call("comma");
            g.call("ident");
            g.optional(|g| {
                g.call("assign_op");
                g.call("expr");
            });
        });
    });

    // Same as `var_decl_items` but after a leading type (shared by all names).
    g.parser_rule("typed_var_decl_items", |g| {
        g.call("ident");
        g.optional(|g| {
            g.call("assign_op");
            g.call("expr");
        });
        g.zero_or_more(|g| {
            g.call("comma");
            g.call("ident");
            g.optional(|g| {
                g.call("assign_op");
                g.call("expr");
            });
        });
    });

    g.parser_rule("function_decl", |g| {
        g.node(K::FunctionDecl, |g| {
            g.call("kw_function");
            g.call("name");
            g.call("lparen");
            g.optional(|g| {
                g.call("fn_param");
                g.zero_or_more(|g| {
                    g.call("comma");
                    g.call("fn_param");
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

    // Typed params before bare names so `(n)` is not parsed as type `n`.
    g.parser_rule("fn_param", |g| {
        g.choice(
            |g| {
                g.call("ls_type");
                g.optional(|g| {
                    g.call("op_at");
                });
                g.call("ident");
            },
            |g| {
                g.optional(|g| {
                    g.call("op_at");
                });
                g.call("ident");
            },
        );
    });

    g.parser_rule("param", |g| {
        g.call("fn_param");
    });

    g.parser_rule("if_stmt", |g| {
        g.node(K::IfStmt, |g| {
            g.call("kw_if");
            // Accept both `if (cond)` and `if cond` (fixture style).
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
            g.call("stmt_or_block");
            g.optional(|g| {
                g.call("kw_else");
                g.call("stmt_or_block");
            });
        });
    });

    // Header after `for` (`(` optional in fixtures) matches `WordCompiler.forBlock()`:
    // optional type, optional `var`/`let`, optional `@`, name, then
    // `:` value-decl `in` expr | `in` expr | `=` init `;` cond `;` update.
    // Try `for (` … `)` before paren-free forms; key:value foreach before `in`.
    g.parser_rule("for_stmt", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.node(K::ForeachStmt, |g| {
                    g.call("kw_for");
                    g.call("lparen");
                    g.call("for_loop_var");
                    g.call("colon");
                    g.call("for_loop_var");
                    g.call("kw_in");
                    g.call("expr");
                    g.call("rparen");
                    g.call("stmt_or_block");
                });
            }),
            Box::new(|g| {
                g.node(K::ForeachStmt, |g| {
                    g.call("kw_for");
                    g.call("lparen");
                    g.call("for_loop_var");
                    g.call("kw_in");
                    g.call("expr");
                    g.call("rparen");
                    g.call("stmt_or_block");
                });
            }),
            // Classic `for ( ; cond ; step )` / `for (;;)` — init omitted (first `;` immediately).
            Box::new(|g| {
                g.node(K::ForStmt, |g| {
                    g.call("kw_for");
                    g.call("lparen");
                    g.call("semi");
                    g.optional(|g| {
                        g.call("expr");
                    });
                    g.call("semi");
                    g.optional(|g| {
                        g.call("expr");
                    });
                    g.call("rparen");
                    g.call("stmt_or_block");
                });
            }),
            Box::new(|g| {
                g.node(K::ForStmt, |g| {
                    g.call("kw_for");
                    g.call("lparen");
                    g.call("for_loop_var");
                    g.call("eq");
                    g.call("expr");
                    g.call("semi");
                    g.optional(|g| {
                        g.call("expr");
                    });
                    g.call("semi");
                    g.optional(|g| {
                        g.call("expr");
                    });
                    g.call("rparen");
                    g.call("stmt_or_block");
                });
            }),
            Box::new(|g| {
                g.node(K::ForeachStmt, |g| {
                    g.call("kw_for");
                    g.call("for_loop_var");
                    g.call("colon");
                    g.call("for_loop_var");
                    g.call("kw_in");
                    g.call("expr");
                    g.call("stmt_or_block");
                });
            }),
            Box::new(|g| {
                g.node(K::ForeachStmt, |g| {
                    g.call("kw_for");
                    g.call("for_loop_var");
                    g.call("kw_in");
                    g.call("expr");
                    g.call("stmt_or_block");
                });
            }),
            Box::new(|g| {
                g.node(K::ForStmt, |g| {
                    g.call("kw_for");
                    g.call("for_loop_var");
                    g.call("eq");
                    g.call("expr");
                    g.call("semi");
                    g.optional(|g| {
                        g.call("expr");
                    });
                    g.call("semi");
                    g.optional(|g| {
                        g.call("expr");
                    });
                    g.call("stmt_or_block");
                });
            }),
        ]);
    });

    // After optional type / `var`, optional `@`, one variable name (Java `forBlock`).
    // Typed branch first so `(integer k = …)` wins; `(k in xs)` uses the untyped branch.
    g.parser_rule("for_loop_var", |g| {
        g.choice(
            |g| {
                g.call("ls_type");
                g.optional(|g| {
                    g.call("op_at");
                });
                g.call("ident");
            },
            |g| {
                g.optional(|g| {
                    g.choice(
                        |g| {
                            g.call("kw_var");
                        },
                        |g| {
                            g.call("kw_let");
                        },
                    );
                });
                g.optional(|g| {
                    g.call("op_at");
                });
                g.call("ident");
            },
        );
    });

    g.parser_rule("do_while_stmt", |g| {
        g.node(K::DoWhileStmt, |g| {
            g.call("kw_do");
            g.call("stmt_or_block");
            g.call("kw_while");
            g.call("lparen");
            g.call("expr");
            g.call("rparen");
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    g.parser_rule("while_stmt", |g| {
        g.node(K::WhileStmt, |g| {
            g.call("kw_while");
            g.call("lparen");
            g.call("expr");
            g.call("rparen");
            g.call("stmt_or_block");
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule("try_stmt", |g| {
        g.node(K::TryStmt, |g| {
            g.call("kw_try");
            g.call("block");
            g.zero_or_more(|g| {
                g.node(K::CatchClause, |g| {
                    g.call("kw_catch");
                    g.call("lparen");
                    g.call("ls_type");
                    g.call("ident");
                    g.call("rparen");
                    g.call("block");
                });
            });
            g.optional(|g| {
                g.call("kw_finally");
                g.call("block");
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule("throw_stmt", |g| {
        g.node(K::ThrowStmt, |g| {
            g.call("kw_throw");
            g.call("expr");
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule("import_stmt", |g| {
        g.node(K::ImportStmt, |g| {
            g.call("kw_import");
            g.choice(
                |g| {
                    g.call("string");
                },
                |g| {
                    g.call("ident");
                    g.zero_or_more(|g| {
                        g.call("dot");
                        g.call("ident");
                    });
                },
            );
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule("export_stmt", |g| {
        g.node(K::ExportStmt, |g| {
            g.call("kw_export");
            g.call("block");
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule("goto_stmt", |g| {
        g.node(K::GotoStmt, |g| {
            g.call("kw_goto");
            g.call("ident");
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule("package_stmt", |g| {
        g.node(K::PackageStmt, |g| {
            g.call("kw_package");
            g.call("ident");
            g.zero_or_more(|g| {
                g.call("dot");
                g.call("ident");
            });
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (`CONST` exists in the lexer only).
    g.parser_rule("const_decl", |g| {
        g.node(K::ConstDecl, |g| {
            g.call("kw_const");
            g.call("var_decl_items");
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    // LeekScript extension; not in leekscript-java `LexicalParser` / `WordCompiler`.
    g.parser_rule("match_stmt", |g| {
        g.node(K::MatchStmt, |g| {
            g.call("kw_match");
            g.call("expr");
            g.call("lbrace");
            g.zero_or_more(|g| {
                g.call("match_case");
            });
            g.call("rbrace");
        });
    });

    g.parser_rule("match_case", |g| {
        // pattern ":" stmt
        // pattern is either an expression or the wildcard `..`
        g.choice(
            |g| {
                g.call("dotdot");
            },
            |g| {
                g.call("expr");
            },
        );
        g.call("colon");
        g.call("stmt");
    });
}
