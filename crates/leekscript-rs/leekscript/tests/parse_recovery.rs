//! Recovering parse (`parse_doc_with_recovery`) and partial rule parse (`parse_rule_at_offset`).

use leekscript::ast::{Root, Stmt};
use leekscript::syntax::kinds::K;
use leekscript::visit::{AstNodeExt, AstNodeTrait};
use leekscript::{Version, parse_doc, parse_doc_with_recovery, parse_rule_at_offset};

#[test]
fn strict_parse_fails_bad_top_level() {
    let src = "function f() {}\n???\nfunction g() {}\n";
    assert!(parse_doc(src, Version::V4).is_err());
}

#[test]
fn recovery_parses_valid_statements_around_garbage() {
    let src = "function f() {}\n???\nfunction g() {}\n";
    let r = parse_doc_with_recovery(src, Version::V4).expect("recover parse");
    let root = Root::cast(r.doc.root().clone()).expect("root");
    let kinds: Vec<K> = AstNodeExt::children::<Stmt>(root.syntax())
        .map(|s| s.syntax().kind_as::<K>().expect("stmt kind"))
        .collect();
    assert!(
        kinds.iter().filter(|k| **k == K::FunctionDecl).count() >= 2,
        "expected at least two functions, got {kinds:?}"
    );
}

#[test]
fn parse_rule_at_offset_stmt_fragment() {
    let src = "var x = 1;";
    let (doc, consumed) =
        parse_rule_at_offset(src, Version::V4, "stmt", 0).expect("fragment stmt");
    assert_eq!(consumed as usize, src.len());
    let root = doc.root();
    assert_eq!(
        root.kind_as::<K>(),
        Some(K::VarDecl),
        "stmt entrypoint root should be the statement node"
    );
}
