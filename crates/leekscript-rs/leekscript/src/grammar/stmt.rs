use super::cfg_flags;
use crate::syntax::kinds::K;
use sipha::prelude::parse::GrammarChoiceFn;
use sipha::prelude::*;

pub fn define(g: &mut GrammarBuilder) {
    // Start rule must be rule 0.
    g.parser_rule("start", |g| {
        g.node(K::Root, |g| {
            g.zero_or_more(|g| {
                g.call("stmt");
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
                g.call("try_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                g.call("throw_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                g.call("import_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                g.call("export_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                g.call("goto_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                g.call("package_stmt");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v3(g);
                g.call("const_decl");
            }),
            Box::new(|g: &mut GrammarBuilder| {
                g.call("while_stmt");
            }),
            // `match` is not a Java lexer keyword; this is a leekscript-rs extension (v4+).
            Box::new(|g: &mut GrammarBuilder| {
                cfg_flags::v4(g);
                g.call("match_stmt");
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
                    g.optional(|g| {
                        g.call("ls_type");
                    });
                    g.call("ident");
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
            g.optional(|g| {
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
            // `break` can be followed by an optional numeric level: `break 2`
            g.optional(|g| {
                g.call("number");
            });
            g.optional(|g| {
                g.call("semi");
            });
        });
    });

    g.parser_rule("continue_stmt", |g| {
        g.node(K::ContinueStmt, |g| {
            g.call("kw_continue");
            // `continue` can be followed by an optional numeric level: `continue 2`
            g.optional(|g| {
                g.call("number");
            });
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
            g.choice(
                |g| {
                    g.call("kw_var");
                },
                |g| {
                    g.call("kw_let");
                },
            );
            g.call("var_decl_items");
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

    g.parser_rule("function_decl", |g| {
        g.node(K::FunctionDecl, |g| {
            g.call("kw_function");
            g.call("ident");
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
