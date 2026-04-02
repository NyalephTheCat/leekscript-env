//! Scope, resolution, and basic inference (`crate::scope`).

use leekscript::scope::{run_semantic_analysis, LeekTy, SymbolKind};
use leekscript::{parse_doc, Version};

#[test]
fn resolves_function_and_param() {
    let doc = parse_doc(
        "function add(integer a, integer b) { return a + b; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root());
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
    let a = run_semantic_analysis(doc.root());
    let x_sym = a.symbols.iter().find(|s| s.name == "x").expect("x symbol");
    assert_eq!(x_sym.kind, SymbolKind::Variable);
    assert_eq!(x_sym.inferred_ty, Some(LeekTy::Integer));
}

#[test]
fn class_field_and_method_in_scope() {
    let doc = parse_doc(
        "class C { integer n; m() { return n; } }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root());
    assert!(a.symbols.iter().any(|s| s.name == "C" && s.kind == SymbolKind::Class));
    assert!(a.symbols.iter().any(|s| s.name == "n" && s.kind == SymbolKind::Field));
    assert!(a.symbols.iter().any(|s| s.name == "m" && s.kind == SymbolKind::Method));
}

#[test]
fn interval_type_decl_and_literal_inference() {
    let doc = parse_doc(
        "function f() { Interval<integer> x; var y = [1..2]; return y; }",
        Version::V4,
    )
    .expect("parse");
    let a = run_semantic_analysis(doc.root());
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
    let a = run_semantic_analysis(doc.root());
    let _f = a.symbols.iter().find(|s| s.name == "f").expect("f");
    // return type not declared — check binary expr type exists
    let has_real = a.expr_types.values().any(|t| *t == LeekTy::Real);
    assert!(has_real, "expected real from int+real, {:?}", a.expr_types);
}

#[test]
fn undefined_name_diagnostic() {
    let doc = parse_doc("function f() { return UnknownName; }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root());
    assert!(
        a.diagnostics.iter().any(|d| d.message.contains("UnknownName")),
        "{:?}",
        a.diagnostics
    );
}

#[test]
fn member_property_not_reported_as_undefined() {
    let doc = parse_doc("function f() { var x = 1; return x.abs(); }", Version::V4).expect("parse");
    let a = run_semantic_analysis(doc.root());
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
    let a = run_semantic_analysis(doc.root());
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
    let a = run_semantic_analysis(doc.root());
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
    let rf = a.references.iter().find(|r| r.name == "String").expect("String ref");
    assert!(
        rf.resolved.is_some(),
        "instanceof String should resolve to the class symbol: {:?}",
        a.references
    );
    let sym = a.symbol(rf.resolved.expect("resolved")).expect("symbol");
    assert_eq!(sym.kind, SymbolKind::Class);
}
