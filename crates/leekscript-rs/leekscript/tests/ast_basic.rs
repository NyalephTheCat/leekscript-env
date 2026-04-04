use leekscript::ast::{BinaryExpr, Expr, IncludeStmt, Root, Stmt, TypeExpr};
use leekscript::syntax::kinds::K;
use leekscript::{LanguageOptions, Version, parse_doc};
use sipha::tree::ast::{AstNode, AstNodeExt};
use sipha::tree::red::SyntaxNode;

fn stmts_v4(src: &str) -> Vec<Stmt> {
    let doc = parse_doc(src, Version::V4).expect("parse");
    let root = Root::cast(doc.root().clone()).expect("Root::cast");
    AstNodeExt::children::<Stmt>(root.syntax()).collect()
}

fn stmts_experimental_all(src: &str) -> Vec<Stmt> {
    let doc = parse_doc(src, LanguageOptions::v4_experimental_all()).expect("parse");
    let root = Root::cast(doc.root().clone()).expect("Root::cast");
    AstNodeExt::children::<Stmt>(root.syntax()).collect()
}

#[test]
fn root_casts_from_parse_tree() {
    let doc = parse_doc("include(\"a.leek\");", Version::V4).expect("parse");
    let root = Root::cast(doc.root().clone()).expect("Root::cast");
    assert!(root.syntax().child_nodes().next().is_some());
}

#[test]
fn include_stmt_and_string_path() {
    let doc = parse_doc("include(\"foo.leek\");", Version::V4).expect("parse");
    let root = Root::cast(doc.root().clone()).expect("Root::cast");
    // `SyntaxNode::children` is inherent; use `AstNodeExt::children` for typed CST nodes.
    let stmts: Vec<Stmt> = AstNodeExt::children::<Stmt>(root.syntax()).collect();
    assert_eq!(stmts.len(), 1, "expected one statement");

    let inc = stmts[0].as_include().expect("include stmt");
    let path = inc.path().expect("string path");
    assert_eq!(path.raw_text(), "\"foo.leek\"");
    assert_eq!(path.value(), "foo.leek");
}

#[test]
fn lit_str_is_ast_token() {
    use leekscript::ast::LitStr;
    use sipha::tree::ast::AstToken;

    let doc = parse_doc("include(\"z.leek\");", Version::V4).expect("parse");
    let root = Root::cast(doc.root().clone()).expect("root");
    let stmt = AstNodeExt::children::<Stmt>(root.syntax())
        .next()
        .expect("stmt");
    let lit = stmt.as_include().expect("inc").path().expect("lit");
    let t = lit.syntax();
    assert!(LitStr::can_cast(t.kind()));
    let round = LitStr::cast(t.clone()).expect("cast");
    assert_eq!(round.value(), lit.value());
}

#[test]
fn stmt_cast_from_include_node() {
    let doc = parse_doc("include('x.leek')\n", Version::V4).expect("parse");
    let root = Root::cast(doc.root().clone()).expect("Root::cast");
    let first = root.syntax().child_nodes().next().expect("stmt node");
    let stmt = Stmt::cast(first).expect("Stmt::cast");
    let inc: IncludeStmt = stmt.into_include().expect("into_include");
    assert_eq!(inc.path().map(|p| p.value()), Some("x.leek".into()));
}

#[test]
fn return_stmt_optional_expr() {
    let s = stmts_v4("return; return 42;");
    assert_eq!(s.len(), 2);
    let r0 = s[0].as_return().expect("return");
    assert!(r0.expr().is_none());
    let r1 = s[1].as_return().expect("return");
    assert!(r1.expr().is_some());
}

#[test]
fn var_decl_first_name() {
    let s = stmts_v4("var x = 1;");
    let v = s[0].as_var_decl().expect("var decl");
    assert_eq!(v.first_name(), Some("x".into()));
}

#[test]
fn let_decl_first_name_requires_experimental_let() {
    let s = stmts_experimental_all("let x = 1;");
    let v = s[0].as_var_decl().expect("var decl");
    assert_eq!(v.first_name(), Some("x".into()));
}

#[test]
fn function_decl_name() {
    let s = stmts_v4("function foo() {}");
    let f = s[0].as_function().expect("function");
    assert_eq!(f.name(), Some("foo".into()));
}

#[test]
fn function_decl_name_may_be_keyword_function() {
    let s = stmts_v4("function function() {}");
    let f = s[0].as_function().expect("function");
    assert_eq!(f.name(), Some("function".into()));
}

#[test]
fn typed_fn_param_may_be_named_function() {
    let s = stmts_v4("function foo(integer function) {}");
    let f = s[0].as_function().expect("function");
    let p = f.fn_params().next().expect("param");
    assert_eq!(p.name(), Some("function".into()));
}

#[test]
fn expr_stmt_wraps_expr() {
    let s = stmts_v4("1 + 2;");
    let e = s[0].as_expr().expect("expr stmt");
    let ex = e.expr().expect("expr");
    assert!(matches!(ex, Expr::Root(_)));
}

#[test]
fn expr_enum_casts_root_and_binary_descendants() {
    let doc = parse_doc("1 + 2;", Version::V4).expect("parse");
    let root = Root::cast(doc.root().clone()).expect("root");
    let stmt = AstNodeExt::children::<Stmt>(root.syntax())
        .next()
        .expect("stmt");
    let est = stmt.as_expr().expect("expr stmt");
    let Expr::Root(er) = est.expr().expect("root expr") else {
        panic!("expected Expr::Root");
    };
    let plus: SyntaxNode = er
        .syntax()
        .descendant_nodes()
        .find(|n| BinaryExpr::can_cast(n.kind()))
        .expect("binary +");
    let b = Expr::cast(plus).expect("Expr::cast");
    assert!(b.as_binary().is_some());
}

#[test]
fn break_and_continue_level() {
    let b = stmts_experimental_all("break 2;");
    assert_eq!(b[0].as_break().expect("break").level(), Some(2));
    let c = stmts_experimental_all("continue 3;");
    assert_eq!(c[0].as_continue().expect("continue").level(), Some(3));
}

/// `break`/`continue`/`return` used to accept the permissive `number` rule for levels; letter
/// sequences like `for`/`var` matched as NUMBER, so `continue for (var x in y)` mis-parsed.
#[test]
fn jump_then_foreach_with_var_after_keyword() {
    let src = concat!(
        "for (var a in xs) {\n",
        "    if (true) continue\n",
        "    for (var cell: var val in cov) { }\n",
        "}\n",
    );
    parse_doc(src, Version::V4).expect("nested foreach after continue without semicolon");

    parse_doc(
        "function f() {\n    return\n    for (var b in ys) {}\n}\n",
        Version::V4,
    )
    .expect("foreach after return without semicolon");
}

#[test]
fn if_while_do_while_conditions() {
    let s = stmts_v4("if (true) { return 1; } else { return 2; }");
    let ifs = s[0].as_if().expect("if");
    assert!(ifs.condition().is_some());
    assert!(ifs.then_block().is_some());
    assert!(ifs.else_block().is_some());

    let s = stmts_v4("while (0) { }");
    let w = s[0].as_while().expect("while");
    assert!(w.condition().is_some());

    let s = stmts_v4("do { } while (0);");
    let d = s[0].as_do_while().expect("do-while");
    assert!(d.condition().is_some());
}

/// `!=` must not be split into postfix `!` + assign `=` (v3+ postfix `!` would steal the first byte).
#[test]
fn if_condition_parses_noteq_operator() {
    use leekscript::syntax::kinds::K;
    use sipha::tree::tree_display::{TreeDisplayOptions, format_syntax_tree};
    use sipha::types::FromSyntaxKind;

    let doc = parse_doc("if (combo != null) {}", Version::V4).expect("parse");
    let tree = format_syntax_tree(doc.root(), &TreeDisplayOptions::default(), |k| {
        K::from_syntax_kind(k)
            .map(|k| k.as_str().to_string())
            .unwrap_or_else(|| "?".to_string())
    });
    assert!(tree.contains("NOTEQ"), "expected != token; tree:\n{tree}");
    assert!(
        !tree.contains("BANG"),
        "unexpected `!` token (postfix/unary); tree:\n{tree}"
    );
}

#[test]
fn foreach_switch_class_global_const() {
    let s = stmts_v4("for (k in xs) { }");
    let fe = s[0].as_foreach().expect("foreach");
    assert!(fe.iterable().is_some());

    let s = stmts_v4("switch (x) { case 1: break; }");
    let sw = s[0].as_switch().expect("switch");
    assert!(sw.expr().is_some());
    assert_eq!(sw.arms().count(), 1);

    let s = stmts_v4("class C {}");
    assert_eq!(s[0].as_class().expect("class").name(), Some("C".into()));

    // `global x` is ambiguous (`x` may be parsed as a type); use an explicit type + name.
    let s = stmts_v4("global integer x;");
    assert_eq!(
        s[0].as_global().expect("global").first_name(),
        Some("x".into())
    );

    let s = stmts_experimental_all("const a = 1;");
    assert_eq!(
        s[0].as_const().expect("const").first_name(),
        Some("a".into())
    );
}

#[test]
fn import_match_try_throw_export() {
    let s = stmts_experimental_all(r#"import "m";"#);
    let imp = s[0].as_import().expect("import");
    assert_eq!(imp.string_path().map(|p| p.value()), Some("m".into()));

    let s = stmts_experimental_all("match 1 { .. : return 0 }");
    let m = s[0].as_match().expect("match");
    assert!(m.scrutinee().is_some());

    let s = stmts_experimental_all("try { return 1; } catch (integer e) { } finally { }");
    let t = s[0].as_try().expect("try");
    assert!(t.try_block().is_some());
    assert_eq!(t.catch_clauses().count(), 1);
    assert!(t.finally_block().is_some());

    let s = stmts_experimental_all("throw 1;");
    let th = s[0].as_throw().expect("throw");
    assert!(th.expr().is_some());

    let s = stmts_experimental_all("export { var x = 1 }");
    let ex = s[0].as_export().expect("export");
    assert!(ex.block().is_some());
}

#[test]
fn function_body_block_stmts() {
    let s = stmts_v4("function f() { return 0; }");
    let f = s[0].as_function().expect("function");
    let body = f.body().expect("body block");
    assert_eq!(AstNodeExt::children::<Stmt>(body.syntax()).count(), 1);
}

#[test]
fn for_stmt_clause_exprs() {
    let s = stmts_v4("for (i = 0; i < 1; i = i + 1) { }");
    let fo = s[0].as_for().expect("for");
    assert!(fo.init_expr().is_some());
    assert!(fo.condition_expr().is_some());
    assert!(fo.step_expr().is_some());
}

#[test]
fn empty_stmt_double_semicolon_and_after_var_decl() {
    let s = stmts_experimental_all("function f() { ;; let x = 1;; }");
    let f = s[0].as_function().expect("function");
    let body = f.body().expect("body");
    assert_eq!(
        AstNodeExt::children::<Stmt>(body.syntax()).count(),
        4,
        "empty, empty, var decl, empty"
    );
}

#[test]
fn for_stmt_infinite_loop_double_semicolon() {
    let s = stmts_v4("for (;;) { }");
    let fo = s[0].as_for().expect("for");
    assert!(fo.init_expr().is_none());
    assert!(fo.condition_expr().is_none());
    assert!(fo.step_expr().is_none());
}

#[test]
fn for_stmt_empty_init_with_cond_and_step() {
    let s = stmts_v4("for (; i < n; i++) { }");
    let fo = s[0].as_for().expect("for");
    assert!(fo.init_expr().is_none());
    assert!(fo.condition_expr().is_some());
    assert!(fo.step_expr().is_some());
}

#[test]
fn class_extends_and_body() {
    let s = stmts_v4("class D extends Base {}");
    let c = s[0].as_class().expect("class");
    assert_eq!(c.name(), Some("D".into()));
    assert_eq!(c.extends(), Some("Base".into()));
    assert!(c.body().is_some());
}

#[test]
fn function_return_type_and_body() {
    let s = stmts_v4("function g() -> integer { return 0; }");
    let f = s[0].as_function().expect("function");
    assert_eq!(f.name(), Some("g".into()));
    assert!(f.return_type().is_some());
    assert!(f.body().is_some());
}

#[test]
fn function_typed_param_is_not_return_type() {
    // Parameters use `T name`; only `->` / `=>` introduces a result type.
    let s = stmts_v4("function h(integer a) { return 0; }");
    let f = s[0].as_function().expect("function");
    assert_eq!(f.name(), Some("h".into()));
    assert!(
        f.return_type().is_none(),
        "integer is a parameter type, not a return type"
    );
}

#[test]
fn global_optional_type() {
    // `global T name`, not `global name: T`
    let s = stmts_v4("global integer x;");
    let g = s[0].as_global().expect("global");
    assert!(g.type_expr().is_some());
    assert_eq!(g.first_name(), Some("x".into()));
}

#[test]
fn type_cst_union_and_type_node() {
    use leekscript::ast::TypeNode;
    use sipha::tree::ast::AstNodeExt;

    // Type unions in parameters are not supported the same way as in class method return types;
    // mirror the formatter fixture shape (`static T | U? f()`).
    let doc = parse_doc(
        "class C { static integer | real? f() { return 0; } }",
        Version::V4,
    )
    .expect("parse");
    let root = Root::cast(doc.root().clone()).expect("root");
    let stmt = AstNodeExt::children::<Stmt>(root.syntax())
        .next()
        .expect("stmt");
    let c = stmt.as_class().expect("class");
    let ty = first_class_member_result_type(c.syntax()).expect("method result type");
    assert!(matches!(
        TypeNode::cast(ty.syntax().clone()),
        Some(TypeNode::Root(_))
    ));
    let union = ty.union_type().expect("TypeUnionType under TypeExpr");
    assert!(matches!(
        TypeNode::cast(union.syntax().clone()),
        Some(TypeNode::Union(_))
    ));
    let parts = union.nullable_members();
    assert_eq!(parts.len(), 2, "integer | real?");
    assert!(!parts[0].is_optional(), "integer");
    assert!(parts[1].is_optional(), "real?");
    assert!(parts[0].primary().is_some());
    assert!(parts[1].primary().is_some());
    assert!(
        ty.syntax()
            .descendant_semantic_tokens()
            .iter()
            .any(|t| t.kind_as::<K>() == Some(K::BitOr)),
        "expected `|` token under union return type"
    );

    let doc2 = parse_doc(
        "class D { static Array<integer> g() { return 0; } }",
        Version::V4,
    )
    .expect("parse");
    let root2 = Root::cast(doc2.root().clone()).expect("root");
    let stmt2 = AstNodeExt::children::<Stmt>(root2.syntax())
        .next()
        .expect("stmt");
    let d = stmt2.as_class().expect("class");
    let ty2 = first_class_member_result_type(d.syntax()).expect("return type");
    let prim = ty2
        .union_type()
        .expect("union")
        .nullable_members()
        .into_iter()
        .next()
        .expect("one segment")
        .primary()
        .expect("primary");
    let args = prim.generic_argument_roots();
    assert_eq!(args.len(), 1, "Array<integer>");
    let inner = args[0]
        .union_type()
        .expect("inner union")
        .nullable_members();
    assert_eq!(inner.len(), 1);
}

/// First `->` / `=>` result `ls_type` (`K::TypeExpr`) under a class declaration.
fn first_class_member_result_type(syntax: &sipha::tree::red::SyntaxNode) -> Option<TypeExpr> {
    first_type_expr_descendant(syntax)
}

fn first_type_expr_descendant(syntax: &sipha::tree::red::SyntaxNode) -> Option<TypeExpr> {
    use sipha::tree::ast::AstNode;
    use sipha::types::IntoSyntaxKind;

    syntax.descendant_nodes().find_map(|n| {
        (n.kind() == K::TypeExpr.into_syntax_kind())
            .then(|| TypeExpr::cast(n))
            .flatten()
    })
}

#[test]
fn catch_clause_param_and_import_name_path() {
    let s = stmts_experimental_all("try { } catch (integer e) { }");
    let t = s[0].as_try().expect("try");
    let c = t.catch_clauses().next().expect("catch");
    assert_eq!(c.param_name(), Some("e".into()));
    assert!(c.param_type().is_some());

    let s = stmts_experimental_all("import foo.bar;");
    let i = s[0].as_import().expect("import");
    assert_eq!(i.name_segments(), Some(vec!["foo".into(), "bar".into()]));
}

#[test]
fn if_then_branch_wrapped_stmt() {
    let s = stmts_v4("if (true) return 1;");
    let i = s[0].as_if().expect("if");
    assert!(i.then_block().is_none());
    match i.then_branch().expect("then") {
        leekscript::ast::StmtBlock::Wrapped(_) => {}
        leekscript::ast::StmtBlock::Block(_) => panic!("expected wrapped stmt"),
    }
}

#[test]
fn lambda_optional_return_type_after_arrow() {
    // `=> real expr` — return type; `=> dp[...]` — untyped (no bare-ident return type).
    parse_doc("f(x => real x + 1);", Version::V4).expect("typed return");
    parse_doc("f(dp => dp[\"avg\"] as real);", Version::V4).expect("untyped body");
}

#[test]
fn lowercase_string_builtin_call() {
    parse_doc("var s = string(x);", Version::V4).expect("string() stringify");
}

/// Interval literals: open/closed bounds with `[` / `]` (leekscript-java `LeekInterval`).
#[test]
fn interval_literals_parse_like_java() {
    for src in [
        "return [..];",
        "return [1..2];",
        "return [1..2[;",
        "return [1..[;",
        "return ]..[;",
        "return ]1..2[;",
        "return ]..2];",
        "return intervalMin(]..[);",
    ] {
        let doc = parse_doc(src, Version::V4).unwrap_or_else(|e| panic!("parse {src:?}: {e:?}"));
        let root = Root::cast(doc.root().clone()).expect("root");
        assert!(
            root.syntax()
                .descendant_nodes()
                .any(|n| n.kind_as::<K>() == Some(K::IntervalExpr)),
            "expected IntervalExpr in {src:?}"
        );
    }
}
