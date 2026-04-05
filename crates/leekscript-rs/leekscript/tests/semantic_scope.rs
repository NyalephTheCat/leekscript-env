//! Scope, resolution, and basic inference (`crate::scope`).

use leekscript::scope::{
    ExprTypeKey, LeekTy, Reference, SemanticCode, SemanticSeverity, SymbolKind,
    run_semantic_analysis,
};
use leekscript::{ExperimentalFeatures, LanguageOptions, Version, parse_doc, parse_signature_doc};

macro_rules! parse_with_templates {
    ($src:literal) => {
        parse_doc(
            $src,
            LanguageOptions::new(
                Version::V4,
                ExperimentalFeatures {
                    templates: true,
                    ..ExperimentalFeatures::NONE
                },
            ),
        )
        .expect("parse")
    };
}

#[test]
fn doxygen_docs_on_symbols() {
    let src = "/** adds */\nfunction add(integer a, integer b) { return a + b; }\n/** Point */\nclass Point {\n/** x coord */\nreal x;\n/** distance */\ndistance() { return 0; }\n}\n/** main global */\nglobal integer G;\n/** entry */\nvar entry = 1;\n";
    let doc = parse_doc(src, LanguageOptions::v4_experimental_all()).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let add = a
        .symbols
        .iter()
        .find(|s| s.name == "add" && s.kind == SymbolKind::Function)
        .expect("add");
    assert_eq!(add.doc_raw(), Some("adds"));
    assert_eq!(
        add.doc.as_ref().and_then(|d| d.brief.as_deref()),
        Some("adds")
    );
    let pt = a
        .symbols
        .iter()
        .find(|s| s.name == "Point" && s.kind == SymbolKind::Class)
        .expect("Point");
    assert_eq!(pt.doc_raw(), Some("Point"));
    let x_field = a
        .symbols
        .iter()
        .find(|s| s.name == "x" && s.kind == SymbolKind::Field)
        .expect("x");
    assert_eq!(x_field.doc_raw(), Some("x coord"));
    let dist = a
        .symbols
        .iter()
        .find(|s| s.name == "distance" && s.kind == SymbolKind::Method)
        .expect("distance");
    assert_eq!(dist.doc_raw(), Some("distance"));
    let g = a
        .symbols
        .iter()
        .find(|s| s.name == "G" && s.kind == SymbolKind::Global)
        .expect("G");
    assert_eq!(g.doc_raw(), Some("main global"));
    let entry = a
        .symbols
        .iter()
        .find(|s| s.name == "entry" && s.kind == SymbolKind::Variable)
        .expect("entry");
    assert_eq!(entry.doc_raw(), Some("entry"));
}

#[test]
fn doxygen_commands_on_function_symbol() {
    let src = r"/** \brief Sum two values.
 * \param a left
 * \param b right
 * \return a + b
 */
function add(integer a, integer b) { return a + b; }
";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let add = a
        .symbols
        .iter()
        .find(|s| s.name == "add" && s.kind == SymbolKind::Function)
        .expect("add");
    let d = add.doc.as_ref().expect("doc");
    assert_eq!(d.brief.as_deref(), Some("Sum two values."));
    assert_eq!(d.params.len(), 2);
    assert_eq!(d.params[0].name, "a");
    assert_eq!(d.params[0].description, "left");
    assert_eq!(d.params[1].name, "b");
    assert_eq!(d.returns.as_deref(), Some("a + b"));
}

#[test]
fn resolves_function_and_param() {
    let doc = parse_doc(
        "function add(integer a, integer b) { return a + b; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let add_sym = a
        .symbols
        .iter()
        .find(|s| s.name == "add" && s.kind == SymbolKind::Function);
    assert!(add_sym.is_some(), "function `add` should be declared");
    let a_ref = a
        .references
        .iter()
        .find(|r| r.name == "a" && r.resolved.is_some());
    assert!(a_ref.is_some(), "param `a` should resolve");
}

#[test]
fn infers_var_from_initializer() {
    let doc = parse_doc("function f() { var x = 1; return x; }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let x_sym = a.symbols.iter().find(|s| s.name == "x").expect("x symbol");
    assert_eq!(x_sym.kind, SymbolKind::Variable);
    assert_eq!(x_sym.inferred_ty, Some(LeekTy::Integer));
}

#[test]
fn class_field_and_method_in_scope() {
    let doc = parse_doc("class C { integer n; m() { return n; } }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        a.symbols
            .iter()
            .any(|s| s.name == "C" && s.kind == SymbolKind::Class)
    );
    assert!(
        a.symbols
            .iter()
            .any(|s| s.name == "n" && s.kind == SymbolKind::Field)
    );
    assert!(
        a.symbols
            .iter()
            .any(|s| s.name == "m" && s.kind == SymbolKind::Method)
    );
}

#[test]
fn class_field_name_before_type_keyword() {
    let doc = parse_doc(
        "class E { id string; name integer; m() { return id; } }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let id = a
        .symbols
        .iter()
        .find(|s| s.name == "id" && s.kind == SymbolKind::Field)
        .expect("field id");
    assert_eq!(id.declared_ty, Some(LeekTy::String));
    let name_f = a
        .symbols
        .iter()
        .find(|s| s.name == "name" && s.kind == SymbolKind::Field)
        .expect("field name");
    assert_eq!(name_f.declared_ty, Some(LeekTy::Integer));
    assert!(
        !a.diagnostics
            .iter()
            .any(|d| d.code == SemanticCode::UndefinedName),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn for_loop_init_var_visible_in_header_and_body() {
    let doc = parse_doc(
        "function f() { for (var i = 0; i < 1; i++) { return i; } return 0; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics
            .iter()
            .any(|d| d.code == SemanticCode::UndefinedName),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn anon_function_in_object_literal_params_resolve() {
    let src = r"class C {
    static Map<integer, Function<integer => integer>> m = [
        1: function(integer x) => integer { return x + 1; }
    ];
}";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics
            .iter()
            .any(|d| d.code == SemanticCode::UndefinedName && d.message.contains("`x`")),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn lambda_param_resolves_in_body() {
    let doc = parse_doc(
        "function f() { var g = (a => a + 1); return g; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics
            .iter()
            .any(|d| d.code == SemanticCode::UndefinedName),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn api_stub_max_min_function_union_single_declaration() {
    let src = r"function max() => Function<integer, integer => integer> | Function<real, real => real>;
function min() => Function<integer, integer => integer> | Function<real, real => real>;
function f() { var _a = max(1, 2); var _b = min(0, 3); }";
    let doc = parse_signature_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics.iter().any(|d| {
            d.code == SemanticCode::UndefinedName
                && (d.message.contains("max") || d.message.contains("min"))
        }),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn map_values_stub_call_in_class_static_initializer_checks() {
    let src = r#"function mapValues<T, U>(Map<T, U> map) => Array<U>;
class Benchmark {
    private static Map<string, Map<string, integer>> dataPoints = [:]
    static display() {
        Array<Map<string, integer>> points = mapValues(Benchmark.dataPoints)
    }
}"#;
    let doc = parse_signature_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn keyword_integer_lexes_as_one_token_not_in_teger() {
    let doc = parse_doc(
        "class C { integer hashcode = 17; } function f() { integer x = 0; return x; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics.iter().any(|d| d.message.contains("teger")),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn interval_type_decl_and_literal_inference() {
    let doc = parse_doc(
        "function f() { Interval<integer> x; var y = [1..2]; return y; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let x = a.symbols.iter().find(|s| s.name == "x").expect("x");
    assert_eq!(
        x.declared_ty,
        Some(LeekTy::Interval(Box::new(LeekTy::Integer)))
    );
    let y = a.symbols.iter().find(|s| s.name == "y").expect("y");
    assert_eq!(
        y.inferred_ty,
        Some(LeekTy::Interval(Box::new(LeekTy::Integer)))
    );
}

#[test]
fn coercion_integer_real_binary() {
    let doc = parse_doc("function f() { return 1 + 2.0; }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let _f = a.symbols.iter().find(|s| s.name == "f").expect("f");
    // return type not declared — check binary expr type exists
    let has_real = a.expr_types.values().any(|t| *t == LeekTy::Real);
    assert!(has_real, "expected real from int+real, {:?}", a.expr_types);
}

#[test]
fn incompatible_initializer_has_code_and_related_span() {
    let doc = parse_doc("function f() { integer x = true; }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert_eq!(bad.len(), 1, "{:?}", a.diagnostics);
    assert!(
        bad[0].related_span.is_some(),
        "expected type annotation span: {:?}",
        bad[0]
    );
    assert!(
        bad[0].message.contains("expected `integer`") && bad[0].message.contains("found `boolean`"),
        "message should name expected and found types: {:?}",
        bad[0].message
    );
}

/// Regression: initializer typing must use the full RHS expression, not the last nested `Expr`
/// (e.g. `C` inside `C.M` as the second argument to `min`).
#[test]
fn typed_var_initializer_min_with_static_field_no_false_mismatch() {
    let src =
        "class C { static final integer M = 40; static f() { integer n = min(1 + 1, C.M); } }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(
        bad.is_empty(),
        "unexpected IncompatibleInitializer: {:?}",
        bad
    );
}

#[test]
fn undefined_name_diagnostic() {
    let doc = parse_doc("function f() { return UnknownName; }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let undef: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::UndefinedName)
        .collect();
    assert!(
        undef.iter().any(|d| d.message.contains("UnknownName")),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn member_property_not_reported_as_undefined() {
    let doc = parse_doc("function f() { var x = 1; return x.abs(); }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics.iter().any(|d| d.message.contains("`abs`")),
        "name after `.` is not a lexical reference: {:?}",
        a.diagnostics
    );
}

#[test]
fn instanceof_rhs_type_names_not_undefined() {
    let doc = parse_doc(
        "function f(any location) { return location instanceof String || location instanceof pkg.Inner; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        a.diagnostics.is_empty(),
        "RHS of instanceof is a type, not a value ref: {:?}",
        a.diagnostics
    );
}

#[test]
fn instanceof_uppercase_type_registers_as_class_symbol() {
    let doc = parse_doc(
        "function f(any x) { return x instanceof String; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let str_sym = a
        .symbols
        .iter()
        .find(|s| s.name == "String" && s.kind == SymbolKind::Class);
    assert!(
        str_sym.is_some(),
        "String should be a class symbol: {:?}",
        a.symbols
    );
    let s = str_sym.expect("String");
    assert_eq!(
        s.declared_ty,
        Some(LeekTy::ClassObject("String".to_string()))
    );
    let rf = a
        .references
        .iter()
        .find(|r| r.name == "String")
        .expect("String ref");
    assert!(
        rf.resolved.is_some(),
        "instanceof String should resolve to the class symbol: {:?}",
        a.references
    );
    let sym = a.symbol(rf.resolved.expect("resolved")).expect("symbol");
    assert_eq!(sym.kind, SymbolKind::Class);
}

#[test]
fn instanceof_else_narrows_union_by_excluding_class() {
    let src = concat!(
        "class GameState {}\n",
        "class Consequences {}\n",
        "function f(GameState | Consequences state) {\n",
        "    if (state instanceof GameState) {\n",
        "        return;\n",
        "    } else {\n",
        "        Consequences c = state;\n",
        "    }\n",
        "}\n",
    );
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn instanceof_early_return_excludes_class_from_union() {
    let src = concat!(
        "class GameState {}\n",
        "class Consequences {}\n",
        "function f(GameState | Consequences state) {\n",
        "    if (state instanceof GameState) {\n",
        "        return;\n",
        "    }\n",
        "    Consequences c = state;\n",
        "}\n",
    );
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn instanceof_early_return_on_negated_guard_narrows_to_class() {
    let src = concat!(
        "class GameState {}\n",
        "class Consequences {}\n",
        "function f(GameState | Consequences state) {\n",
        "    if (!(state instanceof GameState)) {\n",
        "        return;\n",
        "    }\n",
        "    GameState g = state;\n",
        "}\n",
    );
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn instanceof_two_guards_sequential_narrowing() {
    let src = concat!(
        "class A {}\n",
        "class B {}\n",
        "class C {}\n",
        "function f(A | B | C x) {\n",
        "    if (x instanceof A) {\n",
        "        return;\n",
        "    }\n",
        "    if (x instanceof B) {\n",
        "        return;\n",
        "    }\n",
        "    C z = x;\n",
        "}\n",
    );
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn instanceof_else_return_narrows_then_only_path() {
    let src = concat!(
        "class GameState {}\n",
        "class Consequences {}\n",
        "function f(GameState | Consequences x) {\n",
        "    if (x instanceof GameState) {\n",
        "        GameState g = x;\n",
        "    } else {\n",
        "        return;\n",
        "    }\n",
        "}\n",
    );
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn instanceof_narrows_identifier_in_then_block() {
    let doc = parse_doc(
        "function f(any x) { if (x instanceof String) { return x; } return x; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let str_ty = LeekTy::Class("String".to_string());
    let mut x_tys = Vec::new();
    for r in a.references.iter().filter(|r| r.name == "x") {
        let key = ExprTypeKey::from_span(r.span);
        x_tys.push(a.expr_types.get(&key).cloned().unwrap_or(LeekTy::Unknown));
    }
    assert!(
        x_tys.iter().any(|t| *t == str_ty),
        "expected at least one narrowed `x` to String, got {x_tys:?}"
    );
    assert!(
        x_tys.iter().any(|t| *t == LeekTy::Any),
        "expected un-narrowed `x` (e.g. condition or outer return), got {x_tys:?}"
    );
}

#[test]
fn while_condition_instanceof_narrows_body() {
    let doc = parse_doc(
        "function f(any x) { while (x instanceof String) { x; } }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let str_ty = LeekTy::Class("String".to_string());
    let narrowed = a
        .references
        .iter()
        .filter(|r| r.name == "x")
        .any(|r| a.expr_type_at(r.span) == Some(&str_ty));
    assert!(narrowed, "body reference should narrow: {:?}", a.expr_types);
}

#[test]
fn ne_null_narrows_nullable_declared_type() {
    let doc = parse_doc(
        "function f() { string? s; if (s != null) { return s; } }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let string_only = LeekTy::String;
    let in_then = a
        .references
        .iter()
        .filter(|r| r.name == "s")
        .any(|r| a.expr_type_at(r.span) == Some(&string_only));
    assert!(
        in_then,
        "expected narrowed `s` to string in then, got {:?}",
        a.expr_types
    );
}

#[test]
fn foreach_infers_element_type_from_array_typed_var() {
    let doc = parse_doc(
        "function f() { Array<integer> a; for (x in a) { return x; } }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let x_sym = a.symbols.iter().find(|s| s.name == "x").expect("x");
    assert_eq!(x_sym.inferred_ty, Some(LeekTy::Integer));
}

#[test]
fn comparison_expression_infers_boolean() {
    let doc = parse_doc("function f() { return 1 < 2; }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        a.expr_types.values().any(|t| *t == LeekTy::Boolean),
        "expected boolean from `<`, {:?}",
        a.expr_types
    );
}

#[test]
fn and_chain_narrows_both_instanceof_facts() {
    let doc = parse_doc(
        "function f(any a, any b) { if (a instanceof String && b instanceof String) { return a; } }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let str_ty = LeekTy::Class("String".to_string());
    let a_narrowed = a
        .references
        .iter()
        .filter(|r| r.name == "a")
        .any(|r| a.expr_type_at(r.span) == Some(&str_ty));
    assert!(
        a_narrowed,
        "expected `a` narrowed in then, {:?}",
        a.expr_types
    );
}

#[test]
fn triple_equals_deprecated_warning_ls4() {
    let doc = parse_doc("function f() { return 1 === 2; }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let w: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::DeprecatedStrictEquality)
        .collect();
    assert_eq!(w.len(), 1, "{:?}", a.diagnostics);
    assert_eq!(w[0].severity, SemanticSeverity::Warning);
    assert!(w[0].message.contains("===") && w[0].message.contains("=="));
}

#[test]
fn triple_equals_not_warned_ls3() {
    let doc = parse_doc("function f() { return 1 === 2; }", Version::V3).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V3);
    assert!(
        !a.diagnostics
            .iter()
            .any(|d| d.code == SemanticCode::DeprecatedStrictEquality),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn deprecated_documented_function_call_warns() {
    let src = "/** @deprecated use bar */\nfunction old() {}\nfunction f() { old(); }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let old_sym = a
        .symbols
        .iter()
        .find(|s| s.name == "old" && s.kind == SymbolKind::Function)
        .expect("old fn symbol");
    assert!(
        old_sym.doc.as_ref().is_some_and(|d| d.deprecated.is_some()),
        "expected @deprecated on old(), doc={:?}",
        old_sym.doc
    );
    let w: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::DeprecatedCallable)
        .collect();
    assert_eq!(w.len(), 1, "{:?}", a.diagnostics);
    assert_eq!(w[0].severity, SemanticSeverity::Warning);
    assert!(w[0].message.contains("old") && w[0].message.contains("bar"));
}

#[test]
fn template_type_param_resolves_in_function() {
    let doc = parse_with_templates!("function id<T>(T x) { return x; }");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics
            .iter()
            .any(|d| { d.code == SemanticCode::UndefinedName && d.message.contains("`T`") }),
        "{:?}",
        a.diagnostics
    );
    let x = a
        .symbols
        .iter()
        .find(|s| s.name == "x" && s.kind == SymbolKind::Parameter)
        .expect("param x");
    assert_eq!(x.declared_ty, Some(LeekTy::TypeParam("T".into())));
}

#[test]
fn template_return_type_uses_type_param() {
    let doc = parse_with_templates!("function f<T>(T x) => T { return x; }");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let f = a
        .symbols
        .iter()
        .find(|s| s.name == "f" && s.kind == SymbolKind::Function)
        .expect("f");
    assert_eq!(
        f.declared_ty,
        Some(LeekTy::Function {
            params: vec![LeekTy::TypeParam("T".into())],
            ret: Box::new(LeekTy::TypeParam("T".into())),
        })
    );
}

#[test]
fn generic_class_field_and_method_use_class_type_param() {
    let doc = parse_with_templates!("class Box<T> { T value; m(T x) { return x; } }");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        a.symbols
            .iter()
            .any(|s| s.name == "T" && s.kind == SymbolKind::TypeParam),
        "class type param T missing: {:?}",
        a.symbols
    );
    let value = a
        .symbols
        .iter()
        .find(|s| s.name == "value" && s.kind == SymbolKind::Field)
        .expect("value field");
    assert_eq!(value.declared_ty, Some(LeekTy::TypeParam("T".into())));
    assert!(
        !a.diagnostics
            .iter()
            .any(|d| { d.code == SemanticCode::UndefinedName && d.message.contains("`T`") }),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn generic_anon_function_template_params_resolve() {
    let doc = parse_with_templates!(
        "function outer() { var g = function<U>(U x) { return x; }; return g; }"
    );
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics.iter().any(|d| {
            d.code == SemanticCode::UndefinedName
                && (d.message.contains("`U`") || d.message.contains("`x`"))
        }),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn instanceof_does_not_narrow_type_parameter() {
    let doc = parse_with_templates!(
        "function f<T>(T x) { if (x instanceof String) { return x; } return x; }"
    );
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let t_ty = LeekTy::TypeParam("T".into());
    let narrowed_in_then = a
        .references
        .iter()
        .filter(|r| r.name == "x" && r.resolved.is_some())
        .any(|r| a.expr_type_at(r.span) == Some(&LeekTy::Class("String".into())));
    assert!(
        !narrowed_in_then,
        "instanceof must not narrow a type parameter to a concrete class"
    );
    let still_t = a
        .references
        .iter()
        .filter(|r| r.name == "x" && r.resolved.is_some())
        .any(|r| a.expr_type_at(r.span) == Some(&t_ty));
    assert!(
        still_t,
        "expected `x` to keep type T in some ref, {:?}",
        a.expr_types
    );
}

#[test]
fn multiline_typed_var_decl_parses_as_single_ternary() {
    use leekscript::syntax::kinds::Node;

    let src = "function f(integer prevItemId) {\n\treal chainBonus = (prevItemId >= 0) ?\n\t\t1.0 :\n\t\t0.0\n}";
    let doc = parse_doc(src, LanguageOptions::v4_experimental_all()).expect("parse");
    let ternaries = doc
        .root()
        .descendant_nodes()
        .filter(|n| n.kind_as::<Node>() == Some(Node::TernaryExpr))
        .count();
    assert_eq!(
        ternaries, 1,
        "multiline ?: should be one ternary expression"
    );
    let vd = doc
        .root()
        .descendant_nodes()
        .find(|n| n.kind_as::<Node>() == Some(Node::VarDecl))
        .expect("var decl");
    assert!(
        vd.descendant_nodes()
            .any(|n| n.kind_as::<Node>() == Some(Node::TernaryExpr)),
        "ternary must be under VarDecl initializer, VarDecl children: {:?}",
        vd.child_nodes().map(|c| c.kind()).collect::<Vec<_>>()
    );
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(
        bad.is_empty(),
        "unexpected initializer errors: {:?}",
        a.diagnostics
    );
}

#[test]
fn empty_map_literal_bracket_colon_bracket_assigns_to_map() {
    let src = "function f() {\n\tMap<string, integer> m = [:]\n}";
    let doc = parse_doc(src, LanguageOptions::v4_experimental_all()).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn postfix_bang_strips_nullable_for_assignment() {
    let src = "function f() { string? s = \"a\"; string t = s!; }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn typed_param_integer_union_null_is_stored_as_nullable_integer() {
    let src = "function f(integer | null x) { }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let x = a
        .symbols
        .iter()
        .find(|s| s.name == "x" && s.kind == SymbolKind::Parameter)
        .expect("param x");
    assert_eq!(
        x.declared_ty,
        Some(LeekTy::Nullable(Box::new(LeekTy::Integer)))
    );
}

/// `[key: val]` must infer as `Map<K, V>`; a leading `Node::Trivia` node under `ArrayExpr` must not
/// break the `Expr` + `BracketMapExpr` shape (otherwise only `key` is seen → `Array<K>`).
#[test]
fn map_literal_key_colon_value_infers_map_even_with_leading_trivia_node() {
    let src =
        "class Cell {}\nfunction f(Cell from) {\n\tMap<Cell, integer> initial = [from: 0];\n}";
    let doc = parse_doc(src, LanguageOptions::v4_experimental_all()).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn dot_class_member_infers_class_object_for_runtime_class() {
    let src = "class C { integer n; } function g(C c) { var x = c.class; }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        a.expr_types
            .values()
            .any(|t| *t == LeekTy::ClassObject("C".into())),
        "expected Class<C> metaclass value, got {:?}",
        a.expr_types
    );
}

#[test]
fn super_member_infers_parent_class_object_when_extends() {
    let src = "class P {} class C extends P { static f(C c) { var x = c.super; } }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        a.expr_types
            .values()
            .any(|t| *t == LeekTy::ClassObject("P".into())),
        "expected parent class object, got {:?}",
        a.expr_types
    );
}

#[test]
fn static_field_accessible_on_class_name_receiver() {
    let src = "class C { static final integer Z = 0; } function f() { integer y = C.Z; }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let z = a
        .symbols
        .iter()
        .find(|s| s.name == "Z" && s.kind == SymbolKind::Field);
    assert!(
        z.is_some_and(|s| s.is_static),
        "static field Z missing or not static: {:?}",
        a.symbols
            .iter()
            .filter(|s| s.name == "Z")
            .collect::<Vec<_>>()
    );
    let bad: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::IncompatibleInitializer)
        .collect();
    assert!(bad.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn nullable_member_chain_propagates_type_and_warns() {
    let src = r"class Consequences { real score; }
class Combo { Consequences? consequences; }
function f(Combo combo) { var x = combo.consequences.score; }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let score_ty = LeekTy::Nullable(Box::new(LeekTy::Real));
    assert!(
        a.expr_types.values().any(|t| *t == score_ty),
        "expected optional real for chained access, got {:?}",
        a.expr_types
    );
    let warns: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == SemanticCode::NullableChainAccess && d.severity == SemanticSeverity::Warning
        })
        .collect();
    assert_eq!(warns.len(), 1, "{:?}", a.diagnostics);
    assert!(warns[0].message.contains("member"));
}

#[test]
fn nullable_member_chain_postfix_bang_skips_warning_and_unwraps() {
    let src = r"class Consequences { real score; }
class Combo { Consequences? consequences; }
function f(Combo combo) { var x = combo.consequences!.score; }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    assert!(
        !a.diagnostics
            .iter()
            .any(|d| d.code == SemanticCode::NullableChainAccess),
        "{:?}",
        a.diagnostics
    );
    assert!(
        a.expr_types.values().any(|t| *t == LeekTy::Real),
        "expected plain real after !, got {:?}",
        a.expr_types
    );
}

#[test]
fn nullable_array_index_propagates_and_warns() {
    let src = "function f(Array<real>? arr) { var x = arr[0]; }";
    let doc = parse_doc(src, LanguageOptions::v4_experimental_all()).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let elem_ty = LeekTy::Nullable(Box::new(LeekTy::Real));
    assert!(
        a.expr_types.values().any(|t| *t == elem_ty),
        "expected optional real from nullable array index, got {:?}",
        a.expr_types
    );
    assert!(
        a.diagnostics.iter().any(|d| {
            d.code == SemanticCode::NullableChainAccess
                && d.severity == SemanticSeverity::Warning
                && d.message.contains("indexing")
        }),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn logical_or_precedence_looser_than_relational_gt() {
    use leekscript::ast::IfStmt;
    use leekscript::syntax::kinds::{Lex, Node};
    use sipha::prelude::AstNode;
    let src = "function f() { if (best == null || combo.score > best.score) {} }";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let ifn = doc
        .root()
        .descendant_nodes()
        .find_map(IfStmt::cast)
        .expect("if");
    let cond_expr = ifn.condition().expect("cond");
    let cond = cond_expr.syntax();
    let or_only = cond
        .descendant_nodes()
        .find(|n| {
            n.kind_as::<Node>() == Some(Node::BinaryExpr)
                && n.child_tokens().any(|t| t.kind_as::<Lex>() == Some(Lex::OrOr))
        })
        .expect("|| BinaryExpr");
    let gt_only = cond
        .descendant_nodes()
        .find(|n| {
            n.kind_as::<Node>() == Some(Node::BinaryExpr)
                && n.child_tokens().any(|t| t.kind_as::<Lex>() == Some(Lex::Gt))
        })
        .expect("> BinaryExpr");
    let r_or = or_only.text_range();
    let r_gt = gt_only.text_range();
    assert!(
        r_or.start < r_gt.start && r_or.end >= r_gt.end,
        "expected `||` to wrap `>` (outer {r_or:?}, inner {r_gt:?})"
    );
}

/// `best == null || combo.score > best.score` — `||` looser than `>`; RHS narrows `best` for `.score`.
#[test]
fn or_rhs_narrows_after_eq_null_lhs() {
    let src = r"class Consequences { real score; }
function f(Consequences? best, Consequences combo) {
	if (best == null || combo.score > best.score) {
		best = combo;
	}
}";
    let doc = parse_doc(src, Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::V4);
    let nullable_chain = a
        .diagnostics
        .iter()
        .filter(|d| d.code == SemanticCode::NullableChainAccess)
        .collect::<Vec<_>>();
    assert!(
        nullable_chain.is_empty(),
        "did not expect nullable-chain warnings on narrowed `best.score`: {:?}",
        nullable_chain
    );
    let best_refs: Vec<&Reference> = a.references.iter().filter(|r| r.name == "best").collect();
    let narrowed = best_refs.iter().any(|r| {
        a.expr_types.get(&ExprTypeKey::from_span(r.span))
            == Some(&LeekTy::Class("Consequences".into()))
    });
    assert!(
        narrowed,
        "expected `best` typed as non-null Consequences somewhere in RHS, refs={best_refs:?} types={:?}",
        a.expr_types
    );
}
