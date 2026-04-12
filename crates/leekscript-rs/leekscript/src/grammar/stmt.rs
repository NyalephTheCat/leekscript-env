//! Statements, the start rule, blocks, class members, and control flow.
use super::GRule;
use super::cfg_flags;
use crate::parse::version::{FLAG_PARSE_RECOVERY, FLAG_SIGNATURE_MODE};
use crate::syntax::kinds::Node;
use sipha::prelude::*;

pub fn define(g: &mut GrammarBuilder) {
    // Start rule must be rule 0.
    g.parser_rule(GRule::Start.as_str(), |g| {
        g.node(Node::Root, |g| {
            g.zero_or_more(|g| {
                g.choice(
                    |g| {
                        g.if_flag(FLAG_PARSE_RECOVERY);
                        g.recover_until_rule(GRule::TopLevelSync, |g| {
                            g.call_rule(GRule::Stmt);
                        });
                    },
                    |g| {
                        g.if_not_flag(FLAG_PARSE_RECOVERY);
                        g.call_rule(GRule::Stmt);
                    },
                );
            });
            g.skip();
        });
        g.end_of_input();
        g.accept();
    });

    g.parser_rule(GRule::StmtEmptyStatement.as_str(), |g| {
        g.node(Node::EmptyStmt, |g| {
            g.call_rule(GRule::Semi);
        });
    });

    g.parser_rule(GRule::StmtExprStatement.as_str(), |g| {
        g.node(Node::Stmt, |g| {
            g.call_rule(GRule::Expr);
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    g.parser_rule(GRule::Stmt.as_str(), |g| {
        sipha::choices!(
            g,
            |g| {
                g.call_rule(GRule::StmtEmptyStatement);
            },
            |g| {
                g.call_rule(GRule::IncludeStmt);
            },
            |g| {
                g.call_rule(GRule::ReturnStmt);
            },
            |g| {
                g.call_rule(GRule::BreakStmt);
            },
            |g| {
                g.call_rule(GRule::ContinueStmt);
            },
            |g| {
                g.call_rule(GRule::GlobalDecl);
            },
            |g| {
                g.call_rule(GRule::ElseStmt);
            },
            |g| {
                cfg_flags::v3(g);
                g.call_rule(GRule::SwitchStmt);
            },
            |g| {
                g.call_rule(GRule::VarDecl);
            },
            |g| {
                g.call_rule(GRule::FunctionDecl);
            },
            |g| {
                g.call_rule(GRule::ClassDecl);
            },
            |g| {
                g.call_rule(GRule::IfStmt);
            },
            |g| {
                g.call_rule(GRule::ForStmt);
            },
            |g| {
                g.call_rule(GRule::DoWhileStmt);
            },
            |g| {
                cfg_flags::v3(g);
                cfg_flags::exp_exceptions(g);
                g.call_rule(GRule::TryStmt);
            },
            |g| {
                cfg_flags::v3(g);
                cfg_flags::exp_exceptions(g);
                g.call_rule(GRule::ThrowStmt);
            },
            |g| {
                cfg_flags::v3(g);
                cfg_flags::exp_modules(g);
                g.call_rule(GRule::ImportStmt);
            },
            |g| {
                cfg_flags::v3(g);
                cfg_flags::exp_modules(g);
                g.call_rule(GRule::ExportStmt);
            },
            |g| {
                cfg_flags::v3(g);
                cfg_flags::exp_goto(g);
                g.call_rule(GRule::GotoStmt);
            },
            |g| {
                cfg_flags::v3(g);
                cfg_flags::exp_modules(g);
                g.call_rule(GRule::PackageStmt);
            },
            |g| {
                cfg_flags::v3(g);
                cfg_flags::exp_lexical_const(g);
                g.call_rule(GRule::ConstDecl);
            },
            |g| {
                g.call_rule(GRule::WhileStmt);
            },
            |g| {
                cfg_flags::v4(g);
                cfg_flags::exp_match(g);
                g.call_rule(GRule::MatchStmt);
            },
            |g| {
                g.call_rule(GRule::StmtExprStatement);
            },
        );
    });

    g.parser_rule(GRule::TopLevelSync.as_str(), |g| {
        g.skip();
        sipha::choices!(
            g,
            |g| {
                g.call_rule(GRule::Semi);
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwFunction);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwClass);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwVar);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::exp_let(g);
                    g.call_rule(GRule::KwLet);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwGlobal);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwIf);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwFor);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwWhile);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwDo);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwReturn);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwBreak);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwContinue);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwInclude);
                });
            },
            |g| {
                g.lookahead(|g| {
                    g.call_rule(GRule::KwElse);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    g.call_rule(GRule::KwSwitch);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::exp_exceptions(g);
                    g.call_rule(GRule::KwTry);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::exp_exceptions(g);
                    g.call_rule(GRule::KwThrow);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::exp_modules(g);
                    g.call_rule(GRule::KwImport);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::exp_modules(g);
                    g.call_rule(GRule::KwExport);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::exp_goto(g);
                    g.call_rule(GRule::KwGoto);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::exp_modules(g);
                    g.call_rule(GRule::KwPackage);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v3(g);
                    cfg_flags::exp_lexical_const(g);
                    g.call_rule(GRule::KwConst);
                });
            },
            |g| {
                g.lookahead(|g| {
                    cfg_flags::v4(g);
                    cfg_flags::exp_match(g);
                    g.call_rule(GRule::KwMatch);
                });
            },
        );
    });

    g.parser_rule(GRule::ClassDecl.as_str(), |g| {
        g.node(Node::ClassDecl, |g| {
            g.call_rule(GRule::KwClass);
            g.call_rule(GRule::Ident);
            g.optional(|g| {
                cfg_flags::exp_templates(g);
                g.call_rule(GRule::DeclTemplateParams);
            });
            g.optional(|g| {
                g.call_rule(GRule::KwExtends);
                // Java suite allows `extends Array` / `extends Map` etc (type-keywords as names).
                g.call_rule(GRule::Name);
            });
            g.call_rule(GRule::ClassBody);
        });
    });

    // `function f<T, U>(…)`, `class C<T>`, `function<T>(…) {}` — comma-separated idents (not lambdas: `<T>` clashes with set literals).
    g.parser_rule(GRule::DeclTemplateParams.as_str(), |g| {
        g.node(Node::TemplateParams, |g| {
            g.call_rule(GRule::OpLt);
            g.call_rule(GRule::Ident);
            g.zero_or_more(|g| {
                g.call_rule(GRule::Comma);
                g.call_rule(GRule::Ident);
            });
            g.optional(|g| {
                g.call_rule(GRule::Comma);
            });
            g.call_rule(GRule::TypeGt);
        });
    });

    g.parser_rule(GRule::ClassBody.as_str(), |g| {
        g.node(Node::Block, |g| {
            g.call_rule(GRule::Lbrace);
            g.zero_or_more(|g| {
                g.choice(
                    |g| {
                        g.call_rule(GRule::ClassMember);
                    },
                    |g| {
                        g.call_rule(GRule::Stmt);
                    },
                );
            });
            g.call_rule(GRule::Rbrace);
        });
    });

    g.parser_rule(GRule::AccessModifier.as_str(), |g| {
        sipha::choices!(
            g,
            |g| {
                g.call_rule(GRule::KwPublic);
            },
            |g| {
                g.call_rule(GRule::KwPrivate);
            },
            |g| {
                g.call_rule(GRule::KwProtected);
            },
        );
    });

    // Some LeekScript code (including the AI fixtures) uses type-keywords as identifiers,
    // e.g. `string string() { ... }`. Accept those keywords in identifier positions where
    // the Java parser is permissive.
    g.parser_rule(GRule::Name.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::Ident);
            },
            |g| {
                sipha::choices!(
                    g,
                    |g| {
                        g.call_rule(GRule::KwStringType);
                    },
                    |g| {
                        g.call_rule(GRule::KwInteger);
                    },
                    |g| {
                        g.call_rule(GRule::KwReal);
                    },
                    |g| {
                        g.call_rule(GRule::KwBoolean);
                    },
                    |g| {
                        g.call_rule(GRule::KwAny);
                    },
                    |g| {
                        g.call_rule(GRule::KwVoid);
                    },
                    |g| {
                        cfg_flags::v3(g);
                        g.call_rule(GRule::KwDefault);
                    },
                    |g| {
                        g.call_rule(GRule::KwInclude);
                    },
                    |g| {
                        g.call_rule(GRule::KwFunction);
                    },
                    |g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwArray);
                    },
                    |g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwMap);
                    },
                    |g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwObject);
                    },
                    |g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwSetType);
                    },
                    |g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwFunctionType);
                    },
                    |g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwIntervalType);
                    },
                    |g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwClassType);
                    },
                );
            },
        );
    });

    g.parser_rule(GRule::ClassMember.as_str(), |g| {
        g.node(Node::ClassMember, |g| {
            sipha::choices!(
                g,
                |g| {
                    g.optional(|g| {
                        g.call_rule(GRule::AccessModifier);
                    });
                    g.call_rule(GRule::KwConstructor);
                    g.call_rule(GRule::Lparen);
                    g.optional(|g| {
                        g.call_rule(GRule::MethodFnParam);
                        g.zero_or_more(|g| {
                            g.call_rule(GRule::Comma);
                            g.call_rule(GRule::MethodFnParam);
                        });
                        g.optional(|g| {
                            g.call_rule(GRule::Comma);
                        });
                    });
                    g.call_rule(GRule::Rparen);
                    g.call_rule(GRule::Block);
                },
                |g| {
                    g.optional(|g| {
                        g.call_rule(GRule::AccessModifier);
                    });
                    g.optional(|g| {
                        g.call_rule(GRule::KwStatic);
                    });
                    g.optional(|g| {
                        g.call_rule(GRule::KwFinal);
                    });
                    // Support both typed and untyped members:
                    // - `boolean foo(...) {}` / `SomeType bar = ...`
                    // - `foo(...) {}` (implicit return type)
                    g.choice(
                        |g| {
                            g.call_rule(GRule::LsType);
                            g.call_rule(GRule::Name);
                        },
                        |g| {
                            g.call_rule(GRule::Name);
                        },
                    );
                    sipha::choices!(
                        g,
                        |g| {
                            g.call_rule(GRule::Eq);
                            g.call_rule(GRule::Expr);
                            g.optional(|g| {
                                g.call_rule(GRule::Semi);
                            });
                        },
                        |g| {
                            g.call_rule(GRule::Lparen);
                            g.optional(|g| {
                                g.call_rule(GRule::MethodFnParam);
                                g.zero_or_more(|g| {
                                    g.call_rule(GRule::Comma);
                                    g.call_rule(GRule::MethodFnParam);
                                });
                                g.optional(|g| {
                                    g.call_rule(GRule::Comma);
                                });
                            });
                            g.call_rule(GRule::Rparen);
                            g.call_rule(GRule::Block);
                        },
                        // Allow class fields without an initializer, e.g. `private Foo bar`
                        // (common in the AI scripts). This must be last so `ident (...) {}` still
                        // parses as a method and `ident = expr` parses as an assignment.
                        |g| {
                            g.optional(|g| {
                                g.call_rule(GRule::Semi);
                            });
                        },
                    );
                },
            );
        });
    });

    g.parser_rule(GRule::Block.as_str(), |g| {
        g.node(Node::Block, |g| {
            g.call_rule(GRule::Lbrace);
            g.zero_or_more(|g| {
                g.call_rule(GRule::Stmt);
            });
            g.call_rule(GRule::Rbrace);
        });
    });

    g.parser_rule(GRule::StmtOrBlock.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::Block);
            },
            // Do not wrap in an extra `Node::Stmt`: `StmtExprStatement` already builds `Node::Stmt`
            // with an `Expr` child. A second wrapper yields `Stmt(Stmt(expr))` and breaks
            // `ExprStmt::expr()` (`child::<Expr>()` finds the inner `Stmt`, not the `Expr`).
            |g| {
                g.call_rule(GRule::Stmt);
            },
        );
    });

    g.parser_rule(GRule::ReturnStmt.as_str(), |g| {
        g.node(Node::ReturnStmt, |g| {
            g.call_rule(GRule::KwReturn);
            g.optional(|g| {
                g.call_rule(GRule::OpQuestion);
            });
            // Do not parse `return for (…)` as `return` + expr: permissive `number` can lex
            // `for` / `var` as NUMBER, yielding a bogus call parse and hiding the real `for` stmt.
            g.optional(|g| {
                g.neg_lookahead(|g| {
                    g.call_rule(GRule::KwFor);
                });
                g.call_rule(GRule::Expr);
            });
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    g.parser_rule(GRule::GlobalDecl.as_str(), |g| {
        g.node(Node::GlobalDecl, |g| {
            g.call_rule(GRule::KwGlobal);
            // Disambiguate `global x;` (untyped) from `global integer x;` / `global Map<K,V> m;`.
            //
            // In Java LeekScript, a typed global declaration starts with a *type keyword* (`integer`,
            // `real`, `Map`, `Array`, …). A plain identifier after `global` is a variable name, not
            // a user-defined type name (important for snippets like `global y x = (y = [:])`).
            g.choice(
                |g| {
                    g.lookahead(|g| {
                        sipha::choices!(
                            g,
                            |g| {
                                g.call_rule(GRule::KwInteger);
                            },
                            |g| {
                                g.call_rule(GRule::KwReal);
                            },
                            |g| {
                                g.call_rule(GRule::KwBoolean);
                            },
                            |g| {
                                g.call_rule(GRule::KwStringType);
                            },
                            |g| {
                                g.call_rule(GRule::KwAny);
                            },
                            |g| {
                                cfg_flags::v2(g);
                                g.call_rule(GRule::KwArray);
                            },
                            |g| {
                                cfg_flags::v2(g);
                                g.call_rule(GRule::KwIntervalType);
                            },
                            |g| {
                                cfg_flags::v2(g);
                                g.call_rule(GRule::KwSetType);
                            },
                            |g| {
                                cfg_flags::v2(g);
                                g.call_rule(GRule::KwMap);
                            },
                            |g| {
                                cfg_flags::v2(g);
                                g.call_rule(GRule::KwObject);
                            },
                            |g| {
                                cfg_flags::v2(g);
                                g.call_rule(GRule::KwFunctionType);
                            },
                            |g| {
                                cfg_flags::v2(g);
                                g.call_rule(GRule::KwClassType);
                            },
                            |g| {
                                cfg_flags::v3(g);
                                g.call_rule(GRule::KwNull);
                            },
                        );
                    });
                    g.call_rule(GRule::LsType);
                    g.call_rule(GRule::Ident);
                },
                |g| {
                    g.call_rule(GRule::Ident);
                },
            );
            g.optional(|g| {
                g.call_rule(GRule::Eq);
                g.call_rule(GRule::Expr);
            });
            g.zero_or_more(|g| {
                g.call_rule(GRule::Comma);
                g.call_rule(GRule::Ident);
                g.optional(|g| {
                    g.call_rule(GRule::Eq);
                    g.call_rule(GRule::Expr);
                });
            });
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    g.parser_rule(GRule::ElseStmt.as_str(), |g| {
        g.node(Node::ElseStmt, |g| {
            g.call_rule(GRule::KwElse);
            g.call_rule(GRule::StmtOrBlock);
        });
    });

    g.parser_rule(GRule::SwitchStmt.as_str(), |g| {
        g.node(Node::SwitchStmt, |g| {
            g.call_rule(GRule::KwSwitch);
            g.call_rule(GRule::Lparen);
            g.call_rule(GRule::Expr);
            g.call_rule(GRule::Rparen);
            g.call_rule(GRule::Lbrace);
            g.zero_or_more(|g| {
                g.call_rule(GRule::SwitchArm);
            });
            g.call_rule(GRule::Rbrace);
        });
    });

    g.parser_rule(GRule::SwitchArm.as_str(), |g| {
        g.node(Node::SwitchArm, |g| {
            g.one_or_more(|g| {
                g.choice(
                    |g| {
                        g.call_rule(GRule::KwCase);
                        g.call_rule(GRule::Expr);
                        g.call_rule(GRule::Colon);
                    },
                    |g| {
                        g.call_rule(GRule::KwDefault);
                        g.call_rule(GRule::Colon);
                    },
                );
            });
            g.zero_or_more(|g| {
                g.call_rule(GRule::Stmt);
            });
        });
    });

    g.parser_rule(GRule::BreakStmt.as_str(), |g| {
        g.node(Node::BreakStmt, |g| {
            g.call_rule(GRule::KwBreak);
            // `break 2` needs experimental loop levels; without it, reject a digit level so it is
            // not parsed as `break;` + expression statement `2`.
            g.choice(
                |g| {
                    cfg_flags::exp_loop_levels(g);
                    g.optional(|g| {
                        g.call_rule(GRule::BreakContinueLevel);
                    });
                },
                |g| {
                    cfg_flags::not_exp_loop_levels(g);
                    g.neg_lookahead(|g| {
                        g.call_rule(GRule::BreakContinueLevel);
                    });
                },
            );
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    g.parser_rule(GRule::ContinueStmt.as_str(), |g| {
        g.node(Node::ContinueStmt, |g| {
            g.call_rule(GRule::KwContinue);
            g.choice(
                |g| {
                    cfg_flags::exp_loop_levels(g);
                    g.optional(|g| {
                        g.call_rule(GRule::BreakContinueLevel);
                    });
                },
                |g| {
                    cfg_flags::not_exp_loop_levels(g);
                    g.neg_lookahead(|g| {
                        g.call_rule(GRule::BreakContinueLevel);
                    });
                },
            );
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    g.parser_rule(GRule::IncludeStmt.as_str(), |g| {
        g.node(Node::IncludeStmt, |g| {
            g.call_rule(GRule::KwInclude);
            g.call_rule(GRule::Lparen);
            g.call_rule(GRule::String);
            g.call_rule(GRule::Rparen);
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    g.parser_rule(GRule::VarDecl.as_str(), |g| {
        g.node(Node::VarDecl, |g| {
            sipha::choices!(
                g,
                |g| {
                    g.call_rule(GRule::KwVar);
                    g.call_rule(GRule::VarDeclItems);
                },
                |g| {
                    cfg_flags::exp_let(g);
                    g.call_rule(GRule::KwLet);
                    g.call_rule(GRule::VarDeclItems);
                },
                |g| {
                    // Java / LeekScript v2+: `Map<K, V> m = [:]` without `var`/`let`.
                    cfg_flags::v2(g);
                    g.call_rule(GRule::LsType);
                    g.call_rule(GRule::TypedVarDeclItems);
                },
            );
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    // `ident (= expr)? ( , ident (= expr)? )*`
    g.parser_rule(GRule::VarDeclItems.as_str(), |g| {
        g.call_rule(GRule::Ident);
        g.optional(|g| {
            g.call_rule(GRule::AssignOp);
            g.call_rule(GRule::Expr);
        });
        g.zero_or_more(|g| {
            g.call_rule(GRule::Comma);
            g.call_rule(GRule::Ident);
            g.optional(|g| {
                g.call_rule(GRule::AssignOp);
                g.call_rule(GRule::Expr);
            });
        });
    });

    // Same as `var_decl_items` but after a leading type (shared by all names).
    g.parser_rule(GRule::TypedVarDeclItems.as_str(), |g| {
        g.call_rule(GRule::Ident);
        g.optional(|g| {
            g.call_rule(GRule::AssignOp);
            g.call_rule(GRule::Expr);
        });
        g.zero_or_more(|g| {
            g.call_rule(GRule::Comma);
            g.call_rule(GRule::Ident);
            g.optional(|g| {
                g.call_rule(GRule::AssignOp);
                g.call_rule(GRule::Expr);
            });
        });
    });

    g.parser_rule(GRule::FunctionDecl.as_str(), |g| {
        g.node(Node::FunctionDecl, |g| {
            g.call_rule(GRule::KwFunction);
            g.call_rule(GRule::Name);
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
            g.choice(
                |g| {
                    g.if_flag(FLAG_SIGNATURE_MODE);
                    g.choice(
                        |g| {
                            g.call_rule(GRule::Semi);
                        },
                        |g| {
                            g.call_rule(GRule::Block);
                        },
                    );
                },
                |g| {
                    g.if_not_flag(FLAG_SIGNATURE_MODE);
                    g.call_rule(GRule::Block);
                },
            );
        });
    });

    // Typed params before bare names so `(n)` is not parsed as type `n`.
    g.parser_rule(GRule::FnParamCore.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::LsType);
                g.optional(|g| {
                    g.call_rule(GRule::OpAt);
                });
                // Match `name` / class members: `string string` parameter names use type keywords.
                g.call_rule(GRule::Name);
            },
            |g| {
                g.optional(|g| {
                    g.call_rule(GRule::OpAt);
                });
                g.call_rule(GRule::Ident);
            },
        );
    });

    // Method / constructor parameters — optional default with `= expr`.
    g.parser_rule(GRule::MethodFnParam.as_str(), |g| {
        g.node(Node::FnParam, |g| {
            g.call_rule(GRule::FnParamCore);
            g.optional(|g| {
                g.call_rule(GRule::Eq);
                g.call_rule(GRule::Expr);
            });
        });
    });

    // Top-level / anonymous `function` parameters — `= expr` under LSv4 or signature/stub mode.
    g.parser_rule(GRule::FunctionFnParam.as_str(), |g| {
        g.node(Node::FnParam, |g| {
            g.call_rule(GRule::FnParamCore);
            g.optional(|g| {
                sipha::choices!(
                    g,
                    |g| {
                        cfg_flags::v4(g);
                        g.call_rule(GRule::Eq);
                        g.call_rule(GRule::Expr);
                    },
                    |g| {
                        g.if_flag(FLAG_SIGNATURE_MODE);
                        g.call_rule(GRule::Eq);
                        g.call_rule(GRule::Expr);
                    },
                );
            });
        });
    });

    g.parser_rule(GRule::Param.as_str(), |g| {
        g.call_rule(GRule::FunctionFnParam);
    });

    g.parser_rule(GRule::IfStmt.as_str(), |g| {
        g.node(Node::IfStmt, |g| {
            g.call_rule(GRule::KwIf);
            // Accept both `if (cond)` and `if cond` (fixture style).
            g.choice(
                |g| {
                    g.call_rule(GRule::Lparen);
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::Rparen);
                },
                |g| {
                    g.call_rule(GRule::Expr);
                },
            );
            g.call_rule(GRule::StmtOrBlock);
            g.optional(|g| {
                g.call_rule(GRule::KwElse);
                g.call_rule(GRule::StmtOrBlock);
            });
        });
    });

    // Header after `for` (`(` optional in fixtures) matches `WordCompiler.forBlock()`:
    // optional type, optional `var`/`let`, optional `@`, name, then
    // `:` value-decl `in` expr | `in` expr | `=` init `;` cond `;` update.
    // Try `for (` … `)` before paren-free forms; key:value foreach before `in`.
    g.parser_rule(GRule::ForStmt.as_str(), |g| {
        sipha::choices!(
            g,
            |g| {
                g.node(Node::ForeachStmt, |g| {
                    g.call_rule(GRule::KwFor);
                    g.call_rule(GRule::Lparen);
                    g.call_rule(GRule::ForLoopVar);
                    g.call_rule(GRule::Colon);
                    g.call_rule(GRule::ForLoopVar);
                    g.call_rule(GRule::KwIn);
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::Rparen);
                    g.call_rule(GRule::StmtOrBlock);
                });
            },
            |g| {
                g.node(Node::ForeachStmt, |g| {
                    g.call_rule(GRule::KwFor);
                    g.call_rule(GRule::Lparen);
                    g.call_rule(GRule::ForLoopVar);
                    g.call_rule(GRule::KwIn);
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::Rparen);
                    g.call_rule(GRule::StmtOrBlock);
                });
            },
            // Classic `for ( ; cond ; step )` / `for (;;)` — init omitted (first `;` immediately).
            |g| {
                g.node(Node::ForStmt, |g| {
                    g.call_rule(GRule::KwFor);
                    g.call_rule(GRule::Lparen);
                    g.call_rule(GRule::Semi);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.call_rule(GRule::Semi);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.call_rule(GRule::Rparen);
                    g.call_rule(GRule::StmtOrBlock);
                });
            },
            |g| {
                g.node(Node::ForStmt, |g| {
                    g.call_rule(GRule::KwFor);
                    g.call_rule(GRule::Lparen);
                    g.call_rule(GRule::ForLoopVar);
                    g.call_rule(GRule::Eq);
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::Semi);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.call_rule(GRule::Semi);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.call_rule(GRule::Rparen);
                    g.call_rule(GRule::StmtOrBlock);
                });
            },
            |g| {
                g.node(Node::ForeachStmt, |g| {
                    g.call_rule(GRule::KwFor);
                    g.call_rule(GRule::ForLoopVar);
                    g.call_rule(GRule::Colon);
                    g.call_rule(GRule::ForLoopVar);
                    g.call_rule(GRule::KwIn);
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::StmtOrBlock);
                });
            },
            |g| {
                g.node(Node::ForeachStmt, |g| {
                    g.call_rule(GRule::KwFor);
                    g.call_rule(GRule::ForLoopVar);
                    g.call_rule(GRule::KwIn);
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::StmtOrBlock);
                });
            },
            |g| {
                g.node(Node::ForStmt, |g| {
                    g.call_rule(GRule::KwFor);
                    g.call_rule(GRule::ForLoopVar);
                    g.call_rule(GRule::Eq);
                    g.call_rule(GRule::Expr);
                    g.call_rule(GRule::Semi);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.call_rule(GRule::Semi);
                    g.optional(|g| {
                        g.call_rule(GRule::Expr);
                    });
                    g.call_rule(GRule::StmtOrBlock);
                });
            },
        );
    });

    // After optional type / `var`, optional `@`, one variable name (Java `forBlock`).
    // Typed branch first so `(integer k = …)` wins; `(k in xs)` uses the untyped branch.
    g.parser_rule(GRule::ForLoopVar.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::LsType);
                g.optional(|g| {
                    g.call_rule(GRule::OpAt);
                });
                g.call_rule(GRule::Ident);
            },
            |g| {
                g.optional(|g| {
                    g.choice(
                        |g| {
                            g.call_rule(GRule::KwVar);
                        },
                        |g| {
                            cfg_flags::exp_let(g);
                            g.call_rule(GRule::KwLet);
                        },
                    );
                });
                g.optional(|g| {
                    g.call_rule(GRule::OpAt);
                });
                g.call_rule(GRule::Ident);
            },
        );
    });

    g.parser_rule(GRule::DoWhileStmt.as_str(), |g| {
        g.node(Node::DoWhileStmt, |g| {
            g.call_rule(GRule::KwDo);
            g.call_rule(GRule::StmtOrBlock);
            g.call_rule(GRule::KwWhile);
            g.call_rule(GRule::Lparen);
            g.call_rule(GRule::Expr);
            g.call_rule(GRule::Rparen);
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    g.parser_rule(GRule::WhileStmt.as_str(), |g| {
        g.node(Node::WhileStmt, |g| {
            g.call_rule(GRule::KwWhile);
            g.call_rule(GRule::Lparen);
            g.call_rule(GRule::Expr);
            g.call_rule(GRule::Rparen);
            g.call_rule(GRule::StmtOrBlock);
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule(GRule::TryStmt.as_str(), |g| {
        g.node(Node::TryStmt, |g| {
            g.call_rule(GRule::KwTry);
            g.call_rule(GRule::Block);
            g.zero_or_more(|g| {
                g.node(Node::CatchClause, |g| {
                    g.call_rule(GRule::KwCatch);
                    g.call_rule(GRule::Lparen);
                    g.call_rule(GRule::LsType);
                    g.call_rule(GRule::Ident);
                    g.call_rule(GRule::Rparen);
                    g.call_rule(GRule::Block);
                });
            });
            g.optional(|g| {
                g.call_rule(GRule::KwFinally);
                g.call_rule(GRule::Block);
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule(GRule::ThrowStmt.as_str(), |g| {
        g.node(Node::ThrowStmt, |g| {
            g.call_rule(GRule::KwThrow);
            g.call_rule(GRule::Expr);
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule(GRule::ImportStmt.as_str(), |g| {
        g.node(Node::ImportStmt, |g| {
            g.call_rule(GRule::KwImport);
            g.choice(
                |g| {
                    g.call_rule(GRule::String);
                },
                |g| {
                    g.call_rule(GRule::Ident);
                    g.zero_or_more(|g| {
                        g.call_rule(GRule::Dot);
                        g.call_rule(GRule::Ident);
                    });
                },
            );
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule(GRule::ExportStmt.as_str(), |g| {
        g.node(Node::ExportStmt, |g| {
            g.call_rule(GRule::KwExport);
            g.call_rule(GRule::Block);
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule(GRule::GotoStmt.as_str(), |g| {
        g.node(Node::GotoStmt, |g| {
            g.call_rule(GRule::KwGoto);
            g.call_rule(GRule::Ident);
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (lexer token only).
    g.parser_rule(GRule::PackageStmt.as_str(), |g| {
        g.node(Node::PackageStmt, |g| {
            g.call_rule(GRule::KwPackage);
            g.call_rule(GRule::Ident);
            g.zero_or_more(|g| {
                g.call_rule(GRule::Dot);
                g.call_rule(GRule::Ident);
            });
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    // Not in leekscript-java `WordCompiler` (`CONST` exists in the lexer only).
    g.parser_rule(GRule::ConstDecl.as_str(), |g| {
        g.node(Node::ConstDecl, |g| {
            g.call_rule(GRule::KwConst);
            g.call_rule(GRule::VarDeclItems);
            g.optional(|g| {
                g.call_rule(GRule::Semi);
            });
        });
    });

    // LeekScript extension; not in leekscript-java `LexicalParser` / `WordCompiler`.
    g.parser_rule(GRule::MatchStmt.as_str(), |g| {
        g.node(Node::MatchStmt, |g| {
            g.call_rule(GRule::KwMatch);
            g.call_rule(GRule::Expr);
            g.call_rule(GRule::Lbrace);
            g.zero_or_more(|g| {
                g.call_rule(GRule::MatchCase);
            });
            g.call_rule(GRule::Rbrace);
        });
    });

    g.parser_rule(GRule::MatchCase.as_str(), |g| {
        // pattern ":" stmt
        // pattern is either an expression or the wildcard `..`
        g.choice(
            |g| {
                g.call_rule(GRule::Dotdot);
            },
            |g| {
                g.call_rule(GRule::Expr);
            },
        );
        g.call_rule(GRule::Colon);
        g.call_rule(GRule::Stmt);
    });
}
