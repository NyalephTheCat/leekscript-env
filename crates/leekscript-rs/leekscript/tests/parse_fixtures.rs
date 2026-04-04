use leekscript::syntax::kinds::K;
use std::path::Path;

use leekscript::{
    ExperimentalFeatures, LanguageOptions, Version, is_signature_stub_path, parse_doc,
    parse_signature_doc,
};
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

#[cfg(not(feature = "grammar-v4-only"))]
#[test]
fn leeklang_directive_sets_dialect_for_parse() {
    parse_doc(
        "// leeklang: dialect=v2\nVAR x = 1\nreturn x",
        Version::V4,
    )
    .expect("file directive selects v2 keyword rules");
}

#[test]
fn leeklang_directive_enables_experimental_on_top_of_cli_base() {
    parse_doc(
        "//! leeklang: experimental-let=true\nlet x = 1;\n",
        LanguageOptions::new(Version::V4, ExperimentalFeatures::NONE),
    )
    .expect("leading leeklang enables let without CLI experimental flags");
}

#[test]
fn experimental_let_declaration_uses_let_keyword() {
    parse_doc(
        "let x = 1",
        LanguageOptions::new(
            Version::V4,
            ExperimentalFeatures {
                let_bindings: true,
                ..ExperimentalFeatures::NONE
            },
        ),
    )
    .expect("experimental `let` keyword");
}

#[test]
fn v4_rejects_let_decl() {
    assert!(parse_doc("let x = 1", Version::V4).is_err());
}

#[test]
fn v4_break_continue_numeric_level_needs_experimental_loop_levels() {
    parse_doc("while (0) { break; }", Version::V4).expect("break;");
    parse_doc("while (0) { continue; }", Version::V4).expect("continue;");
    assert!(parse_doc("while (0) { break 2; }", Version::V4).is_err());
    assert!(parse_doc("while (0) { continue 2; }", Version::V4).is_err());
    let loop_only = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            loop_levels: true,
            ..ExperimentalFeatures::NONE
        },
    );
    parse_doc("while (0) { break 2; }", loop_only).expect("break 2");
    parse_doc("while (0) { continue 2; }", loop_only).expect("continue 2");
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
fn experimental_all_parses_reserved_statement_shapes() {
    let all = LanguageOptions::v4_experimental_all();
    parse_doc(
        "try { var x = 1 } catch (integer e) { } finally { }",
        all,
    )
    .expect("try/catch/finally");
    parse_doc("throw 1", all).expect("throw");
    parse_doc(r#"import "m""#, all).expect("import string");
    parse_doc("package a.b", all).expect("package");
    parse_doc("goto lbl", all).expect("goto");
    parse_doc("const x = 1", all).expect("const");
    parse_doc("export { var x = 1 }", all).expect("export block");
}

#[cfg(not(feature = "grammar-v4-only"))]
#[test]
fn v3_allows_match_as_identifier() {
    parse_doc("var match = 1", Version::V3).expect("`match` is not reserved in v3");
}

#[test]
fn experimental_match_statement_parses() {
    parse_doc(
        "match 1 { .. : return 0 }",
        LanguageOptions::new(
            Version::V4,
            ExperimentalFeatures {
                match_stmt: true,
                ..ExperimentalFeatures::NONE
            },
        ),
    )
    .expect("experimental `match`");
}

#[test]
fn doxygen_doc_comments_before_decls_parse() {
    parse_doc(
        "/** Module fn */\nfunction f() { }\n/** class */\nclass C {\n/** field */\ninteger n;\n/** method */\nm() { }\n}",
        Version::V4,
    )
    .expect("doxygen before decls");
    parse_doc("/// one\n/// two\nfunction g() { }", Version::V4).expect("slash-slash-slash lines");
    parse_doc("/** g */\nglobal integer x;", LanguageOptions::v4_experimental_all())
        .expect("doc on global");
    parse_doc(
        "/** c */\nconst y = 1;",
        LanguageOptions::new(
            Version::V4,
            ExperimentalFeatures {
                lexical_const: true,
                ..ExperimentalFeatures::NONE
            },
        ),
    )
    .expect("doc on const");
    parse_doc("/** v */\nvar z = 2;", Version::V4).expect("doc on var");
    parse_doc(
        "/** l */\nlet w = 3;",
        LanguageOptions::new(
            Version::V4,
            ExperimentalFeatures {
                let_bindings: true,
                ..ExperimentalFeatures::NONE
            },
        ),
    )
    .expect("doc on let");
    /* plain block comment must not break parsing */
    parse_doc("/* not doc */\nfunction h() {}", Version::V4).expect("plain comment before fn");
}

#[test]
fn v4_allows_match_as_identifier_like_v3() {
    parse_doc("var match = 1", Version::V4)
        .expect("`match` is not reserved in v4 without experimental match");
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

#[test]
fn v4_rejects_function_stub_without_signature_mode() {
    assert!(parse_doc("function f(integer x) => integer;", Version::V4).is_err());
}

#[test]
fn signature_mode_accepts_function_stub_and_block() {
    parse_signature_doc("function f(integer x) => integer;", Version::V4).expect("stub");
    parse_signature_doc("function g() { return 1; }", Version::V4).expect("block");
    parse_signature_doc(
        "function max() => Function<integer, integer => integer> | Function<real, real => real>;",
        Version::V4,
    )
    .expect("homogeneous number overload union");
}

#[test]
fn signature_mode_allows_default_on_stub_without_fn_optional_in_language_options() {
    parse_signature_doc(
        "function arrayFlatten<T>(Array<T> array, integer depth = 1) => Array<T>;",
        Version::V4,
    )
    .expect("default param on signature stub");
}

#[test]
fn signature_stub_path_heuristic() {
    assert!(is_signature_stub_path(Path::new("sig/std.sig.leek")));
    assert!(is_signature_stub_path(Path::new("std.sig.en.leek")));
    assert!(!is_signature_stub_path(Path::new("main.leek")));
}

#[test]
fn method_params_allow_default_without_experimental() {
    parse_doc(
        "class C { m(integer x = 0) { return x; } }",
        Version::V4,
    )
    .expect("method default param");
}

#[test]
fn function_default_param_requires_experimental() {
    assert!(parse_doc("function f(a = 1) { return a; }", Version::V4).is_err());
    let opts = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            fn_optional_params: true,
            ..ExperimentalFeatures::NONE
        },
    );
    parse_doc("function f(a = 1) { return a; }", opts).expect("experimental function default");
}

#[test]
fn template_params_require_experimental() {
    assert!(parse_doc("function id<T>(T x) { return x; }", Version::V4).is_err());
    let opts = LanguageOptions::new(
        Version::V4,
        ExperimentalFeatures {
            templates: true,
            ..ExperimentalFeatures::NONE
        },
    );
    parse_doc("function id<T>(T x) { return x; }", opts).expect("generic function");
    parse_doc("class Box<T> { T value; }", opts).expect("generic class");
    parse_doc("var g = function<U>(U x) { return x; };", opts).expect("generic anon function");
}

#[test]
fn signature_mode_accepts_templated_function_stub() {
    parse_signature_doc("function first<T>(Array<T> a) => T;", Version::V4).expect("stub with templates");
}

#[test]
fn signature_mode_accepts_param_named_function_after_function_type() {
    parse_signature_doc(
        "function fold<T, U>(Array<T> array, Function<T, U => U> function, U accumulator) => T;",
        Version::V4,
    )
    .expect("param named `function` after Function<…> type");
}
