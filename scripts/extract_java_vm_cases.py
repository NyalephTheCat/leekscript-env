#!/usr/bin/env python3
"""
Extract Java generator test cases for LeekScript JVM parity (converted to Rust static data).

Pulls chained calls on TestCommon.Case:
  .equals("…"), .ops(n), .almost(x[, delta]), .error(Error.X), .warning(Error.X),
  .noWarning(), .any_error()
plus modifiers .max_ops(n), .max_ram(n), .debug() (debug ignored).

Emits version range and strict from the factory (code_v1_3, code_v4_, code_strict_v4_, file_v2_, …).
LATEST = 4 (LeekScript.LATEST_VERSION).

Skips: DISABLED_* methods, unknown terminals (e.g. .quine()).

Also skips cases the scanner cannot reconstruct: dynamic `equals(...)`, snippets using
non-literal `+` operands (e.g. loop variables), or locals not assigned to constant ints in the
same test method.

Writes (paths relative to repo root):
  crates/leekscript_run/tests/java_vm_suite/cases_generated.rs
  crates/leekscript_run/tests/java_vm_suite/java_vm_export_group_tests.inc.rs

In `java_vm_export_group_tests.inc.rs`, each `*Stress.java` source gets its own top-level module
(e.g. `testarraystress`) with one `#[test]` per Java source line that contains cases (submodules like
`l30`, `l40`, …), each `#[ignore = "…"]` with a `cargo test … {mod}:: -- --ignored` hint, so the
default suite stays fast and failures pinpoint a single Java line.

`TestFiles.java` and `TestEuler.java` are grouped in top-level modules `testfiles` and `testeuler`
(rather than under `io` / `misc`). Each Leek file path gets its own Rust submodule (e.g.
`testeuler::ai_euler_pe001::…`) so you can run one script with a narrow filter.

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
OUT_CASES = REPO / "crates/leekscript_run/tests/java_vm_suite/cases_generated.rs"
OUT_GROUP_TESTS = (
    REPO / "crates/leekscript_run/tests/java_vm_suite/java_vm_export_group_tests.inc.rs"
)

SKIP_FILES = frozenset(
    {
        "TestAI.java",
        "TestCommon.java",
        "SummaryExtension.java",
        "BenchRAM.java",
    }
)

# Ignored by default (slow); message tells how to run with --ignored.
SLOW_IGNORED_FILES: dict[str, str] = {
    "TestFiles.java": (
        "slow file I/O parity suite; run with "
        "`cargo test -p leekscript_run --test java_vm_suite testfiles:: -- --ignored`"
    ),
    "TestJSON.java": (
        "slow file I/O parity suite; run with "
        "`cargo test -p leekscript_run --test java_vm_suite io:: -- --ignored`"
    ),
    "TestSystem.java": (
        "slow file I/O parity suite; run with "
        "`cargo test -p leekscript_run --test java_vm_suite io:: -- --ignored`"
    ),
    "TestEuler.java": (
        "slow euler parity suite; run with "
        "`cargo test -p leekscript_run --test java_vm_suite testeuler:: -- --ignored`"
    ),
}

LATEST_VERSION = 4

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
FACTORY_RE = re.compile(
    r"\b(" + "|".join(re.escape(m) for m in METHOD_ORDER) + r")\s*\("
)

# LeekConstants.TYPE_* → getIntValue() (see leekscript/runner/LeekConstants.java)
# One Rust #[test] group per Leek path (slug from `ai/foo/bar.leek` → `ai_foo_bar`).
SPLIT_JAVA_CASE_GROUPS_BY_LEEK_PATH = frozenset({"TestFiles.java", "TestEuler.java"})

JAVA_CASE_LINE_IN_ID_RE = re.compile(r"^[^:]+\.java:(\d+):")

LEEK_CONSTANTS_GETINT: dict[str, int] = {
    "TYPE_NULL": 0,
    "TYPE_NUMBER": 1,
    "TYPE_BOOLEAN": 2,
    "TYPE_STRING": 3,
    "TYPE_ARRAY": 4,
    "TYPE_FUNCTION": 5,
    "TYPE_CLASS": 6,
    "TYPE_OBJECT": 7,
    "TYPE_MAP": 8,
    "TYPE_SET": 9,
    "TYPE_INTERVAL": 10,
}

FLOAT_RE = re.compile(
    r"^[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?$"
)


def java_index_outside_string_literals(line: str, pos: int) -> bool:
    """True if `pos` is not inside a Java string or char literal (best-effort for test sources)."""
    i = 0
    in_double = False
    in_single = False
    esc = False
    while i < pos:
        c = line[i]
        if in_double:
            if esc:
                esc = False
            elif c == "\\":
                esc = True
            elif c == '"':
                in_double = False
        elif in_single:
            if esc:
                esc = False
            elif c == "\\":
                esc = True
            elif c == "'":
                in_single = False
        else:
            if c == '"':
                in_double = True
            elif c == "'":
                in_single = True
        i += 1
    return not in_double and not in_single


def find_call_end(s: str, open_paren_idx: int) -> int:
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
            return decode_java_escapes(raw), i + 1
        buf.append(c)
        i += 1
    return None


def decode_java_escapes(raw: str) -> str:
    out: list[str] = []
    i = 0
    while i < len(raw):
        c = raw[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        if i + 1 >= len(raw):
            out.append("\\")
            i += 1
            continue
        n = raw[i + 1]
        if n == "n":
            out.append("\n")
            i += 2
        elif n == "r":
            out.append("\r")
            i += 2
        elif n == "t":
            out.append("\t")
            i += 2
        elif n in {'"', "'", "\\"}:
            out.append(n)
            i += 2
        elif n == "u":
            if i + 6 <= len(raw):
                hex4 = raw[i + 2 : i + 6]
                if all(ch in "0123456789abcdefABCDEF" for ch in hex4):
                    out.append(chr(int(hex4, 16)))
                    i += 6
                else:
                    out.append("\\u")
                    i += 2
            else:
                out.append("\\u")
                i += 2
        else:
            out.append(n)
            i += 2
    return "".join(out)


def parse_java_long_literal(inner: str) -> int | None:
    inner = inner.strip().rstrip("lL").replace("_", "")
    if inner.startswith(("0x", "0X")):
        try:
            return int(inner, 16)
        except ValueError:
            return None
    if inner.startswith("-") and inner[1:].isdigit():
        return int(inner)
    if inner.isdigit():
        return int(inner)
    return None


def parse_long_token(tok: str, locals_: dict[str, int]) -> int | None:
    tok = tok.strip()
    if not tok:
        return None
    ident = tok.rstrip("lL")
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", ident):
        if ident in locals_:
            return locals_[ident]
        return None
    return parse_java_long_literal(tok)


def eval_java_assign_rhs(expr: str, locals_: dict[str, int]) -> int | None:
    expr = expr.strip()
    if "*" in expr:
        acc = 1
        for part in expr.split("*"):
            v = parse_long_token(part, locals_)
            if v is None:
                return None
            acc *= v
        return acc
    return parse_long_token(expr, locals_)


def update_method_locals(line: str, locals_: dict[str, int]) -> None:
    line = line.split("//")[0]
    for m in re.finditer(
        r"\b(?:final\s+)?(?:long|int)\s+(\w+)\s*=\s*([^;]+);",
        line,
    ):
        name, rhs = m.group(1), m.group(2)
        v = eval_java_assign_rhs(rhs, locals_)
        if v is not None:
            locals_[name] = v


def parse_java_concat_snippet(expr: str, locals_: dict[str, int]) -> str | None:
    i = 0
    out: list[str] = []
    n = len(expr)
    while i < n:
        while i < n and expr[i].isspace():
            i += 1
        if i >= n:
            break
        if expr[i] == '"':
            r = read_java_double_quoted_string(expr, i)
            if r is None:
                return None
            out.append(r[0])
            i = r[1]
        else:
            j = i
            while j < n and expr[j] != "+":
                j += 1
            tok = expr[i:j].strip()
            if not tok:
                return None
            v = parse_long_token(tok, locals_)
            if v is None:
                return None
            out.append(str(v))
            i = j
        while i < n and expr[i].isspace():
            i += 1
        if i >= n:
            break
        if expr[i] == "+":
            i += 1
            continue
        return None
    return "".join(out)


def parse_factory_first_arg_snippet(arg: str, locals_: dict[str, int]) -> str | None:
    arg = arg.strip()
    if "+" not in arg:
        r = read_java_double_quoted_string(arg, 0)
        if r is None:
            return None
        decoded, after = r
        if after < len(arg) and arg[after:].strip():
            return None
        return decoded
    return parse_java_concat_snippet(arg, locals_)


def parse_equals_arg(inner: str, locals_: dict[str, int]) -> str | None:
    inner = inner.strip()
    r = read_java_double_quoted_string(inner, 0)
    if r is not None:
        decoded, after = r
        if after >= len(inner) or not inner[after:].strip():
            return decoded

    m = re.fullmatch(
        r"String\.valueOf\s*\(\s*LeekConstants\.(\w+)\.getIntValue\s*\(\s*\)\s*\)",
        inner,
    )
    if m:
        k = m.group(1)
        if k in LEEK_CONSTANTS_GETINT:
            return str(LEEK_CONSTANTS_GETINT[k])
        return None

    m = re.fullmatch(
        r"String\.valueOf\s*\(\s*(-?(?:0[xX][0-9a-fA-F_]+|\d[\d_]*))[lL]?\s*\)",
        inner,
    )
    if m:
        v = parse_java_long_literal(m.group(1))
        if v is None:
            return None
        return str(v)

    return None


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


RUST_IDENT_RE = re.compile(r"[^a-zA-Z0-9_]+")

RUST_KEYWORDS = {
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
    "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn",
    "abstract", "become", "box", "do", "final", "macro", "override", "priv", "try",
    "typeof", "unsized", "virtual", "yield",
}


def rust_ident(s: str) -> str:
    s = s.strip()
    s = s.replace("-", "_").replace(".", "_").replace(" ", "_")
    s = RUST_IDENT_RE.sub("_", s)
    s = re.sub(r"_+", "_", s)
    s = s.strip("_")
    if not s:
        return "unknown"
    if s[0].isdigit():
        s = "_" + s
    return s.lower()


def rust_safe_ident(s: str) -> str:
    if s in RUST_KEYWORDS:
        return "r#" + s
    return s


def java_file_to_rust_ident(java_file: str) -> tuple[str, str]:
    stem = java_file.removesuffix(".java")
    snake = pascal_to_snake(stem)
    return snake.upper(), snake


def stress_ignore_message(java_file: str) -> str:
    mod = rust_ident(java_file.removesuffix(".java"))
    return (
        f"Java VM parity stress — {java_file}; run with "
        f"`cargo test -p leekscript_run --test java_vm_suite {mod}:: -- --ignored`"
    )


def java_file_category(java_file: str) -> str:
    if java_file == "TestFiles.java":
        return "testfiles"
    if java_file == "TestEuler.java":
        return "testeuler"
    if java_file.endswith("Stress.java"):
        return rust_ident(java_file.removesuffix(".java"))
    stem = java_file.removesuffix(".java")
    collections = {"Array", "Map", "Set", "Object"}
    control_flow = {"If", "Loops", "Switch", "Interval"}
    operators = {"Operators", "Operations"}
    primitives = {"Boolean", "Number", "String"}
    io = {"JSON", "System"}
    language = {"Function", "Class", "Globals", "Reference", "Narrowing", "Comments"}
    misc = {"General", "EdgeCases"}
    if stem.startswith("Test"):
        name = stem[len("Test") :]
    else:
        name = stem
    if name in collections:
        return "collections"
    if name in control_flow:
        return "control_flow"
    if name in operators:
        return "operators"
    if name in primitives:
        return "primitives"
    if name in io:
        return "io"
    if name in language:
        return "language"
    if name in misc:
        return "misc"
    return "other"


def parse_chain(
    rest_after_code_string: str, locals_: dict[str, int]
) -> tuple[dict[str, int], str, object] | None:
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
            v = parse_long_token(args, locals_)
            if v is None:
                return None
            modifiers["max_ops"] = v
            continue
        if name == "max_ram":
            v = parse_long_token(args, locals_)
            if v is None:
                return None
            modifiers["max_ram"] = v
            continue

        if name == "equals":
            exp = parse_equals_arg(args, locals_)
            if exp is None:
                return None
            return modifiers, "equals", exp
        if name == "ops":
            v = parse_long_token(args, locals_)
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


def extract_line_cases(
    line: str,
    locals_: dict[str, int],
) -> list[tuple[str, str, str, object, dict[str, int], int, int, bool]]:
    if "//" in line:
        line = line[: line.index("//")]
    out: list[
        tuple[str, str, str, object, dict[str, int], int, int, bool]
    ] = []
    for m in FACTORY_RE.finditer(line):
        if not java_index_outside_string_literals(line, m.start()):
            continue
        method = m.group(1)
        if method.startswith("DISABLED"):
            continue
        spec = METHOD_SPECS.get(method)
        if spec is None:
            continue
        vmin, vmax, strict = spec
        open_paren = m.end() - 1
        close_paren = find_call_end(line, open_paren)
        if close_paren < 0:
            continue
        first_arg = line[open_paren + 1 : close_paren]
        code = parse_factory_first_arg_snippet(first_arg, locals_)
        if code is None:
            continue
        parsed = parse_chain(line[close_paren:], locals_)
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


def leek_path_to_subgroup_slug(leek_path: str) -> str:
    base = leek_path.strip().removesuffix(".leek").strip()
    slug = base.replace("/", "_").replace("-", "_")
    return rust_ident(slug)


def stress_line_slug_from_case_id(cid: str) -> str:
    """Subgroup for *Stress.java: one submodule per Java line (`l42`, …)."""
    m = JAVA_CASE_LINE_IN_ID_RE.match(cid)
    if not m:
        return "unknown_line"
    return f"l{int(m.group(1))}"


def stress_line_slug_sort_key(slug: str) -> tuple[int, str]:
    if slug.startswith("l"):
        rest = slug[1:]
        if rest.isdigit():
            return (int(rest), slug)
    return (10**9, slug)


def rust_test_fn_name(java_file_stem: str, section: str) -> str:
    """Stable `fn` name inside a submodule (section may be `run__ai_euler_pe001`)."""
    file_mod = rust_ident(java_file_stem)
    sec_ident = rust_ident(section)
    prefix = file_mod + "_"
    short = sec_ident[len(prefix) :] if sec_ident.startswith(prefix) else sec_ident
    return rust_safe_ident(short or sec_ident)


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

    by_section: dict[
        tuple[str, str],
        list[tuple[str, str, str, object, dict[str, int], int, int, bool]],
    ] = defaultdict(list)

    file_to_sections: dict[str, list[str]] = defaultdict(list)

    for path in sorted(JAVA_DIR.glob("Test*.java")):
        if path.name in SKIP_FILES:
            continue
        java_file = path.name
        current_java_test_method: str | None = None
        method_locals: dict[str, int] = {}
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            mm = re.search(r"\bpublic\s+void\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(", line)
            if mm:
                current_java_test_method = mm.group(1)
                method_locals = {}
                if current_java_test_method not in file_to_sections[java_file]:
                    file_to_sections[java_file].append(current_java_test_method)

            update_method_locals(line, method_locals)

            for method, code, term, payload, mods, vmin, vmax, strict in extract_line_cases(
                line, method_locals
            ):
                cid = f"{java_file}:{lineno}:{method}.{term}"
                section = current_java_test_method or "unknown_section"
                if section not in file_to_sections[java_file]:
                    file_to_sections[java_file].append(section)
                by_section[(java_file, section)].append(
                    (cid, method, code, term, payload, mods, vmin, vmax, strict)
                )

    sorted_files = sorted(file_to_sections.keys())
    total = sum(
        len(by_section[(jf, sec)])
        for jf in sorted_files
        for sec in file_to_sections[jf]
    )

    out_cases: list[str] = []
    out_cases.append("// @generated by scripts/extract_java_vm_cases.py — do not edit by hand")
    out_cases.append("")
    out_cases.append("#![allow(clippy::large_stack_arrays, dead_code)]")
    out_cases.append("")
    out_cases.append("#[derive(Clone, Copy, Debug, PartialEq, Eq)]")
    out_cases.append("pub enum SourceKind {")
    out_cases.append("    Snippet,")
    out_cases.append("    File,")
    out_cases.append("}")
    out_cases.append("")
    out_cases.append("#[derive(Clone, Copy, Debug)]")
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
    out_cases.append("#[derive(Clone, Copy, Debug)]")
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

    group_rows: list[tuple[str, str, str, str]] = []

    def emit_one_group(
        java_file: str,
        section: str,
        rows: list[
            tuple[str, str, str, object, dict[str, int], int, int, bool]
        ],
    ) -> None:
        static_suffix, _file_snake = java_file_to_rust_ident(java_file)
        section_ident = rust_ident(section)
        group_ident = f"{rust_ident(java_file.removesuffix('.java'))}__{section_ident}"
        static_name = f"VM_JAVA_CASES_{static_suffix}__{section_ident.upper()}"

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

        group_rows.append((java_file, section, group_ident, static_name))

    for java_file in sorted_files:
        for section in file_to_sections[java_file]:
            rows = by_section.get((java_file, section), [])
            if not rows:
                continue

            if java_file in SPLIT_JAVA_CASE_GROUPS_BY_LEEK_PATH:
                buckets: dict[str, list] = defaultdict(list)
                for row in rows:
                    _cid, method, code, *_rest = row
                    if method.startswith("file"):
                        key = leek_path_to_subgroup_slug(code)
                    else:
                        key = rust_ident("non_file")
                    buckets[key].append(row)
                for slug in sorted(buckets.keys()):
                    syn_section = f"{section}__{slug}"
                    emit_one_group(java_file, syn_section, buckets[slug])
            elif java_file.endswith("Stress.java"):
                buckets: dict[str, list] = defaultdict(list)
                for row in rows:
                    cid = row[0]
                    key = rust_ident(stress_line_slug_from_case_id(cid))
                    buckets[key].append(row)
                for slug in sorted(buckets.keys(), key=stress_line_slug_sort_key):
                    syn_section = f"{section}__{slug}"
                    emit_one_group(java_file, syn_section, buckets[slug])
            else:
                emit_one_group(java_file, section, rows)

    out_cases.append(f"pub const VM_JAVA_SUITE_TOTAL_CASES: usize = {total};")
    out_cases.append("")
    out_cases.append("/// `(java_filename, java_test_method, group_ident, cases)`.")
    out_cases.append(
        "pub static VM_JAVA_GROUPS: &[(&'static str, &'static str, &'static str, &'static [JavaVmCase])] = &["
    )
    for java_file, section, group_ident, static_name in group_rows:
        jf = rust_string_literal(java_file)
        sec = rust_string_literal(section)
        gi = rust_string_literal(group_ident)
        out_cases.append(f"    ({jf}, {sec}, {gi}, {static_name}),")
    out_cases.append("];")
    out_cases.append("")

    out_inc: list[str] = []
    out_inc.append("// @generated by scripts/extract_java_vm_cases.py — do not edit by hand")
    out_inc.append("")

    by_cat: dict[str, dict[str, list[tuple[str, str]]]] = defaultdict(
        lambda: defaultdict(list)
    )
    for java_file, section, _group_ident, static_name in group_rows:
        by_cat[java_file_category(java_file)][java_file].append((section, static_name))

    for cat in sorted(by_cat.keys()):
        out_inc.append(f"mod {rust_safe_ident(cat)} {{")
        for java_file in sorted(by_cat[cat].keys()):
            file_mod = rust_ident(java_file.removesuffix(".java"))
            java_stem = java_file.removesuffix(".java")
            split_by_path = java_file in SPLIT_JAVA_CASE_GROUPS_BY_LEEK_PATH or java_file.endswith(
                "Stress.java"
            )
            flatten_inner = file_mod == cat and not split_by_path
            inner_pad = "    " if flatten_inner else "        "
            if not flatten_inner and not split_by_path:
                out_inc.append(f"    mod {rust_safe_ident(file_mod)} {{")
            is_stress = java_file.endswith("Stress.java")
            slow_msg = SLOW_IGNORED_FILES.get(java_file)
            for section, static_name in by_cat[cat][java_file]:
                if split_by_path:
                    sub_mod = section.split("__", 1)[1] if "__" in section else section
                    out_inc.append(f"    mod {rust_safe_ident(sub_mod)} {{")
                    pad = "        "
                    fn_name = rust_test_fn_name(java_stem, section)
                else:
                    pad = inner_pad
                    fn_name = rust_test_fn_name(java_stem, section)
                if slow_msg is not None:
                    out_inc.append(f'{pad}#[ignore = "{slow_msg}"]')
                elif is_stress:
                    out_inc.append(
                        f'{pad}#[ignore = "{stress_ignore_message(java_file)}"]'
                    )
                out_inc.append(f"{pad}#[test]")
                out_inc.append(f"{pad}fn {fn_name}() {{")
                out_inc.append(
                    f"{pad}    crate::runner::run_cases(crate::cases_generated::{static_name});"
                )
                out_inc.append(f"{pad}}}")
                out_inc.append("")
                if split_by_path:
                    out_inc.append("    }")
                    out_inc.append("")
            if not flatten_inner and not split_by_path:
                out_inc.append("    }")
                out_inc.append("")
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
