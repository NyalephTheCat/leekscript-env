use leekscript::parse::{parse_doc, ExperimentalFeatures, LanguageOptions, Version};

#[test]
fn parse_smoke_global_decl_v4() {
    let lang = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    );
    let src = "global x; return x;";
    let _doc = parse_doc(src, lang).expect("parse_doc");
}

#[test]
fn parse_smoke_array_of_function_calls_v4() {
    use leekscript::ast::{Expr, Root, Stmt};
    use sipha::tree::ast::{AstNode, AstNodeExt};

    let lang = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    );
    let src = "var f = function(obj) { return obj.a } return [f({a: 'foo'}), f({a: 'bar'})]";
    let doc = parse_doc(src, lang).expect("parse_doc");
    let root = Root::cast(doc.root().clone()).expect("root");
    let mut array_expr_count: Option<usize> = None;
    for s in AstNodeExt::children::<Stmt>(root.syntax()) {
        if let Stmt::Return(r) = s {
            let e = r.expr().expect("return expr");
            match e {
                Expr::Array(ae) => {
                    let items: Vec<Expr> = AstNodeExt::children::<Expr>(ae.syntax()).collect();
                    array_expr_count = Some(items.len());
                }
                other => {
                    panic!("expected return array expr, got {other:?}");
                }
            }
        }
    }
    assert_eq!(array_expr_count, Some(2));
}

#[test]
fn parse_smoke_lambda_shape_v4() {
    use leekscript::ast::{Expr, LambdaExpr, Root, Stmt};
    use sipha::tree::ast::{AstNode, AstNodeExt};
    use sipha::types::FromSyntaxKind;
    use leekscript::syntax::kinds::K;

    let lang = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    );
    let src = "var f = x -> x return f(12)";
    let doc = parse_doc(src, lang).expect("parse_doc");
    let root = Root::cast(doc.root().clone()).expect("root");
    let mut found = None;
    for s in AstNodeExt::children::<Stmt>(root.syntax()) {
        if let Stmt::VarDecl(v) = s {
            for el in v.syntax().children() {
                if let sipha::tree::red::SyntaxElement::Node(n) = el {
                    if let Some(e) = Expr::cast(n.clone()) {
                        if let Expr::Lambda(le) = e {
                            let lam = LambdaExpr::cast(le.syntax().clone()).unwrap_or(le);
                            let toks: Vec<String> = lam
                                .syntax()
                                .child_tokens()
                                .map(|t| {
                                    let k = K::from_syntax_kind(t.kind())
                                        .map(|k| k.as_str().to_string())
                                        .unwrap_or_else(|| format!("?{:?}", t.kind()));
                                    format!("{k}({})", t.text())
                                })
                                .collect();
                            found = Some(toks.join(" "));
                        } else {
                            found = Some(format!("{e:?}"));
                        }
                        break;
                    }
                }
            }
        }
    }
    panic!("{found:?}");
}

