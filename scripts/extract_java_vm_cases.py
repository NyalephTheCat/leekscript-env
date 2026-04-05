#!/usr/bin/env python3
"""
Extract Java generator test cases for leekscript-rs VM parity.

Pulls chained calls on TestCommon.Case:
  .equals("…"), .ops(n), .almost(x[, delta]), .error(Error.X), .warning(Error.X),
  .noWarning(), .any_error()
plus modifiers .max_ops(n), .max_ram(n), .debug() (debug ignored).

Also emits **version range** and **strict** flag from the factory method (code_v1_3, code_v4_,
code_strict_v4_, file_v2_, …). LATEST is taken as 4 (LeekScript.LATEST_VERSION in Java).

Skips: DISABLED_* methods, lines whose terminal is unknown (e.g. .quine()), max_ops/max_ram
arguments that are not numeric literals (e.g. fight_max_ops).

Writes:
  crates/leekscript-rs/leekscript/tests/vm_java_suite/cases_generated.rs
  crates/leekscript-rs/leekscript/tests/vm_java_suite/java_vm_export_group_tests.inc.rs

Run from repo root:

    python3 scripts/extract_java_vm_cases.py
"""

from __future__ import annotations

import re
import sys
import warnings
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
JAVA_DIR = REPO / "leek-wars-generator/leekscript/src/test/java/test"
OUT_CASES = (
    REPO
    / "crates/leekscript-rs/leekscript/tests/vm_java_suite/cases_generated.rs"
)
OUT_GROUP_TESTS = (
    REPO
    / "crates/leekscript-rs/leekscript/tests/vm_java_suite/java_vm_export_group_tests.inc.rs"
)

SKIP_FILES = frozenset(
    {
        "TestAI.java",
        "TestCommon.java",
        "SummaryExtension.java",
        "BenchRAM.java",
    }
)

# Must match leekscript/compiler/LeekScript.java
LATEST_VERSION = 4

# (version_min, version_max, strict)
METHOD_SPECS: dict[str, tuple[int, int, bool]] = {
    "code": (1, LATEST_VERSION, False),
    "code_strict": (1, LATEST_VERSION, True),
    "file": (1, LATEST_VERSION, False),
    "file_v1": (1, 1, False),
    "file_v2_": (2, LATEST_VERSION, False),
    "file_v3": (3, 3, False),
    "file_v4_": (4, LATEST_VERSION, False),
    "code_v1": (1, 1, False),
    "code_strict_v1": (1, 1, True),
    "code_v1_2": (1, 2, False),
    "code_v1_3": (1, 3, False),
    "code_v1_4": (1, 4, False),
    "code_v2": (2, 2, False),
    "code_v2_": (2, LATEST_VERSION, False),
    "code_strict_v2_": (2, LATEST_VERSION, True),
    "code_v2_3": (2, 3, False),
    "code_v2_4": (2, 4, False),
    "code_v3": (3, 3, False),
    "code_v3_": (3, LATEST_VERSION, False),
    "code_v4": (4, 4, False),
    "code_v4_": (4, LATEST_VERSION, False),
    "code_strict_v4_": (4, LATEST_VERSION, True),
}

METHOD_ORDER = sorted(METHOD_SPECS.keys(), key=len, reverse=True)
METHOD_RE = re.compile(
    "(" + "|".join(re.escape(m) for m in METHOD_ORDER) + r")\s*\(\s*\""
)

FLOAT_RE = re.compile(
    r"^[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?$"
)


def find_call_end(s: str, open_paren_idx: int) -> int:
    """Index of closing `)` matching `(` at open_paren_idx; string/char aware."""
    i = open_paren_idx + 1
    depth = 1
    in_str = False
    str_quote: str | None = None
    esc = False
    while i < len(s) and depth > 0:
        c = s[i]
        if in_str:
            if esc:
                esc = False
            elif c == "\\":
                esc = True
            elif str_quote is not None and c == str_quote:
                in_str = False
        else:
            if c in "\"'":
                in_str = True
                str_quote = c
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return -1


def read_java_double_quoted_string(line: str, start: int) -> tuple[str, int] | None:
    """start points at opening `"`. Returns (decoded, index_after_closing_quote)."""
    if start >= len(line) or line[start] != '"':
        return None
    i = start + 1
    buf: list[str] = []
    while i < len(line):
        c = line[i]
        if c == "\\" and i + 1 < len(line):
            buf.append(line[i : i + 2])
            i += 2
            continue
        if c == '"':
            raw = "".join(buf)
            try:
                decoded = bytes(raw, "utf-8").decode("unicode_escape")
            except UnicodeDecodeError:
                decoded = raw
            return decoded, i + 1
        buf.append(c)
        i += 1
    return None


def parse_equals_expected(inner: str) -> str | None:
    inner = inner.strip()
    r = read_java_double_quoted_string(inner, 0)
    if r is None:
        return None
    decoded, after = r
    if after < len(inner) and inner[after:].strip():
        return None
    return decoded


def parse_java_long_literal(inner: str) -> int | None:
    inner = inner.strip().rstrip("lL")
    if not inner.isdigit():
        return None
    return int(inner)


def parse_almost_args(inner: str) -> tuple[float, float] | None:
    inner = inner.strip()
    if "," in inner:
        a, b = inner.split(",", 1)
        a, b = a.strip(), b.strip()
        try:
            return float(a), float(b)
        except ValueError:
            return None
    try:
        return float(inner), 1e-10
    except ValueError:
        return None


def rust_string_literal(s: str) -> str:
    out: list[str] = ['"']
    for ch in s:
        o = ord(ch)
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif o < 32 or o == 0x7F:
            out.append(f"\\u{{{o:04x}}}")
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def pascal_to_snake(stem: str) -> str:
    s1 = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", stem)
    s2 = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s1)
    return s2.lower()


def java_file_to_rust_ident(java_file: str) -> tuple[str, str]:
    stem = java_file.removesuffix(".java")
    snake = pascal_to_snake(stem)
    return snake.upper(), snake


def parse_chain(rest_after_code_string: str) -> tuple[dict[str, int], str, object] | None:
    """
    rest_after_code_string begins right after the closing `"` of the code/file path argument.
    Typical: `).debug().equals("x");`
    Returns (modifiers, terminal_name, terminal_payload) or None.
    """
    m = re.match(r"\s*\)", rest_after_code_string)
    if not m:
        return None
    pos = m.end()
    modifiers: dict[str, int] = {}

    while True:
        m2 = re.match(r"\s*\.\s*(\w+)\s*\(", rest_after_code_string[pos:])
        if not m2:
            return None
        name = m2.group(1)
        open_paren = pos + m2.end() - 1
        close_paren = find_call_end(rest_after_code_string, open_paren)
        if close_paren < 0:
            return None
        args = rest_after_code_string[open_paren + 1 : close_paren]
        pos = close_paren + 1

        if name == "debug":
            continue
        if name == "max_ops":
            v = parse_java_long_literal(args)
            if v is None:
                return None
            modifiers["max_ops"] = v
            continue
        if name == "max_ram":
            v = parse_java_long_literal(args)
            if v is None:
                return None
            modifiers["max_ram"] = v
            continue

        if name == "equals":
            exp = parse_equals_expected(args)
            if exp is None:
                return None
            return modifiers, "equals", exp
        if name == "ops":
            v = parse_java_long_literal(args)
            if v is None:
                return None
            return modifiers, "ops", v
        if name == "almost":
            ap = parse_almost_args(args)
            if ap is None:
                return None
            return modifiers, "almost", ap
        if name == "error":
            em = re.search(r"Error\.(\w+)", args)
            if not em:
                return None
            return modifiers, "error", em.group(1)
        if name == "warning":
            em = re.search(r"Error\.(\w+)", args)
            if not em:
                return None
            return modifiers, "warning", em.group(1)
        if name == "noWarning":
            if args.strip():
                return None
            return modifiers, "noWarning", None
        if name == "any_error":
            if args.strip():
                return None
            return modifiers, "any_error", None

        return None


def extract_line_cases(line: str) -> list[tuple[str, str, str, object, dict[str, int], int, int, bool]]:
    """
    Each tuple:
      (method, code_or_path, cid_suffix_terminal, terminal_payload, modifiers, vmin, vmax, strict)
    cid is built by caller with java_file:lineno.
    """
    if "//" in line:
        line = line[: line.index("//")]
    out: list[
        tuple[str, str, str, object, dict[str, int], int, int, bool]
    ] = []
    for m in METHOD_RE.finditer(line):
        method = m.group(1)
        if method.startswith("DISABLED"):
            continue
        spec = METHOD_SPECS.get(method)
        if spec is None:
            continue
        vmin, vmax, strict = spec
        i = m.end()
        read = read_java_double_quoted_string(line, i - 1)
        if read is None:
            continue
        code, after_quote = read
        try:
            code = bytes(code, "utf-8").decode("unicode_escape")
        except UnicodeDecodeError:
            pass
        parsed = parse_chain(line[after_quote:])
        if parsed is None:
            continue
        modifiers, term, payload = parsed
        out.append((method, code, term, payload, modifiers, vmin, vmax, strict))
    return out


def emit_expect_rust(term: str, payload: object) -> str:
    if term == "equals":
        assert isinstance(payload, str)
        e = rust_string_literal(payload)
        return f"ExpectKind::ExportEqual {{ expected_export: {e} }}"
    if term == "ops":
        assert isinstance(payload, int)
        return f"ExpectKind::OpsOnly {{ expected_ops: {payload} }}"
    if term == "almost":
        assert isinstance(payload, tuple)
        v, d = payload
        return f"ExpectKind::Almost {{ value: {v}, delta: {d} }}"
    if term == "error":
        assert isinstance(payload, str)
        n = rust_string_literal(payload)
        return f"ExpectKind::JavaError {{ name: {n} }}"
    if term == "warning":
        assert isinstance(payload, str)
        n = rust_string_literal(payload)
        return f"ExpectKind::JavaWarning {{ name: {n} }}"
    if term == "noWarning":
        return "ExpectKind::NoWarning"
    if term == "any_error":
        return "ExpectKind::AnyError"
    raise ValueError(term)


def emit_case_row(
    cid: str,
    method: str,
    source: str,
    expect_rust: str,
    vmin: int,
    vmax: int,
    strict: bool,
    max_ops: str,
    max_ram: str,
) -> str:
    is_file = method.startswith("file")
    kind = "SourceKind::File" if is_file else "SourceKind::Snippet"
    src_lit = rust_string_literal(source)
    cid_lit = rust_string_literal(cid)
    st = "true" if strict else "false"
    return (
        f"    JavaVmCase {{ id: {cid_lit}, kind: {kind}, source: {src_lit}, expect: {expect_rust}, "
        f"version_min: {vmin}, version_max: {vmax}, strict: {st}, "
        f"max_ops_limit: {max_ops}, max_ram_quads_limit: {max_ram} }},"
    )


def main() -> None:
    warnings.simplefilter("ignore", DeprecationWarning)
    if not JAVA_DIR.is_dir():
        print(f"error: missing {JAVA_DIR}", file=sys.stderr)
        sys.exit(1)

    by_file: dict[str, list[tuple[str, str, str, object, dict[str, int], int, int, bool]]] = (
        defaultdict(list)
    )

    for path in sorted(JAVA_DIR.glob("Test*.java")):
        if path.name in SKIP_FILES:
            continue
        java_file = path.name
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for method, code, term, payload, mods, vmin, vmax, strict in extract_line_cases(
                line
            ):
                cid = f"{java_file}:{lineno}:{method}.{term}"
                by_file[java_file].append(
                    (cid, method, code, term, payload, mods, vmin, vmax, strict)
                )

    sorted_files = sorted(by_file.keys())
    total = sum(len(by_file[f]) for f in sorted_files)

    out_cases: list[str] = []
    out_cases.append("// @generated by scripts/extract_java_vm_cases.py — do not edit by hand")
    out_cases.append("")
    out_cases.append("#![allow(clippy::large_stack_arrays, dead_code)]")
    out_cases.append("")
    out_cases.append("pub enum SourceKind {")
    out_cases.append("    Snippet,")
    out_cases.append("    File,")
    out_cases.append("}")
    out_cases.append("")
    out_cases.append("pub enum ExpectKind {")
    out_cases.append("    ExportEqual { expected_export: &'static str },")
    out_cases.append("    OpsOnly { expected_ops: u64 },")
    out_cases.append("    Almost { value: f64, delta: f64 },")
    out_cases.append("    JavaError { name: &'static str },")
    out_cases.append("    JavaWarning { name: &'static str },")
    out_cases.append("    NoWarning,")
    out_cases.append("    AnyError,")
    out_cases.append("}")
    out_cases.append("")
    out_cases.append("pub struct JavaVmCase {")
    out_cases.append("    pub id: &'static str,")
    out_cases.append("    pub kind: SourceKind,")
    out_cases.append("    pub source: &'static str,")
    out_cases.append("    pub expect: ExpectKind,")
    out_cases.append("    pub version_min: u8,")
    out_cases.append("    pub version_max: u8,")
    out_cases.append("    pub strict: bool,")
    out_cases.append("    pub max_ops_limit: Option<u64>,")
    out_cases.append("    pub max_ram_quads_limit: Option<u64>,")
    out_cases.append("}")
    out_cases.append("")

    group_rows: list[tuple[str, str, str]] = []

    for java_file in sorted_files:
        rows = by_file[java_file]
        static_suffix, group_snake = java_file_to_rust_ident(java_file)
        static_name = f"VM_JAVA_CASES_{static_suffix}"
        out_cases.append(f"pub static {static_name}: &[JavaVmCase] = &[")

        for cid, method, code, term, payload, mods, vmin, vmax, strict in rows:
            mo = mods.get("max_ops")
            mr = mods.get("max_ram")
            max_ops_rust = f"Some({mo})" if mo is not None else "None"
            max_ram_rust = f"Some({mr})" if mr is not None else "None"
            expect_rust = emit_expect_rust(term, payload)
            out_cases.append(
                emit_case_row(
                    cid,
                    method,
                    code,
                    expect_rust,
                    vmin,
                    vmax,
                    strict,
                    max_ops_rust,
                    max_ram_rust,
                )
            )

        out_cases.append("];")
        out_cases.append("")
        group_rows.append((java_file, group_snake, static_name))

    out_cases.append(f"pub const VM_JAVA_SUITE_TOTAL_CASES: usize = {total};")
    out_cases.append("")
    out_cases.append(
        "/// `(java_filename, group_snake, cases)` — filter tests: `cargo test java_vm_export_<snake>`."
    )
    out_cases.append(
        "pub static VM_JAVA_GROUPS: &[(&'static str, &'static str, &'static [JavaVmCase])] = &["
    )
    for java_file, group_snake, static_name in group_rows:
        jf = rust_string_literal(java_file)
        gs = rust_string_literal(group_snake)
        out_cases.append(f"    ({jf}, {gs}, {static_name}),")
    out_cases.append("];")
    out_cases.append("")

    out_inc: list[str] = []
    out_inc.append(
        "// @generated by scripts/extract_java_vm_cases.py — do not edit by hand"
    )
    out_inc.append("")
    for java_file, group_snake, static_name in group_rows:
        fn_name = f"java_vm_export_{group_snake}"
        out_inc.append("#[test]")
        out_inc.append(
            f'#[ignore = "Java VM parity — {java_file} — run with --ignored"]'
        )
        out_inc.append(f"fn {fn_name}() {{")
        out_inc.append(f"    run_cases(cases_generated::{static_name});")
        out_inc.append("}")
        out_inc.append("")

    OUT_CASES.parent.mkdir(parents=True, exist_ok=True)
    OUT_CASES.write_text("\n".join(out_cases) + "\n", encoding="utf-8")
    OUT_GROUP_TESTS.write_text("\n".join(out_inc) + "\n", encoding="utf-8")
    print(
        f"wrote {OUT_CASES.relative_to(REPO)} ({total} cases, {len(group_rows)} groups) and "
        f"{OUT_GROUP_TESTS.relative_to(REPO)}",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
