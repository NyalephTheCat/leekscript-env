//! Scope, resolution, and basic inference (`crate::scope`).

use leekscript::scope::{
    ExprTypeKey, LeekTy, SemanticCode, SemanticSeverity, SymbolKind, run_semantic_analysis,
};
use leekscript::{Version, parse_doc};

#[test]
fn doxygen_docs_on_symbols() {
    let src = "/** adds */\nfunction add(integer a, integer b) { return a + b; }\n/** Point */\nclass Point {\n/** x coord */\nreal x;\n/** distance */\ndistance() { return 0; }\n}\n/** main global */\nglobal integer G;\n/** entry */\nvar entry = 1;\n";
    let doc = parse_doc(src, Version::VNext).expect("parse");
    let a = run_semantic_analysis(doc.root(), Version::VNext);
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
    assert_eq!(s.declared_ty, Some(LeekTy::Class("String".to_string())));
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
