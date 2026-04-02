//! LeekScript type grammar (aligned with leekscript-java `eatType` / `eatPrimaryType`).
use super::cfg_flags;
use crate::syntax::kinds::K;
use sipha::prelude::*;

pub fn define(g: &mut GrammarBuilder) {
    // type = union (| union)*
    g.parser_rule("ls_type", |g| {
        g.node(K::TypeExpr, |g| {
            g.call("type_union");
        });
    });

    g.parser_rule("type_union", |g| {
        g.node(K::TypeUnionType, |g| {
            g.call("type_nullable");
            g.zero_or_more(|g| {
                g.call("op_bitor");
                g.call("type_nullable");
            });
        });
    });

    g.parser_rule("type_nullable", |g| {
        g.node(K::TypeNullableType, |g| {
            g.call("type_primary");
            g.optional(|g| {
                g.call("op_question");
            });
        });
    });

    // Like `type_primary` but without bare `ident`, so `x => x + 1` is not parsed as typed lambda
    // `x` (type) `+ 1` (body). Used only after `=>` for explicit lambda return types (`=> real expr`).
    g.parser_rule("lambda_type_primary_no_ident", |g| {
        g.choices(vec![
            Box::new(|g| {
                g.call("kw_void");
            }),
            Box::new(|g| {
                g.call("kw_boolean");
            }),
            Box::new(|g| {
                g.call("kw_any");
            }),
            Box::new(|g| {
                g.call("kw_integer");
            }),
            Box::new(|g| {
                g.call("kw_real");
            }),
            Box::new(|g| {
                g.call("kw_string_type");
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.call("kw_class_type");
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.call("kw_object");
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.call("kw_array");
                g.optional(|g| {
                    g.call("generic_type_args");
                });
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.call("kw_set_type");
                g.optional(|g| {
                    g.call("generic_type_args");
                });
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.call("kw_map");
                g.optional(|g| {
                    g.call("generic_map_args");
                });
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.call("kw_function_type");
                g.optional(|g| {
                    g.call("generic_function_args");
                });
            }),
            Box::new(|g| {
                cfg_flags::v2(g);
                g.call("kw_interval_type");
                g.optional(|g| {
                    g.call("generic_type_args");
                });
            }),
            Box::new(|g| {
                cfg_flags::v3(g);
                g.call("kw_null");
            }),
        ]);
    });

    g.parser_rule("type_primary", |g| {
        g.node(K::TypePrimaryType, |g| {
            g.choices(vec![
                Box::new(|g| {
                    g.call("lambda_type_primary_no_ident");
                }),
                Box::new(|g| {
                    g.call("ident");
                }),
            ]);
        });
    });

    // Return type after `=>` in arrow lambdas (`dp => real dp["avg"]`).
    // Single primary only here; unions/nullable use full `ls_type` if needed later.
    g.parser_rule("lambda_return_type", |g| {
        g.node(K::TypeExpr, |g| {
            g.node(K::TypeUnionType, |g| {
                g.node(K::TypeNullableType, |g| {
                    g.node(K::TypePrimaryType, |g| {
                        g.call("lambda_type_primary_no_ident");
                    });
                    g.optional(|g| {
                        g.call("op_question");
                    });
                });
            });
        });
    });

    // <T> or <K,V>
    g.parser_rule("generic_type_args", |g| {
        g.call("op_lt");
        g.call("ls_type");
        g.zero_or_more(|g| {
            g.call("comma");
            g.call("ls_type");
        });
        g.call("type_gt");
    });

    g.parser_rule("generic_map_args", |g| {
        g.call("op_lt");
        g.call("ls_type");
        g.call("comma");
        g.call("ls_type");
        g.call("type_gt");
    });

    // Function<arg, arg, ... -> ret>  (arrow optional in Java)
    g.parser_rule("generic_function_args", |g| {
        g.call("op_lt");
        g.optional(|g| {
            g.call("ls_type");
            g.zero_or_more(|g| {
                g.call("comma");
                g.call("ls_type");
            });
        });
        g.optional(|g| {
            g.call("arrow");
            g.call("ls_type");
        });
        g.call("type_gt");
    });

    // Closing `>` for generics (single `>`, not `>>`)
    g.parser_rule("type_gt", |g| {
        g.call("op_gt");
    });
}
