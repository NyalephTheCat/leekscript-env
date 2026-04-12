use leekscript::parse::{ExperimentalFeatures, LanguageOptions, Version, parse_doc};

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
    let lang = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    );
    let src = "var f = function(obj) { return obj.a } return [f({a: 'foo'}), f({a: 'bar'})]";
    let _doc = parse_doc(src, lang).expect("parse_doc");
}

#[test]
fn parse_smoke_unparen_multi_param_lambda_outside_call_args_v4() {
    let lang = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    );
    let src = "var f = a, b -> a + b return f(1, 2)";
    let _doc = parse_doc(src, lang).expect("parse_doc");
}

#[test]
fn parse_smoke_call_arg_list_does_not_absorb_comma_lambda_v4() {
    let lang = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    );
    let src = "function transform(arr, depth) { if (depth == 0) { return arr } return transform(arrayMap(arr, x -> x * 2), depth - 1) } return transform([1, 2, 3, 4], 3)";
    let _doc = parse_doc(src, lang).expect("parse_doc");
}

#[test]
fn parse_smoke_lambda_shape_v4() {
    let lang = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    );
    let src = "var f = x -> x return f(12)";
    let _doc = parse_doc(src, lang).expect("parse_doc");
}

#[test]
fn parse_smoke_return_f_call_has_call_expr_v4() {
    use leekscript::ast::{Root, Stmt};
    use sipha::tree::ast::{AstNode, AstNodeExt};
    use sipha::types::IntoSyntaxKind;

    let lang = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            lexical_const: true,
            exceptions: true,
            ..ExperimentalFeatures::NONE
        },
    );
    let src = "var f = -> 12 return f()";
    let doc = parse_doc(src, lang).expect("parse_doc");
    let root = Root::cast(doc.root().clone()).expect("root");
    let mut saw_call_expr = false;
    for s in AstNodeExt::children::<Stmt>(root.syntax()) {
        if let Stmt::Return(r) = s {
            let e = r.expr().expect("return expr");
            // Return expr should contain a `CallExpr` node for `f()`.
            for el in e.syntax().children() {
                if let sipha::tree::red::SyntaxElement::Node(n) = el {
                    if n.kind() == leekscript::syntax::kinds::Node::CallExpr.into_syntax_kind() {
                        saw_call_expr = true;
                    }
                }
            }
        }
    }
    assert!(
        saw_call_expr,
        "expected `return f()` to include CallExpr node"
    );
}
