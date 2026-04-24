#!/usr/bin/env python3
"""Emit `data/bench_corpus/generated/*.leek` and `data/bench_corpus/java_vm/**/*.leek`.

Synthetic programs go under `generated/` (same as before). Java parity snippets and resource
files are derived from `crates/leekscript_run/tests/java_vm_suite/cases_generated.rs` (regenerate
that file with `python3 scripts/extract_java_vm_cases.py` when Java tests change).

Run `leekscript-bench` on the Java slice with `--respect-preamble` so `// leek-version` matches
each JVM test row.

Idempotent: clears `generated/*.leek` and the entire `java_vm/` tree, then rewrites.
"""

from __future__ import annotations

import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "data" / "bench_corpus" / "generated"
JAVA_VM_OUT = ROOT / "data" / "bench_corpus" / "java_vm"
CASES_RS = ROOT / "crates" / "leekscript_run" / "tests" / "java_vm_suite" / "cases_generated.rs"
JAVA_TEST_RESOURCES = ROOT / "leek-wars-generator" / "leekscript" / "src" / "test" / "resources"

JAVA_CASE_LINE_RE = re.compile(
    r"^    JavaVmCase \{ id: \"([^\"]+)\", kind: SourceKind::(Snippet|File), source: \"((?:[^\"\\]|\\.)*)\", expect: ExpectKind::ExportEqual \{ expected_export: \"((?:[^\"\\]|\\.)*)\" \}, version_min: (\d+), version_max: (\d+), strict: (true|false),"
)


def write(idx: int, stem: str, src: str) -> int:
    path = OUT / f"{idx:04d}_{stem}.leek"
    path.write_text(src.rstrip() + "\n", encoding="utf-8")
    return idx + 1


def unescape_rust_string_lit(inner: str) -> str:
    """Decode a Rust string literal body (e.g. cases_generated.rs `source: \"...\"`)."""
    out: list[str] = []
    i = 0
    while i < len(inner):
        if inner[i] == "\\" and i + 1 < len(inner):
            n = inner[i + 1]
            if n == "\\":
                out.append("\\")
            elif n == '"':
                out.append('"')
            elif n == "n":
                out.append("\n")
            elif n == "r":
                out.append("\r")
            elif n == "t":
                out.append("\t")
            elif n == "0":
                out.append("\0")
            elif n == "x" and i + 3 < len(inner):
                hexv = inner[i + 2 : i + 4]
                try:
                    out.append(chr(int(hexv, 16)))
                    i += 4
                    continue
                except ValueError:
                    pass
            else:
                out.append(n)
            i += 2
            continue
        out.append(inner[i])
        i += 1
    return "".join(out)


def effective_leek_version(version_min: int, version_max: int) -> int:
    """Pick a version in [min,max] suitable for the reference JVM (LS 1–4 in current suite)."""
    hi = min(version_max, 4)
    return max(version_min, hi)


def preamble_lines(version: int, strict: bool) -> str:
    return f"// leek-version: {version}\n// leek-strict: {'true' if strict else 'false'}\n"


def slug_from_java_id(case_id: str) -> str:
    return re.sub(r"[^0-9A-Za-z._-]+", "_", case_id)


def emit_java_vm_corpus() -> tuple[int, int, int]:
    """Returns (snippet_files, resource_files_written, skipped_missing_resource)."""
    if JAVA_VM_OUT.exists():
        shutil.rmtree(JAVA_VM_OUT)
    JAVA_VM_OUT.mkdir(parents=True, exist_ok=True)

    if not CASES_RS.is_file():
        print(f"skip java_vm: missing {CASES_RS.relative_to(ROOT)}")
        return (0, 0, 0)

    text = CASES_RS.read_text(encoding="utf-8")
    snippet_n = 0
    file_n = 0
    skipped_res = 0

    for line in text.splitlines():
        if "JavaVmCase {" not in line or "ExpectKind::ExportEqual" not in line:
            continue
        m = JAVA_CASE_LINE_RE.match(line)
        if not m:
            continue
        case_id, kind, source_lit, _exp_lit, vmin_s, vmax_s, strict_s = m.groups()
        vmin, vmax = int(vmin_s), int(vmax_s)
        strict = strict_s == "true"
        v_eff = effective_leek_version(vmin, vmax)
        pre = preamble_lines(v_eff, strict)
        body = unescape_rust_string_lit(source_lit)

        if kind == "Snippet":
            slug = slug_from_java_id(case_id)
            path = JAVA_VM_OUT / f"snippet__{slug}.leek"
            path.write_text(pre + body.rstrip() + "\n", encoding="utf-8")
            snippet_n += 1
        else:
            rel = Path(unescape_rust_string_lit(source_lit))
            src_path = JAVA_TEST_RESOURCES / rel
            dest = JAVA_VM_OUT / rel
            if not src_path.is_file():
                skipped_res += 1
                continue
            dest.parent.mkdir(parents=True, exist_ok=True)
            raw = src_path.read_text(encoding="utf-8")
            dest.write_text(pre + raw.lstrip("\ufeff"), encoding="utf-8")
            file_n += 1

    return (snippet_n, file_n, skipped_res)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for p in OUT.glob("*.leek"):
        p.unlink()

    n = 1

    # --- Pure arithmetic & literals (60) ---
    for i in range(60):
        a, b, c = i, (i * 3 + 7) % 97, (i * 11 + 13) % 50
        n = write(n, f"arith_{i:02d}", f"return {a} * {b} - {c} + {i % 17};")

    # --- Ternary & boolean (40) ---
    for i in range(40):
        n = write(
            n,
            f"ternary_{i:02d}",
            f"""var x = {i};
return x % 3 == 0 ? x * 2 : (x % 3 == 1 ? x + 10 : x * x + 1);""",
        )

    # --- Triangular sums via C-style for (40) ---
    for m in range(3, 43):
        n = write(
            n,
            f"tri_{m:02d}",
            f"""var s = 0;
for (var k = 1; k <= {m}; ++k) {{
	s += k
}}
return s;""",
        )

    # --- Array build + foreach sum (40) ---
    for m in range(2, 42):
        k = m % 7 + 1
        n = write(
            n,
            f"arrsum_{m:02d}",
            f"""var a = [];
for (var i = 0; i < {m}; ++i) {{
	a += [i * {k}]
}}
var t = 0;
for (var v in a) {{
	t += v
}}
return t;""",
        )

    # --- Small Fibonacci (20) ---
    for f in range(8, 28):
        n = write(
            n,
            f"fib_{f:02d}",
            f"""var fib = function(n) {{
	return n < 2 ? n : fib(n - 1) + fib(n - 2);
}};
return fib({f});""",
        )

    # --- Functions & nested calls (25) ---
    for i in range(25):
        n = write(
            n,
            f"fn_{i:02d}",
            f"""function add3(a, b, c) {{
	return a + b + c;
}}
function scale(x, y) {{
	return x * y + {i};
}}
return scale(add3(1, 2, 3), {4 + (i % 5)});""",
        )

    # --- If / else chains (25) ---
    for i in range(25):
        n = write(
            n,
            f"ifchain_{i:02d}",
            f"""var x = {i + 3};
if (x < 8) {{
	return 1;
}} else if (x < 18) {{
	return 2;
}} else if (x < 28) {{
	return 3;
}}
return 4;""",
        )

    # --- Bitwise (20) ---
    for i in range(20):
        a = (i * 17 + 3) & 0xFF
        b = (i * 31 + 5) & 0xFF
        n = write(
            n,
            f"bits_{i:02d}",
            f"return ({a} & {b}) ^ ({a} | {b}) ^ ({a} <<1) >>1;",
        )

    # --- Real arithmetic (15) ---
    for i in range(15):
        n = write(
            n,
            f"real_{i:02d}",
            f"return floor(3.14159 * {i + 1});",
        )

    # --- Strings (build with +=, then count) (15) ---
    for i in range(15):
        n = write(
            n,
            f"str_{i:02d}",
            f"""var s = "";
for (var j = 0; j < {i + 1}; ++j) {{
	s += "xy";
}}
return count(s);""",
        )

    # --- Maps (15) ---
    for i in range(15):
        n = write(
            n,
            f"map_{i:02d}",
            f"""var m = [:];
m[0] = {i};
m[1] = {i + 1};
m[2] = {i + 2};
return m[0] + m[1] + m[2];""",
        )

    # --- Nested fors + break (10) — trimmed from reference style ---
    for t in range(10):
        lim = 5 + t
        n = write(
            n,
            f"nest_{t:02d}",
            f"""var sum = 0;
for (var i = 0; i < {lim}; ++i) {{
	for (var j = 0; j < {lim}; ++j) {{
		if (i == j) {{ continue; }}
		sum += i + j;
		if (sum > 200) {{ break; }}
	}}
}}
return sum;""",
        )

    gen_count = len(list(OUT.glob("*.leek")))
    snip_n, file_n, skip_res = emit_java_vm_corpus()
    java_total = sum(1 for _ in JAVA_VM_OUT.rglob("*.leek")) if JAVA_VM_OUT.is_dir() else 0

    print(f"Wrote {gen_count} files under {OUT.relative_to(ROOT)}")
    if snip_n or file_n or skip_res:
        print(
            f"Wrote {java_total} files under {JAVA_VM_OUT.relative_to(ROOT)} "
            f"({snip_n} snippets, {file_n} resource copies; {skip_res} missing resources skipped)"
        )
        print(
            "Bench Java VM corpus: "
            f"cargo run -p leekscript_bench --release -- "
            f"--corpus {JAVA_VM_OUT.relative_to(ROOT)} --recursive --respect-preamble"
        )
        print(
            "Bench full tree (synthetic + Java): "
            "cargo run -p leekscript_bench --release -- "
            f"--corpus {OUT.parent.relative_to(ROOT)} --recursive --respect-preamble"
        )


if __name__ == "__main__":
    main()
