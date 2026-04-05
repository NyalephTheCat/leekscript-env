//! LeekScript type grammar (aligned with leekscript-java `eatType` / `eatPrimaryType`).
use super::cfg_flags;
use super::GRule;
use crate::syntax::kinds::Node;
use sipha::prelude::*;

pub fn define(g: &mut GrammarBuilder) {
    // type = union (| union)*
    g.parser_rule(GRule::LsType.as_str(), |g| {
        g.node(Node::TypeExpr, |g| {
            g.call_rule(GRule::TypeUnion);
        });
    });

    g.parser_rule(GRule::TypeUnion.as_str(), |g| {
        g.node(Node::TypeUnionType, |g| {
            g.call_rule(GRule::TypeNullable);
            g.zero_or_more(|g| {
                g.call_rule(GRule::OpBitor);
                g.call_rule(GRule::TypeNullable);
            });
        });
    });

    g.parser_rule(GRule::TypeNullable.as_str(), |g| {
        g.node(Node::TypeNullableType, |g| {
            g.call_rule(GRule::TypePrimary);
            g.optional(|g| {
                g.call_rule(GRule::OpQuestion);
            });
        });
    });

    // `any` / `array`, `integer` / `interval`, `string` / `Set` share a leading letter.
    g.parser_rule(GRule::LambdaTypeKwA.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::KwAny);
            },
            |g| {
                cfg_flags::v2(g);
                g.call_rule(GRule::KwArray);
                g.optional(|g| {
                    g.call_rule(GRule::GenericTypeArgs);
                });
            },
        );
    });
    g.parser_rule(GRule::LambdaTypeKwI.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::KwInteger);
            },
            |g| {
                cfg_flags::v2(g);
                g.call_rule(GRule::KwIntervalType);
                g.optional(|g| {
                    g.call_rule(GRule::GenericTypeArgs);
                });
            },
        );
    });
    g.parser_rule(GRule::LambdaTypeKwS.as_str(), |g| {
        g.choice(
            |g| {
                g.call_rule(GRule::KwStringType);
            },
            |g| {
                cfg_flags::v2(g);
                g.call_rule(GRule::KwSetType);
                g.optional(|g| {
                    g.call_rule(GRule::GenericTypeArgs);
                });
            },
        );
    });

    // Like `type_primary` but without bare `ident`, so `x => x + 1` is not parsed as typed lambda
    // `x` (type) `+ 1` (body). Used only after `=>` for explicit lambda return types (`=> real expr`).
    g.parser_rule(GRule::LambdaTypePrimaryNoIdent.as_str(), |g| {
        // v3-only spellings are lowercase; v2 type keywords (`Class`, `Interval`, …) are ASCII-CI.
        let a_ci = CharClass::from_byte(b'a').union(CharClass::from_byte(b'A'));
        let i_ci = CharClass::from_byte(b'i').union(CharClass::from_byte(b'I'));
        let s_ci = CharClass::from_byte(b's').union(CharClass::from_byte(b'S'));
        let c_ci = CharClass::from_byte(b'c').union(CharClass::from_byte(b'C'));
        let o_ci = CharClass::from_byte(b'o').union(CharClass::from_byte(b'O'));
        let m_ci = CharClass::from_byte(b'm').union(CharClass::from_byte(b'M'));
        let f_ci = CharClass::from_byte(b'f').union(CharClass::from_byte(b'F'));
        let n_ci = CharClass::from_byte(b'n').union(CharClass::from_byte(b'N'));

        g.byte_dispatch(
            vec![
                (
                    CharClass::from_byte(b'v'),
                    Box::new(|g| {
                        g.call_rule(GRule::KwVoid);
                    }),
                ),
                (
                    CharClass::from_byte(b'b'),
                    Box::new(|g| {
                        g.call_rule(GRule::KwBoolean);
                    }),
                ),
                (
                    a_ci,
                    Box::new(|g| {
                        g.call_rule(GRule::LambdaTypeKwA);
                    }),
                ),
                (
                    i_ci,
                    Box::new(|g| {
                        g.call_rule(GRule::LambdaTypeKwI);
                    }),
                ),
                (
                    CharClass::from_byte(b'r'),
                    Box::new(|g| {
                        g.call_rule(GRule::KwReal);
                    }),
                ),
                (
                    s_ci,
                    Box::new(|g| {
                        g.call_rule(GRule::LambdaTypeKwS);
                    }),
                ),
                (
                    c_ci,
                    Box::new(|g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwClassType);
                    }),
                ),
                (
                    o_ci,
                    Box::new(|g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwObject);
                    }),
                ),
                (
                    m_ci,
                    Box::new(|g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwMap);
                        g.optional(|g| {
                            g.call_rule(GRule::GenericMapArgs);
                        });
                    }),
                ),
                (
                    f_ci,
                    Box::new(|g| {
                        cfg_flags::v2(g);
                        g.call_rule(GRule::KwFunctionType);
                        g.optional(|g| {
                            g.call_rule(GRule::GenericFunctionArgs);
                        });
                    }),
                ),
                (
                    n_ci,
                    Box::new(|g| {
                        cfg_flags::v3(g);
                        g.call_rule(GRule::KwNull);
                    }),
                ),
            ],
            None,
        );
    });

    g.parser_rule(GRule::TypePrimary.as_str(), |g| {
        g.node(Node::TypePrimaryType, |g| {
            sipha::choices!(
                g,
                |g| {
                    g.call_rule(GRule::LambdaTypePrimaryNoIdent);
                },
                |g| {
                    g.call_rule(GRule::Ident);
                },
            );
        });
    });

    // Return type after `=>` in arrow lambdas (`dp => real dp["avg"]`).
    // Single primary only here; unions/nullable use full `ls_type` if needed later.
    g.parser_rule(GRule::LambdaReturnType.as_str(), |g| {
        g.node(Node::TypeExpr, |g| {
            g.node(Node::TypeUnionType, |g| {
                g.node(Node::TypeNullableType, |g| {
                    g.node(Node::TypePrimaryType, |g| {
                        g.call_rule(GRule::LambdaTypePrimaryNoIdent);
                    });
                    g.optional(|g| {
                        g.call_rule(GRule::OpQuestion);
                    });
                });
            });
        });
    });

    // <T> or <K,V>
    g.parser_rule(GRule::GenericTypeArgs.as_str(), |g| {
        g.call_rule(GRule::OpLt);
        g.call_rule(GRule::LsType);
        g.zero_or_more(|g| {
            g.call_rule(GRule::Comma);
            g.call_rule(GRule::LsType);
        });
        g.call_rule(GRule::TypeGt);
    });

    g.parser_rule(GRule::GenericMapArgs.as_str(), |g| {
        g.call_rule(GRule::OpLt);
        g.call_rule(GRule::LsType);
        g.call_rule(GRule::Comma);
        g.call_rule(GRule::LsType);
        g.call_rule(GRule::TypeGt);
    });

    // Function<arg, arg, ... -> ret>  (arrow optional in Java)
    g.parser_rule(GRule::GenericFunctionArgs.as_str(), |g| {
        g.call_rule(GRule::OpLt);
        g.optional(|g| {
            g.call_rule(GRule::LsType);
            g.zero_or_more(|g| {
                g.call_rule(GRule::Comma);
                g.call_rule(GRule::LsType);
            });
        });
        g.optional(|g| {
            g.call_rule(GRule::Arrow);
            g.call_rule(GRule::LsType);
        });
        g.call_rule(GRule::TypeGt);
    });

    // Closing `>` for generics (single `>`, not `>>`)
    g.parser_rule(GRule::TypeGt.as_str(), |g| {
        g.call_rule(GRule::OpGt);
    });
}
