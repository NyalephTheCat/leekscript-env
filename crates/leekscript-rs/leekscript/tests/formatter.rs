use leekscript::format::{FormatOptions, SemicolonStyle, format_document};
use leekscript::{LanguageOptions, Version};

#[test]
fn formats_let_with_spaces() {
    let out = format_document("let x=1;", LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert_eq!(out.trim(), "let x = 1;");
}

#[test]
fn semicolon_style_always_inserts_missing() {
    let opts = FormatOptions {
        semicolon_style: SemicolonStyle::Always,
        ..Default::default()
    };
    let out = format_document("function f() { let x = 1 }", LanguageOptions::v4_experimental_all(), &opts).unwrap();
    assert!(
        out.contains("let x = 1;"),
        "expected inserted semicolon, got:\n{out}"
    );
}

#[test]
fn semicolon_style_only_needed_drops_optional_semicolon() {
    let opts = FormatOptions {
        semicolon_style: SemicolonStyle::OnlyNeeded,
        ..Default::default()
    };
    let out = format_document("function f() { let x = 1; }", LanguageOptions::v4_experimental_all(), &opts).unwrap();
    assert!(
        !out.contains("let x = 1;"),
        "only-needed should omit var semicolon, got:\n{out}"
    );
}

#[test]
fn semicolon_style_only_needed_keeps_bare_return_break_continue() {
    let opts = FormatOptions {
        semicolon_style: SemicolonStyle::OnlyNeeded,
        ..Default::default()
    };
    let out = format_document("function f() { return; }", Version::V4, &opts).unwrap();
    assert!(out.contains("return;"), "got:\n{out}");

    let out = format_document(
        "function f() { while (true) { break; } }",
        Version::V4,
        &opts,
    )
    .unwrap();
    assert!(out.contains("break;"), "got:\n{out}");

    let out = format_document(
        "function f() { while (true) { continue; } }",
        Version::V4,
        &opts,
    )
    .unwrap();
    assert!(out.contains("continue;"), "got:\n{out}");
}

#[test]
fn semicolon_style_only_needed_omits_after_return_value() {
    let opts = FormatOptions {
        semicolon_style: SemicolonStyle::OnlyNeeded,
        ..Default::default()
    };
    let out = format_document("function f() { return 1; }", Version::V4, &opts).unwrap();
    assert!(
        !out.contains("return 1;"),
        "only-needed should omit semicolon after return expr, got:\n{out}"
    );
    assert!(out.contains("return 1"), "got:\n{out}");
}

#[test]
fn formats_double_semicolon_empty_stmts() {
    let out = format_document(
        "function f() { ;; }",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.matches(';').count() >= 2,
        "expected two empty statements, got:\n{out}"
    );
}

#[test]
fn formats_for_infinite_loop_header() {
    let out = format_document("for(;;){}", Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains(";;") || out.contains("; ;"),
        "expected classic for header, got:\n{out}"
    );
}

#[test]
fn formats_for_header_with_spaces_after_semicolons() {
    let out = format_document(
        "for (var i = 0;i < n;i++) { }",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("0; i < n; i++"),
        "expected spaces after `;` in classic for header, got:\n{out}"
    );
}

#[test]
fn formats_space_after_nullable_type_question_before_name() {
    let out = format_document(
        "class C { Map?get(string key) {} }",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("Map? get("),
        "expected space after `?` on nullable type before member name, got:\n{out}"
    );
}

#[test]
fn formats_arrow_lambda_without_duplicate_arrow() {
    let out = format_document(
        "let f = x -> x + 1;",
        LanguageOptions::v4_experimental_all(),
        &FormatOptions::default(),
    )
    .unwrap();
    assert_eq!(out.trim(), "let f = x -> x + 1;");
    assert!(
        !out.contains("-> x ->"),
        "lookahead probe must not leave a stray `->` in output:\n{out}"
    );
}

#[test]
fn formats_lambda_block_return_with_space_before_paren() {
    let out = format_document(
        "let g = () -> { return(1); };",
        LanguageOptions::v4_experimental_all(),
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("return (1)"),
        "expected space after `return` before `(`, got:\n{out}"
    );
}

#[test]
fn formats_new_expression_on_one_line() {
    let out = format_document(
        "var LOGGER = new Logger(\"Main\");",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("new Logger("),
        "expected single-line new expr, got:\n{out}"
    );
    assert!(
        !out.contains("new\n"),
        "should not break after `new`:\n{out}"
    );
}

#[test]
fn formats_noteq_without_splitting_operator() {
    let out = format_document(
        "if (combo != null) {}",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(out.contains("!="), "expected `!=` preserved, got:\n{out}");
    assert!(!out.contains("! ="), "should not split `!=`, got:\n{out}");
}

#[test]
fn formats_space_after_paren_before_continue() {
    let out = format_document(
        "if (geo == null) continue",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains(") continue") || out.contains(")\ncontinue"),
        "expected space or newline before continue, got:\n{out}"
    );
}

#[test]
fn block_lines_share_indent_after_rparen_stmt() {
    // Regression: `)` before the next line must not insert an extra leading space (RParen+Ident).
    let out = format_document(
        "if (1) {\n    f()\n    g()\n}",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    let g_line = out.lines().find(|l| l.contains("g()")).expect("g() line");
    assert_eq!(
        g_line.chars().take_while(|c| *c == ' ').count(),
        4,
        "expected 4-space indent, got:\n{out}"
    );
}

#[test]
fn indent_width_from_directive() {
    let src = "// leekfmt: indent-width=2\nfunction f() {\nlet x=1;\n}";
    let out = format_document(src, LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out.contains("  let x = 1;"),
        "expected 2-space indent, got:\n{out}"
    );
}

#[test]
fn file_wide_inner_directive_formats_lines_above() {
    let src = "function f() {\nlet x=1 + 2;\n}\n//! leekfmt: space-around-binary-ops=false\n";
    let out = format_document(src, LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out.contains("1+2") && !out.contains("1 + 2"),
        "file-wide //! should apply to body above the directive, got:\n{out}"
    );

    let src_outer = "function f() {\nlet x=1 + 2;\n}\n// leekfmt: space-around-binary-ops=false\n";
    let out_outer = format_document(src_outer, LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out_outer.contains("1 + 2"),
        "ordinary // directive should not change lines above it, got:\n{out_outer}"
    );
}

#[test]
fn off_on_preserves_mangled() {
    let src = "a;\n// leekfmt: off\nlet x=1+  2;\n// leekfmt: on\nb;\n";
    let out = format_document(src, LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out.contains("let x=1+  2;"),
        "verbatim region should stay:\n{out}"
    );
    assert!(out.contains("a;"), "formatted first stmt:\n{out}");
}

#[test]
fn space_around_binary_ops_false() {
    let o = FormatOptions {
        space_around_binary_ops: false,
        ..Default::default()
    };
    let out = format_document("1 + 2;", Version::V4, &o).unwrap();
    assert_eq!(out.trim(), "1+2;");
}

#[test]
fn ignore_next_line() {
    let src = "// leekfmt: ignore-next-line\nlet   y=2;\nlet z=3;\n";
    let out = format_document(src, LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(out.contains("let   y=2;"), "next line kept:\n{out}");
    assert!(
        out.contains("let z = 3;") || out.contains("let z=3"),
        "following line formatted:\n{out}"
    );
}

#[test]
fn block_comment_directive() {
    let src = "/* leekfmt: use-tabs=true */\nfunction f() {\nlet a=1;\n}\n";
    let out = format_document(src, LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out.contains("\tlet a = 1;"),
        "expected tab-indented body:\n{out}"
    );
}

#[test]
fn spaces_around_keyword_binary_ops_before_paren() {
    // Use a context where `instanceof`/`in` are unambiguously in expression position.
    let out = format_document(
        "if (a instanceof(b)) {}",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("a instanceof (b)"),
        "expected space after `instanceof` before `(`, got:\n{out}"
    );

    let out = format_document("if (a in(b)) {}", Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("a in (b)"),
        "expected space after `in` before `(`, got:\n{out}"
    );

    // `as` is a cast in this grammar: `expr as Type`
    let out = format_document(
        "let x=(a as integer);",
        LanguageOptions::v4_experimental_all(),
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("a as integer"),
        "expected spaces around `as`, got:\n{out}"
    );
}

#[test]
fn spaces_after_assign_before_paren() {
    let out = format_document("let x=(y);", LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out.trim().contains("let x = (y);"),
        "expected space after `=` before `(`, got:\n{out}"
    );
}

#[test]
fn spaces_between_class_member_modifiers() {
    let out = format_document(
        "class C{private static final foo(){}}",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("private static final foo()"),
        "expected spaces between modifiers, got:\n{out}"
    );
}

#[test]
fn spaces_between_adjacent_keywords() {
    let out = format_document(
        "if (a) {} else if(b) {}",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("else if"),
        "expected `else if` to keep a space, got:\n{out}"
    );

    let out = format_document("do{}while(true);", Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("do {") && out.contains("} while"),
        "expected `do while` spacing, got:\n{out}"
    );
}

#[test]
fn space_after_comma_default() {
    let out =
        format_document("let x = f(1,2);", LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out.contains("f(1, 2)"),
        "expected space after comma, got:\n{out}"
    );
}

#[test]
fn space_after_comma_false() {
    let o = FormatOptions {
        space_after_comma: false,
        ..Default::default()
    };
    let out = format_document("let x = f(1, 2);", LanguageOptions::v4_experimental_all(), &o).unwrap();
    assert!(
        out.contains("f(1,2)"),
        "expected no space after comma, got:\n{out}"
    );
    assert!(
        !out.contains("1, 2"),
        "unexpected space after comma:\n{out}"
    );
}

#[test]
fn space_before_brace_after_rparen_and_class_name() {
    let out = format_document(
        "function f(){return 1;}",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("() {") || out.contains("(){\n"),
        "expected space before `{{` after `)`, got:\n{out}"
    );

    let out = format_document("class C{}", Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("class C {") || out.contains("class C\n"),
        "expected space before class body `{{`, got:\n{out}"
    );
}

#[test]
fn space_before_brace_after_keyword_return_type() {
    let out = format_document(
        "function objectToMap(Object obj) => Map{ return 1; }",
        Version::V4,
        &FormatOptions::default(),
    )
    .unwrap();
    assert!(
        out.contains("=> Map {"),
        "expected space before `{{` after keyword return type (e.g. MapKw), got:\n{out}"
    );
}

#[test]
fn wraps_after_comma_when_line_exceeds_width() {
    let o = FormatOptions {
        // `    return g("aaaa",` ends at column 20; space + `"bbbb"` exceeds this width.
        line_width: 26,
        indent_width: 4,
        ..Default::default()
    };
    let out = format_document(
        "function f() {\nreturn g(\"aaaa\", \"bbbb\");\n}",
        Version::V4,
        &o,
    )
    .unwrap();
    assert!(
        out.contains(",\n") || out.contains(",\r\n"),
        "expected newline after comma when over line width, got:\n{out}"
    );
}

#[test]
fn line_width_zero_disables_comma_wrap() {
    let o = FormatOptions {
        line_width: 0,
        ..Default::default()
    };
    let out = format_document(
        "function f() {\nreturn g(\"aaaa\", \"bbbb\");\n}",
        Version::V4,
        &o,
    )
    .unwrap();
    let one_line_return = out
        .lines()
        .any(|l| l.contains("return g") && l.contains("\"bbbb\""));
    assert!(
        one_line_return,
        "expected single-line return when line_width=0, got:\n{out}"
    );
}

#[test]
fn blank_lines_between_class_members_default() {
    let src = "class C {\nvoid a() {}\nvoid b() {}\n}";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("}\n\n    void b"),
        "expected blank line between class methods by default, got:\n{out}"
    );
}

#[test]
fn blank_lines_after_class_default() {
    let src = "class C {}\nfunction f() {}";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("}\n\n\nfunction"),
        "expected two extra blank lines after top-level class by default, got:\n{out}"
    );
}

#[test]
fn blank_lines_after_class_zero_tight() {
    let src = "class C {}\nfunction f() {}";
    let o = FormatOptions {
        blank_lines_after_class: 0,
        ..Default::default()
    };
    let out = format_document(src, Version::V4, &o).unwrap();
    assert!(
        out.contains("}\nfunction"),
        "blank_lines_after_class=0 should not add extra blank lines, got:\n{out}"
    );
}

#[test]
fn blank_lines_after_class_from_directive() {
    let src = "// leekfmt: blank-lines-after-class=0\nclass C {}\nfunction f() {}";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("}\nfunction"),
        "directive should remove extra blank after class, got:\n{out}"
    );
}

#[test]
fn class_fields_grouped_no_blank_between_consecutive() {
    let src = "class C {\nprivate integer a;\nprivate integer b;\nvoid m() {}\n}";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        !out.contains("a;\n\n    private"),
        "consecutive properties should not get an extra blank line, got:\n{out}"
    );
    assert!(
        out.contains("b;\n\n    void m"),
        "expected blank line before method after field group, got:\n{out}"
    );
}

#[test]
fn blank_lines_between_block_statements() {
    let o = FormatOptions {
        blank_lines_between_block_statements: 1,
        ..Default::default()
    };
    let out = format_document("function f() {\nlet a=1;\nlet b=2;\n}", LanguageOptions::v4_experimental_all(), &o).unwrap();
    assert!(
        out.contains("\n\n    let b"),
        "expected one extra blank line between block stmts, got:\n{out}"
    );
}

#[test]
fn max_consecutive_blank_lines_caps_block_extra() {
    let o = FormatOptions {
        blank_lines_between_block_statements: 5,
        max_consecutive_blank_lines_in_block: 1,
        ..Default::default()
    };
    let out = format_document("function f() {\nlet a=1;\nlet b=2;\n}", LanguageOptions::v4_experimental_all(), &o).unwrap();
    assert!(
        !out.contains("\n\n\n\n    let b"),
        "expected cap on extra blank lines, got:\n{out}"
    );
    assert!(
        out.contains("\n\n    let b"),
        "expected at least one blank line between stmts, got:\n{out}"
    );
}

#[test]
fn space_after_comma_from_directive() {
    let src = "// leekfmt: space-after-comma=false\nlet x = f(1, 2);\n";
    let out = format_document(src, LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out.contains("f(1,2)") && !out.contains("1, 2"),
        "directive should disable space after comma, got:\n{out}"
    );
}

#[test]
fn compact_type_union_and_angle_generics() {
    let src = "class T {\nstatic integer | real f() {\nreturn 0;\n}\nstatic Function < any, any => integer | real > g() {}\n}";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("integer|real"),
        "expected compact `|`, got:\n{out}"
    );
    assert!(
        out.contains("Function<any, any => integer|real>"),
        "expected compact `<`/`>` and `|` in function type, got:\n{out}"
    );
}

#[test]
fn comparison_and_bitwise_still_spaced_outside_types() {
    let out = format_document("let x = 1 | 2;", LanguageOptions::v4_experimental_all(), &FormatOptions::default()).unwrap();
    assert!(
        out.contains("1 | 2"),
        "bitwise `|` outside TypeExpr should stay spaced, got:\n{out}"
    );

    let out = format_document("if (a < b) {}", Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("a < b"),
        "comparison `<` outside types should stay spaced, got:\n{out}"
    );
}

#[test]
fn space_around_type_operators_true() {
    let o = FormatOptions {
        space_around_type_operators: true,
        ..Default::default()
    };
    let src = "class T {\nstatic integer|real f() {}\n}";
    let out = format_document(src, Version::V4, &o).unwrap();
    assert!(
        out.contains("integer | real"),
        "expected spaced type union, got:\n{out}"
    );
}

#[test]
fn space_around_type_operators_from_directive() {
    let src =
        "// leekfmt: space-around-type-operators=true\nclass T {\nstatic integer|real f() {}\n}\n";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("integer | real"),
        "directive should enable spaced `|`, got:\n{out}"
    );
}
