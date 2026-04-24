#!/usr/bin/env python3
"""
Mine single-line TestCommon-style cases from the Java JUnit sources.

Matches calls like:
  code_v4_("return 1").equals("1")
  code("return true").equals("true")

Output: TSV with columns: file, method_name_guess, snippet, expected_java_export

Limitations:
  - Only single-line string literals (no embedded quotes).
  - Multiline snippets and .almost(...) / .error(...) are skipped.

Usage:
  python3 scripts/parity/extract_snippets_from_java_tests.py \\
    leek-wars-generator/leekscript/src/test/java/test > /tmp/cases.tsv

For the full JVM parity suite in Rust (static tables + grouped tests), run:
  python3 scripts/extract_java_vm_cases.py
  cargo test -p leekscript_run --test java_vm_suite -- --ignored
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# code / code_v1_2 / code_strict_v4_ / ...
CALL_RE = re.compile(
    r"\bcode(?:_[A-Za-z0-9]+)*\(\s*\"([^\"]*)\"\s*\)\s*\.\s*equals\s*\(\s*\"([^\"]*)\"\s*\)"
)


def current_method_name(lines: list[str], line_idx: int) -> str:
    for j in range(line_idx, -1, -1):
        m = re.search(r"\bvoid\s+(\w+)\s*\(", lines[j])
        if m:
            return m.group(1)
    return "?"


def main() -> None:
    if len(sys.argv) != 2:
        print("usage: extract_snippets_from_java_tests.py <test-java-dir>", file=sys.stderr)
        sys.exit(2)
    root = Path(sys.argv[1])
    if not root.is_dir():
        print(f"not a directory: {root}", file=sys.stderr)
        sys.exit(2)

    print("file\tmethod\tsnippet\texpected_export")
    for path in sorted(root.glob("Test*.java")):
        text = path.read_text(encoding="utf-8", errors="replace")
        lines = text.splitlines()
        for i, line in enumerate(lines):
            for m in CALL_RE.finditer(line):
                method = current_method_name(lines, i)
                snippet, expected = m.group(1), m.group(2)
                print(
                    f"{path.name}\t{method}\t{snippet.replace(chr(9), ' ')}\t{expected.replace(chr(9), ' ')}"
                )


if __name__ == "__main__":
    main()
