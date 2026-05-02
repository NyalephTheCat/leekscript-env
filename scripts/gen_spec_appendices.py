#!/usr/bin/env python3
"""Regenerate spec appendices from data/ sources.

  python3 scripts/gen_spec_appendices.py

Requires: PyYAML (python3-yaml). Run from repository root.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    print("Install PyYAML: pip install pyyaml", file=sys.stderr)
    sys.exit(1)

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "data/diagnostics/registry.yaml"
CORE_SIG = ROOT / "data/signatures/core.sig.leek"
LEEKWARS_SIG = ROOT / "data/signatures/leekwars.sig.leek"
OUT_C = ROOT / "docs/spec/appendices/C-diagnostic-codes-mapping.md"
OUT_F = ROOT / "docs/spec/appendices/F-builtin-signatures-catalog.md"

# band -> primary normative spec pointers (paths relative to docs/spec/)
BAND_SPEC: dict[str, str] = {
    "builtin_api": "[11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md)",
    "call_shape": "[08-expressions.md](../08-expressions.md), [10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md)",
    "collections": "[08-expressions.md](../08-expressions.md), [09-statements-and-control-flow.md](../09-statements-and-control-flow.md)",
    "config": "[leek-toml.md](../../reference/leek-toml.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md)",
    "deprecated": "[11-builtins-and-api-surface.md](../11-builtins-and-api-surface.md)",
    "directives": "[12-directives-and-pragmas.md](../12-directives-and-pragmas.md)",
    "expr": "[08-expressions.md](../08-expressions.md)",
    "ice": "[13-interpreter-behavior.md](../13-interpreter-behavior.md) (*internal*)",
    "include": "[09-statements-and-control-flow.md](../09-statements-and-control-flow.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md)",
    "lexical": "[03-lexical-grammar.md](../03-lexical-grammar.md)",
    "limits": "[13-interpreter-behavior.md](../13-interpreter-behavior.md)",
    "lint": "[diagnostics-registry.md](../../reference/diagnostics-registry.md) (*tooling*)",
    "names": "[05-names-and-scoping.md](../05-names-and-scoping.md)",
    "oop_struct": "[05-names-and-scoping.md](../05-names-and-scoping.md)",
    "parse": "[04-syntactic-grammar.md](../04-syntactic-grammar.md)",
    "platform": "[13-interpreter-behavior.md](../13-interpreter-behavior.md), [operations docs](../../operations/)",
    "runtime_iter": "[09-statements-and-control-flow.md](../09-statements-and-control-flow.md)",
    "runtime_val": "[07-semantics-overview.md](../07-semantics-overview.md), [08-expressions.md](../08-expressions.md), [13-interpreter-behavior.md](../13-interpreter-behavior.md)",
    "this_super": "[05-names-and-scoping.md](../05-names-and-scoping.md), [08-expressions.md](../08-expressions.md)",
    "types": "[06-types-and-subtyping.md](../06-types-and-subtyping.md)",
    "types_assign": "[06-types-and-subtyping.md](../06-types-and-subtyping.md)",
    "user_fn_decl": "[10-functions-and-call-conventions.md](../10-functions-and-call-conventions.md)",
    "visibility": "[05-names-and-scoping.md](../05-names-and-scoping.md)",
}


def code_sort_key(code: str) -> tuple[int, str]:
    m = re.match(r"E(\d+)", code)
    return (int(m.group(1)), code) if m else (99999, code)


def emit_registry_table() -> str:
    data = yaml.safe_load(REGISTRY.read_text())
    rows: list[tuple[str, str, str, str]] = []
    for e in data["entries"]:
        code = e["code"]
        ref = e.get("reference")
        tid = e.get("id")
        if ref and str(ref).lower() != "null":
            ident = str(ref)
        elif tid:
            ident = str(tid)
        else:
            ident = "—"
        band = e.get("band") or "—"
        spec = BAND_SPEC.get(band, "—")
        rows.append((code, ident, band, spec))
    rows.sort(key=lambda r: code_sort_key(r[0]))
    lines = [
        "| Code | `reference` / `id` | Band | Primary spec (*informative*) |",
        "|------|---------------------|------|------------------------------|",
    ]
    for code, ident, band, spec in rows:
        lines.append(f"| {code} | `{ident}` | {band} | {spec} |")
    return "\n".join(lines)


GLOBAL_RE = re.compile(r"^global\s+(\S+)\s+(\w+)\s*=")
FUNC_RE = re.compile(r"^function\s+(.+);\s*$")


def parse_sig_file(path: Path) -> tuple[list[str], list[str]]:
    globals_out: list[str] = []
    funcs: list[str] = []
    for line in path.read_text().splitlines():
        s = line.strip()
        gm = GLOBAL_RE.match(s)
        if gm:
            ty, name = gm.group(1), gm.group(2)
            globals_out.append(f"| `{name}` | `{ty}` |")
            continue
        fm = FUNC_RE.match(s)
        if fm:
            funcs.append(fm.group(1).strip())
    return globals_out, funcs


def emit_appendix_c() -> None:
    table = emit_registry_table()
    bands_lines = [
        "| Band | Typical spec chapters |",
        "|------|------------------------|",
    ]
    for band in sorted(BAND_SPEC.keys()):
        bands_lines.append(f"| `{band}` | {BAND_SPEC[band]} |")
    bands_tbl = "\n".join(bands_lines)

    content = f"""# Appendix C — Diagnostic codes mapping

**Normative policy**; the **registry table** below is a **generated snapshot** of the **bundled diagnostic registry** (stable **`E####`** ↔ **`reference`** / toolchain id). Human workflow and override rules: [diagnostics-registry.md](../../reference/diagnostics-registry.md). After changing the registry dataset, re-run **`python3 scripts/gen_spec_appendices.py`** from the repository root.

## Registry snapshot (`E####` ↔ identifier ↔ band)

{table}

## Band → spec guide (*informative*)

{bands_tbl}

## Static compilation phases

Phases in the **compilation API**: **Directives**, **Lexer**, **Parser**, **HIR**, **Resolve**, **Types**. Bands above map loosely to these phases (e.g. `lexical` → Lexer, `parse` → Parser, `names` → Resolve).

## Interpreter (`InterpretError`)

Stable **`reference`** strings include the interpreter’s **published emit list** and additional variants on **`InterpretError`** (e.g. **`TOO_MUCH_OPERATIONS`**, **`OUT_OF_MEMORY`**). Rows with bands **`runtime_val`**, **`runtime_iter`**, **`limits`**, **`collections`** often correspond to dynamic errors.

## PR checklist

When normative spec text introduces a **new** error condition, contributors **MUST**:

1. Add or reuse a row in the **diagnostic registry dataset**.
2. Emit that **`reference`** / id from the implementation.
3. Re-run **`python3 scripts/gen_spec_appendices.py`** and commit updated appendix C.
4. Add or extend a test cited from [E-conformance-tests-index.md](E-conformance-tests-index.md).

---

*Revision: includes generated registry table; maintain via `gen_spec_appendices`.*
"""
    OUT_C.write_text(content)
    print(f"Wrote {OUT_C.relative_to(ROOT)}")


def emit_appendix_f() -> None:
    g_core, f_core = parse_sig_file(CORE_SIG)
    g_lw, f_lw = parse_sig_file(LEEKWARS_SIG)

    def table_globals(rows: list[str]) -> str:
        if not rows:
            return "_No `global` declarations parsed._\n"
        hdr = "| Global | Type |\n|--------|------|\n"
        return hdr + "\n".join(rows) + "\n"

    def table_funcs(names: list[str]) -> str:
        if not names:
            return "_No `function` declarations parsed._\n"
        hdr = "| Signature |\n|-------------|\n"
        body = "\n".join(f"| `{s}` |" for s in names)
        return hdr + body + "\n"

    content = f"""# Appendix F — Builtin and global signatures catalog

**Informative.** This appendix is **generated** from **bundled signature-definition sources**: a **stdlib-oriented** layer and a **game-host API** layer (Leek-typed **`global`** and **`function`** headers). **Runtime** arity and behavior **MUST** still match the **interpreter** and **VM export parity** tests.

Regenerate via **`python3 scripts/gen_spec_appendices.py`** from the repository root.

## Stdlib-oriented signatures

**Globals** ({len(g_core)} rows)

{table_globals(g_core)}

**Functions** ({len(f_core)} rows)

{table_funcs(f_core)}

## Game-host API signatures

**Globals** ({len(g_lw)} rows)

{table_globals(g_lw)}

**Functions** ({len(f_lw)} rows)

{table_funcs(f_lw)}

## Relation to resolution

Name resolution seeds the **stdlib global identifier list** plus **`Infinity`**, **`PI`**, **`E`**. The signature layers **MAY** declare additional **`global`** constants; those names **SHOULD** appear in the merged global set used when **type-aware checking** loads signatures.

---

*Revision: generated catalog; maintain via `gen_spec_appendices`.*
"""
    OUT_F.write_text(content)
    print(f"Wrote {OUT_F.relative_to(ROOT)}")


def main() -> None:
    if not REGISTRY.exists():
        print(f"Missing {REGISTRY}", file=sys.stderr)
        sys.exit(1)
    emit_appendix_c()
    emit_appendix_f()


if __name__ == "__main__":
    main()
