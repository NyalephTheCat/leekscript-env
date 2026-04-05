//! Recovering parse (`parse_doc_with_recovery`) and partial rule parse (`parse_rule_at_offset`).

use leekscript::ast::{Root, Stmt};
use leekscript::syntax::kinds::Node;
use leekscript::visit::{AstNodeExt, AstNodeTrait};
use leekscript::{
    GRule, ParseError, Version, parse_doc, parse_doc_with_recovery, parse_rule_at_offset,
    parse_signature_doc_with_recovery,
};

#[test]
fn strict_parse_fails_bad_top_level() {
    let src = "function f() {}\n???\nfunction g() {}\n";
    assert!(parse_doc(src, Version::V4).is_err());
}

#[test]
fn parse_error_message_uses_friendly_rule_names() {
    let src = "???\n";
    let err = parse_doc(src, Version::V4).expect_err("invalid top-level");
    let msg = err.format_with_source(src);
    assert!(
        !msg.contains("rule#"),
        "expected readable labels, got: {msg}"
    );
    assert!(
        msg.contains("function")
            || msg.contains("`function`")
            || msg.contains("statement")
            || msg.contains("expression")
            || msg.contains("identifier"),
        "expected keyword or construct hints, got: {msg}"
    );
    assert!(
        matches!(err, ParseError::Sipha(_)),
        "expected sipha parse failure"
    );
}

#[test]
fn signature_recovery_parses_function_stub() {
    let src = "function abs(integer|real a) => integer|real;\n";
    let r = parse_signature_doc_with_recovery(src, Version::V4).expect("signature recover parse");
    let root = Root::cast(r.doc.root().clone()).expect("root");
    let fn_count = AstNodeExt::children::<Stmt>(root.syntax())
        .filter(|s| s.syntax().kind_as::<Node>() == Some(Node::FunctionDecl))
        .count();
    assert_eq!(fn_count, 1, "expected one function stub decl");
}

#[test]
fn recovery_parses_valid_statements_around_garbage() {
    let src = "function f() {}\n???\nfunction g() {}\n";
    let r = parse_doc_with_recovery(src, Version::V4).expect("recover parse");
    let root = Root::cast(r.doc.root().clone()).expect("root");
    let kinds: Vec<Node> = AstNodeExt::children::<Stmt>(root.syntax())
        .map(|s| s.syntax().kind_as::<Node>().expect("stmt kind"))
        .collect();
    assert!(
        kinds.iter().filter(|k| **k == Node::FunctionDecl).count() >= 2,
        "expected at least two functions, got {kinds:?}"
    );
}

#[test]
fn parse_rule_at_offset_stmt_fragment() {
    let src = "var x = 1;";
    let (doc, consumed) =
        parse_rule_at_offset(src, Version::V4, GRule::Stmt.as_str(), 0).expect("fragment stmt");
    assert_eq!(consumed as usize, src.len());
    let root = doc.root();
    assert_eq!(
        root.kind_as::<Node>(),
        Some(Node::VarDecl),
        "stmt entrypoint root should be the statement node"
    );
}
