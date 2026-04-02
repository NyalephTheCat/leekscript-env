use leekscript::syntax::kinds::K;
use leekscript::{Version, parse_doc};
use sipha::prelude::*;

fn kind_to_name(k: SyntaxKind) -> Option<&'static str> {
    K::from_syntax_kind(k).map(K::as_str)
}

fn sexp(root: &SyntaxNode) -> String {
    let opts = sipha::extras::diff::SexpOptions {
        include_trivia: false,
        kind_to_name: Some(kind_to_name),
        max_token_len: None,
    };
    sipha::extras::diff::syntax_node_to_sexp(root, &opts)
}

#[test]
fn parses_basic_fixture_v4() {
    let src = include_str!("../testdata/basic.leek");
    let doc = parse_doc(src, Version::V4).expect("parse should succeed");
    let got = sexp(doc.root());
    assert!(got.contains("ROOT"));
    assert!(got.contains("NUMBER"));
    assert!(got.contains("STRING"));
    assert!(got.contains("PI"));
    assert!(got.contains("INFINITY"));
    assert!(got.contains("PAREN_EXPR"));
}

#[cfg(not(feature = "grammar-v4-only"))]
#[test]
fn v2_keywords_are_ascii_case_insensitive() {
    parse_doc("VAR x = 1\nReTuRn x", Version::V2).expect("v2: VAR / ReTuRn are keywords");
}

#[test]
fn v3_plus_keywords_are_case_sensitive() {
    // Reference lexer uses exact spelling for version >= 3.
    parse_doc("var ReTuRn = 1", Version::V4).expect("v4: ReTuRn is an identifier, not `return`");
}

#[cfg(not(feature = "grammar-v4-only"))]
#[test]
fn v2_treats_let_as_identifier_before_v3() {
    // Reference lexer only emits `LET` for version >= 3; before that `let` is `STRING`.
    parse_doc("var let = 1", Version::V2).expect("v2: `let` is a valid variable name");
}

#[test]
fn vnext_let_declaration_uses_let_keyword() {
    parse_doc("let x = 1", Version::VNext).expect("vnext keyword `let`");
}

#[test]
fn v4_rejects_let_decl() {
    assert!(parse_doc("let x = 1", Version::V4).is_err());
}

#[test]
fn v4_break_continue_without_level_vnext_allows_numeric_level() {
    parse_doc("while (0) { break; }", Version::V4).expect("break;");
    parse_doc("while (0) { continue; }", Version::V4).expect("continue;");
    assert!(parse_doc("while (0) { break 2; }", Version::V4).is_err());
    assert!(parse_doc("while (0) { continue 2; }", Version::V4).is_err());
    parse_doc("while (0) { break 2; }", Version::VNext).expect("break 2");
    parse_doc("while (0) { continue 2; }", Version::VNext).expect("continue 2");
}

#[test]
fn triple_less_lexes_as_single_token() {
    let src = "return a <<< b";
    let doc = parse_doc(src, Version::V4).expect("parse <<<");
    let s = sexp(doc.root());
    assert!(
        s.contains("TRIPLE_SHL"),
        "expected single <<< token, got: {s}"
    );
}

// Shapes using reserved keywords the reference `WordCompiler` does not implement yet.
#[test]
fn vnext_parses_reserved_statement_shapes() {
    parse_doc(
        "try { var x = 1 } catch (integer e) { } finally { }",
        Version::VNext,
    )
    .expect("try/catch/finally");
    parse_doc("throw 1", Version::VNext).expect("throw");
    parse_doc(r#"import "m""#, Version::VNext).expect("import string");
    parse_doc("package a.b", Version::VNext).expect("package");
    parse_doc("goto lbl", Version::VNext).expect("goto");
    parse_doc("const x = 1", Version::VNext).expect("const");
    parse_doc("export { var x = 1 }", Version::VNext).expect("export block");
}

#[cfg(not(feature = "grammar-v4-only"))]
#[test]
fn v3_allows_match_as_identifier() {
    parse_doc("var match = 1", Version::V3).expect("`match` is not reserved in v3");
}

#[test]
fn vnext_match_statement_parses() {
    parse_doc("match 1 { .. : return 0 }", Version::VNext).expect("match is vnext in this grammar");
}

#[test]
fn v4_allows_match_as_identifier_like_v3() {
    parse_doc("var match = 1", Version::V4).expect("`match` is not reserved in v4 without vnext");
}

#[cfg(not(feature = "grammar-v4-only"))]
#[test]
fn parses_v1_block_comment_short_form_only_in_v1() {
    let src = include_str!("../testdata/v1_block_comment_short.leek");

    // v1: accepted as a comment, then parses number.
    parse_doc(src, Version::V1).expect("v1 should accept /*/ comment");

    // v4: `/*/` is not a valid closed block comment; should fail.
    assert!(parse_doc(src, Version::V4).is_err());
}
