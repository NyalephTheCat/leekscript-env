//! `LeekScript` parser: delimiter validation, then a growing **statement / expression** grammar lowered
//! to a rowan tree ([`leekscript_syntax`]).
//!
//! When the grammar does not apply (or you only need trivia-preserving token soup), use
//! [`leekscript_syntax::build_source_file_tree`] instead.

mod emit;
mod parse;

pub mod ast;

pub use ast::ParsedFile;

use leekscript_lexer::{Token, TokenKind};
use leekscript_syntax::LeekLanguage;
use rowan::SyntaxNode;

#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    /// Java `Error` name for registry lookup.
    pub reference: &'static str,
    pub span: leekscript_span::Span,
    pub message: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Delim {
    Paren,
    Bracket,
    Brace,
}

/// Match `()`, `[]`, `{}` across the token stream (comments already skipped by lexer).
#[must_use]
pub fn validate_delimiters(tokens: &[Token]) -> Vec<ParseDiagnostic> {
    let mut stack: Vec<(leekscript_span::Span, Delim)> = Vec::new();
    let mut out = Vec::new();

    for (ti, t) in tokens.iter().enumerate() {
        match t.kind {
            TokenKind::ParOpen => stack.push((t.span, Delim::Paren)),
            TokenKind::BracketOpen => {
                if matches!(stack.last(), Some((_, Delim::Bracket)))
                    && parse::bracket_open_closes_leek_interval(tokens, ti)
                {
                    stack.pop();
                } else {
                    stack.push((t.span, Delim::Bracket));
                }
            }
            TokenKind::BraceOpen => stack.push((t.span, Delim::Brace)),
            TokenKind::ParClose => close(&mut stack, &mut out, Delim::Paren, t.span),
            TokenKind::BracketClose => {
                if parse::bracket_close_may_start_interval(tokens, ti) {
                    stack.push((t.span, Delim::Bracket));
                } else {
                    close(&mut stack, &mut out, Delim::Bracket, t.span);
                }
            }
            TokenKind::BraceClose => close(&mut stack, &mut out, Delim::Brace, t.span),
            _ => {}
        }
    }

    for (span, d) in stack {
        let msg = match d {
            Delim::Paren => "unclosed '('",
            Delim::Bracket => "unclosed '['",
            Delim::Brace => "unclosed '{'",
        };
        out.push(ParseDiagnostic {
            reference: "END_OF_SCRIPT_UNEXPECTED",
            span,
            message: msg,
        });
    }

    out
}

fn close(
    stack: &mut Vec<(leekscript_span::Span, Delim)>,
    out: &mut Vec<ParseDiagnostic>,
    want: Delim,
    close_span: leekscript_span::Span,
) {
    match stack.last().copied() {
        None => {
            out.push(ParseDiagnostic {
                reference: "NO_BLOC_TO_CLOSE",
                span: close_span,
                message: "extra closing delimiter",
            });
        }
        Some((_, top)) if top == want => {
            stack.pop();
        }
        Some(_) => {
            out.push(ParseDiagnostic {
                reference: "CLOSING_PARENTHESIS_EXPECTED",
                span: close_span,
                message: "mismatched closing delimiter",
            });
            stack.pop();
        }
    }
}

/// Parse `src` + `tokens` into a **grammar-shaped** rowan tree (`SOURCE_FILE` → `VarDecl`, `Expr`, …).
///
/// Fails if delimiter validation fails or the statement/expression grammar hits an unexpected token.
/// On success, [`SyntaxNode::text()`] reproduces the full `src` (including trivia between tokens).
pub fn parse_file_green(
    src: &str,
    tokens: &[Token],
) -> Result<SyntaxNode<LeekLanguage>, Vec<ParseDiagnostic>> {
    let delim = validate_delimiters(tokens);
    if !delim.is_empty() {
        return Err(delim);
    }
    let (file, errors) = parse::parse_file(src, tokens);
    if !errors.is_empty() {
        return Err(errors);
    }
    let node = emit::emit_file(src, tokens, &file);
    debug_assert_eq!(
        node.text().to_string(),
        src,
        "grammar emitter must preserve full source text"
    );
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use leekscript_lexer::{Lexer, LexerConfig};

    #[test]
    fn balanced_ok() {
        let src = "var x = (1);";
        let (tok, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        assert!(validate_delimiters(&tok).is_empty());
    }

    #[test]
    fn parse_var_and_precedence() {
        let src = "var x = 1 + 2 * 3;\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(
            tree.contains("BinaryExpr"),
            "expected BinaryExpr in tree: {tree}"
        );
    }

    #[test]
    fn parse_comparison_bind_looser_than_add() {
        let src = "var x = 1 + 2 == 3;\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    #[test]
    fn parse_power_binds_tighter_than_mul() {
        let src = "var x = 2 * 3 ** 2;\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    #[test]
    fn parse_instanceof_roundtrip() {
        let src = "var x = [1] instanceof Array;\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    #[test]
    fn parse_new_map_interval_roundtrip() {
        let src =
            "var m = new Map(\"a\", 1, \"b\", 2);\nvar iv = new Interval(true, 1, true, 3);\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("NewExpr"), "{tree}");
    }

    /// Java `WordCompiler` bracket map / interval and `<` `>` set literals (`docs/spec/leekscript-language.md` §8).
    #[test]
    fn parse_map_interval_set_literal_roundtrip() {
        let src = concat!(
            "var em = [:];\n",
            "var m = [\"a\": 1, \"b\": 2];\n",
            "var mtrail = [\"a\": 1,];\n",
            "var atrail = [1, 2,];\n",
            "var iv0 = [..];\n",
            "var iv1 = [1..3];\n",
            "var iv2 = [..3];\n",
            "var iv3 = [1..];\n",
            "var st = <1, 2, 2>;\n",
            "var es = <>;\n",
            "var tc = <1,>;\n",
            "var ih = [1..3[;\n",
        );
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("MapLiteralExpr"), "{tree}");
        assert!(tree.contains("IntervalLiteralExpr"), "{tree}");
        assert!(tree.contains("SetLiteralExpr"), "{tree}");
    }

    #[test]
    fn parse_function_value_expr_in_map_roundtrip() {
        let src = "var m = [1: function(integer a, integer b) => integer { return a + b }];\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("FunctionValueExpr"), "{tree}");
    }

    /// Java v2+ `{` key `:` expr … `}` object literals (`docs/spec/leekscript-language.md` §8).
    #[test]
    fn parse_object_literal_roundtrip() {
        let src = concat!(
            "var o0 = {};\n",
            "var o1 = { a: 1, b: 2, };\n",
            "var o2 = { \"x\": 3, 4: 5, null: 0, };\n",
            "return o1.a + o2[\"x\"];\n",
        );
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("ObjectLiteralExpr"), "{tree}");
    }

    /// Bracket maps and brace objects must not share the same concrete syntax kind.
    #[test]
    fn map_literal_and_object_literal_distinct_cst_kinds() {
        let src = "var m = [a: 1]; var o = { a: 1 };\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        let tree = format!("{root:#?}");
        assert!(tree.contains("MapLiteralExpr"), "{tree}");
        assert!(tree.contains("ObjectLiteralExpr"), "{tree}");
    }

    #[test]
    fn parse_unary_and_logical_roundtrip() {
        let src = "var x = !true || -1;\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("UnaryExpr"), "{tree}");
    }

    #[test]
    fn for_in_and_array_literal_roundtrip() {
        let src = "for (var x in [1, 2]) { var y = x; }\nfor (k in \"ab\") { return k; }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("ForInStmt"), "{tree}");
        assert!(tree.contains("ArrayLiteralExpr"), "{tree}");
    }

    #[test]
    fn for_in_key_value_roundtrip() {
        let src = "for (i : var v in [1]) { return i; }\nfor (var k : v in [0]) { return k; }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("ForInKeyValueStmt"), "{tree}");
    }

    #[test]
    fn for_in_optional_type_and_at_roundtrip() {
        let src =
            "for (integer @x in [1]) { return x; }\nfor (Map<string, integer> var m in []) {}\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("ForInTypeAnn"), "{tree}");
    }

    #[test]
    fn for_loop_roundtrip() {
        let src = "for (var i = 0; i < 10; i = i + 1) { return i; }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("ForStmt"), "{tree}");
        assert!(tree.contains("ForInitVar"), "{tree}");
        assert!(tree.contains("ForAssign"), "{tree}");
    }

    /// `for` body may be a single statement (`Item.leek` style).
    #[test]
    fn for_in_single_statement_body_roundtrip() {
        let src = "for (var raw in []) push(0);\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    #[test]
    fn class_extends_and_protected_constructor_roundtrip() {
        let src = "class B extends A { protected constructor(integer id) { super(id); } }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    /// Java `LexicalParser`: `and` / `or` / `xor` are word-operator tokens (`docs/spec/leekscript-language.md` §2.5).
    #[test]
    fn word_operators_and_empty_stmt_roundtrip() {
        let src = "var x = true and false or true xor 1;\nnot x;\n;;\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("WordOp"), "{tree}");
        assert!(tree.contains("EmptyStmt"), "{tree}");
    }

    #[test]
    fn block_and_return() {
        let src = "{ return 1; }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    #[test]
    fn class_static_function_generic_method_roundtrip() {
        let src = "class T { static Function<any, any => integer|real> ascBy() {} }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    /// `ai/v2/utilities/Benchmark.leek`: static fields with generics, `final`, and static methods.
    #[test]
    fn postfix_inc_dec_roundtrip() {
        let src = "var i = 0;\ni++;\ni--;\nreturn i;\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("PostUpdateExpr"), "{tree}");
    }

    /// `class A { name part constructor(...) }` — consecutive lowercase idents are two untyped fields
    /// (Java VM `TestArray.partition`), not user-type `name` + field `part`.
    #[test]
    fn class_two_untyped_fields_before_constructor_roundtrip() {
        let src =
            "class A { name part constructor(name, part) { this.name = name this.part = part } }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(
            tree.matches("ClassFieldDecl").count() >= 2,
            "expected two ClassFieldDecl, got:\n{tree}"
        );
    }

    #[test]
    fn class_benchmark_style_members_parse() {
        let src = r#"class Benchmark {
    private static final Logger LOGGER = new Logger(Benchmark)
    private static Map<string, Map<string, real|integer>> dataPoints = [:]

    static start(string title) {
        return 0
    }

    static string format(real n) {
        return "" + n
    }
}
"#;
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        let tree = format!("{root:#?}");
        assert!(tree.contains("ClassFieldDecl"), "{tree}");
        assert!(tree.contains("FunctionDecl"), "{tree}");
    }

    #[test]
    fn function_decl_and_call_roundtrip() {
        let src = "function add(a, b) { return a + b; }\nreturn add(1, 2);\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("FunctionDecl"), "{tree}");
        assert!(tree.contains("CallExpr"), "{tree}");
    }

    /// Class members: generics with user class args, union types, `null` in unions (`Grid.leek`).
    #[test]
    fn class_field_and_method_java_types_roundtrip() {
        let src = r#"class Grid {
    private static Array<Cell> cells = []
    private static Set<integer> obstacleIds = <>
    static init() { }
    private static initNeighbors(Cell cell) { }
    private static Array<Cell> buildArea(Cell cell, integer|null baseArea, Array<Array<integer>> ranges) { }
    static Cell|null getCell(integer id) { }
    static boolean isObstacle(Cell cell) { }
    static Array<Cell> getCells() { }
    string string() { return ""; }
}
"#;
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    /// Java-style header with `=>` return type (e.g. `RegisterManager.leek`).
    #[test]
    fn class_static_function_nullary_generic_type_field() {
        let src = "class A { public static Function< => integer> f = function () => integer { return 1 } } return A.f()\n";
        let (tokens, err) = Lexer::new(src, LexerConfig { version: 2 }).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    #[test]
    fn function_arrow_return_type_roundtrip() {
        let src = "function objectToMap(Object obj) => Map {\n    return [:];\n}\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    /// Leek Wars–style `.sig.leek`: typed globals, signature-only functions, `include` as a name.
    #[test]
    fn sig_leek_global_type_and_function_stub_roundtrip() {
        let src = concat!(
            "global integer BULB_FIRE = 2;\n",
            "function getLife(integer entity = getEntity()) => integer;\n",
            "function include(string path) => void;\n",
        );
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("GlobalLeadingType"), "{tree}");
    }

    #[test]
    fn typed_local_without_initializer_roundtrip() {
        let src = "function f() {\n    real cellValue\n    real proximity = 1.0\n    return 0\n}\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty(), "{err:?}");
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
    }

    #[test]
    fn do_while_and_switch_roundtrip() {
        let src = "do { var x = 0; } while (0);\n\
switch (0) {\n  case 0:\n  case 1:\n    break;\n  default:\n    break;\n}\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("DoWhileStmt"), "{tree}");
        assert!(tree.contains("SwitchStmt"), "{tree}");
        assert!(tree.contains("CaseLabel"), "{tree}");
    }

    #[test]
    fn if_while_roundtrip() {
        let src = "if (1) { var x = 0; } else if (0) { var y = 1; }\nwhile (0) { return; }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        assert!(tree.contains("IfStmt"), "{tree}");
        assert!(tree.contains("WhileStmt"), "{tree}");
    }

    #[test]
    fn assign_break_continue_roundtrip() {
        let src = "var x = 1;\nx = 2;\nwhile (true) { break; continue; }\n";
        let (tokens, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let root = parse_file_green(src, &tokens).expect("parse");
        assert_eq!(root.text().to_string(), src);
        let tree = format!("{root:#?}");
        // `x = 2;` parses as an expression-statement with `AssignExpr` (same as Java expression grammar).
        assert!(tree.contains("AssignExpr"), "{tree}");
        assert!(tree.contains("BreakStmt"), "{tree}");
        assert!(tree.contains("ContinueStmt"), "{tree}");
    }

    #[test]
    fn unclosed_paren_delimiter() {
        let src = "(1 + 2";
        let (tok, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let d = validate_delimiters(&tok);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].reference, "END_OF_SCRIPT_UNEXPECTED");
    }

    #[test]
    fn mismatch_then_extra_close_reports_mismatch_and_no_bloc() {
        let src = "([)])";
        let (tok, err) = Lexer::new(src, LexerConfig::default()).tokenize();
        assert!(err.is_empty());
        let d = validate_delimiters(&tok);
        assert_eq!(
            d.iter()
                .filter(|x| x.reference == "CLOSING_PARENTHESIS_EXPECTED")
                .count(),
            2
        );
        assert_eq!(
            d.iter()
                .filter(|x| x.reference == "NO_BLOC_TO_CLOSE")
                .count(),
            1
        );
    }

    #[test]
    fn java_suite_edgecases_implicit_stmt_separators_parse() {
        let src = "function rec(depth, acc) { if (depth == 0) return acc push(acc, [depth, depth * 2]) return rec(depth - 1, acc) } return count(rec(50, []))\n";
        let (tokens, err) = Lexer::new(src, LexerConfig { version: 4 }).tokenize();
        assert!(err.is_empty(), "{err:?}");
        // Debug: the Java suite generator flattens newlines; this case relies on implicit stmt separators.
        // If this test fails, inspect token stream around `return acc push(...)`.
        for t in &tokens {
            if t.span.start >= 35 && t.span.start <= 90 {
                eprintln!(
                    "{:>3}..{:>3} {:?} {:?}",
                    t.span.start,
                    t.span.end,
                    t.kind,
                    &src[t.span.start as usize..t.span.end as usize]
                );
            }
        }
        if let Err(diags) = parse_file_green(src, &tokens) {
            panic!("parse failed: {diags:?}");
        }
    }

    #[test]
    fn java_suite_edgecases_search_implicit_stmt_separators_parse() {
        let src = "function search(depth, alpha, beta, isMax) { if (depth == 0) return randInt(-100, 100) for (var i = 0; i < 3; i++) { var score = search(depth - 1, alpha, beta, !isMax) if (isMax) { alpha = max(alpha, score) } else { beta = min(beta, score) } if (beta <= alpha) break } return isMax ? alpha : beta } return search(5, -1000, 1000, true) >= -1000\n";
        let (tokens, err) = Lexer::new(src, LexerConfig { version: 4 }).tokenize();
        assert!(err.is_empty(), "{err:?}");
        for t in &tokens {
            if (t.span.start >= 180 && t.span.start <= 230)
                || (t.span.start >= 275 && t.span.start <= 305)
            {
                eprintln!(
                    "{:>3}..{:>3} {:?} {:?}",
                    t.span.start,
                    t.span.end,
                    t.kind,
                    &src[t.span.start as usize..t.span.end as usize]
                );
            }
        }
        if let Err(diags) = parse_file_green(src, &tokens) {
            panic!("parse failed: {diags:?}");
        }
    }

    #[test]
    fn ternary_expression_smoke() {
        let src = "return isMax ? alpha : beta\n";
        let (tokens, err) = Lexer::new(src, LexerConfig { version: 4 }).tokenize();
        assert!(err.is_empty(), "{err:?}");
        for t in &tokens {
            eprintln!(
                "{:>3}..{:>3} {:?} {:?}",
                t.span.start,
                t.span.end,
                t.kind,
                &src[t.span.start as usize..t.span.end as usize]
            );
        }
        if let Err(diags) = parse_file_green(src, &tokens) {
            panic!("parse failed: {diags:?}");
        }
    }
}
