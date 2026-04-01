use leekscript::format::{FormatOptions, format_document};
use leekscript::Version;

#[test]
fn formats_let_with_spaces() {
    let out = format_document("let x=1;", Version::V4, &FormatOptions::default()).unwrap();
    assert_eq!(out.trim(), "let x = 1;");
}

#[test]
fn indent_width_from_directive() {
    let src = "// leekfmt: indent-width=2\nfunction f() {\nlet x=1;\n}";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(out.contains("  let x = 1;"), "expected 2-space indent, got:\n{out}");
}

#[test]
fn off_on_preserves_mangled() {
    let src = "a;\n// leekfmt: off\nlet x=1+  2;\n// leekfmt: on\nb;\n";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(out.contains("let x=1+  2;"), "verbatim region should stay:\n{out}");
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
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(out.contains("let   y=2;"), "next line kept:\n{out}");
    assert!(out.contains("let z = 3;") || out.contains("let z=3"), "following line formatted:\n{out}");
}

#[test]
fn block_comment_directive() {
    let src = "/* leekfmt: use-tabs=true */\nfunction f() {\nlet a=1;\n}\n";
    let out = format_document(src, Version::V4, &FormatOptions::default()).unwrap();
    assert!(
        out.contains("\tlet a = 1;"),
        "expected tab-indented body:\n{out}"
    );
}
