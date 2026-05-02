# LeekScript language specification (index)

**Audience:** implementers, reviewers, and advanced users who need a **single normative story** for the language (not just CLI tooling).

**Scope:** Index and **reading order** for chapters under **`docs/spec/`**. **Tooling-only** material stays in **`docs/reference/`** (e.g. **`Leek.toml`**, scenario JSON).

## Status

| Topic | Status |
|-------|--------|
| **Spec version** | **Unassigned** — no `LeekScript x.y` label yet. Chapters **00–13** and **appendices A–C**, **E**, **F** are **drafted**; **C** and **F** are **machine-generated** from registry and signature data (run **`python3 scripts/gen_spec_appendices.py`** from the repo root after edits). |
| **Implementation** | Primary language tooling and interpreter live in this repository’s workspace; see architecture documentation for package roles. |
| **Relationship to VM** | Default interpreter aims at **game VM** alignment; see [project charter](../overview/project-charter.md). **Implementation notes (this repository)** mark known divergences from the **reference implementation** (e.g. postfix `++`/`--`, `===`). |

When the spec gains a version string, record it here and in [release & versioning](../operations/release-and-versioning.md).

## How to read

1. **[00-conventions-and-notation.md](00-conventions-and-notation.md)** — RFC 2119, normative vs informative, grammar notation.
2. **01–13** and **appendices** — chapter list below; keep status in this README aligned with tests and registry coverage (**`lek registry --verify-emit-refs`**).

## Chapter map

| Ch. | File | Topic |
|-----|------|--------|
| 00 | [00-conventions-and-notation.md](00-conventions-and-notation.md) | Conventions |
| 01 | [01-introduction-and-scope.md](01-introduction-and-scope.md) | Introduction, scope, reference vs this implementation |
| 02 | [02-unicode-and-source-text.md](02-unicode-and-source-text.md) | UTF-8, lines, includes |
| 03 | [03-lexical-grammar.md](03-lexical-grammar.md) | Tokens, literals, versioned keywords |
| 04 | [04-syntactic-grammar.md](04-syntactic-grammar.md) | Items, precedence sketch |
| 05 | [05-names-and-scoping.md](05-names-and-scoping.md) | Bindings, classes |
| 06 | [06-types-and-subtyping.md](06-types-and-subtyping.md) | Type syntax, runtime kinds, minimal static checks |
| 07 | [07-semantics-overview.md](07-semantics-overview.md) | Evaluation order, abrupt completion |
| 08 | [08-expressions.md](08-expressions.md) | All `HirExpr` forms |
| 09 | [09-statements-and-control-flow.md](09-statements-and-control-flow.md) | All `HirStmt` forms |
| 10 | [10-functions-and-call-conventions.md](10-functions-and-call-conventions.md) | Calls, `@`, closures |
| 11 | [11-builtins-and-api-surface.md](11-builtins-and-api-surface.md) | Builtins + [appendix F](appendices/F-builtin-signatures-catalog.md) |
| 12 | [12-directives-and-pragmas.md](12-directives-and-pragmas.md) | `// leek-*` (see [directives.md](../reference/directives.md)) |
| 13 | [13-interpreter-behavior.md](13-interpreter-behavior.md) | Limits, strict mode, host |
| A | [appendices/A-grammar-summary.md](appendices/A-grammar-summary.md) | Grammar summary (*informative*) |
| B | [appendices/B-reserved-and-future-keywords.md](appendices/B-reserved-and-future-keywords.md) | Keywords by version |
| C | [appendices/C-diagnostic-codes-mapping.md](appendices/C-diagnostic-codes-mapping.md) | **`E####`** table (*generated*) + policy |
| E | [appendices/E-conformance-tests-index.md](appendices/E-conformance-tests-index.md) | Tests index (*informative*) |
| F | [appendices/F-builtin-signatures-catalog.md](appendices/F-builtin-signatures-catalog.md) | Builtin / global signatures (*generated*) |

## Cross-links

| Artifact | Role |
|----------|------|
| [00-conventions-and-notation.md](00-conventions-and-notation.md) | Normative language and doc conventions. |
| [directives.md](../reference/directives.md) | Operational **`// leek-*`** (spec ch. 12 will normativize). |
| [diagnostics-registry.md](../reference/diagnostics-registry.md) | **`E####`** / `reference` ids. |
| [embed-toolchain.md](../architecture/embed-toolchain.md) | Library API (not language semantics). |
| [release-and-versioning.md](../operations/release-and-versioning.md) | Git revision vs future spec/crate versioning. |

---

*Revision: full chapter index; bump spec version when assigned.*
