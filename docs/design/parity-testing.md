# Parity testing: JVM oracle and frozen corpus

This document fixes **how** the Rust toolchain proves equivalence with the reference implementation.

---

## Primary oracle: JVM reference compiler

The **authoritative** behavior for standard LeekScript is the Java implementation in `leek-wars-generator/leekscript`:

- **Compile-time:** `leekscript.compiler` (lexer through `WordCompiler`, `Error` enum, messages implied by `AnalyzeError`).
- **Runtime:** `leekscript.runner` when executing generated code.

**CI strategy (recommended):**

1. **Differential tests:** For selected snippets and files, run the Java compiler (or packaged `leekscript.jar`) and the Rust pipeline, then compare:
   - success vs failure,
   - list of **reference** `Error` ids (and eventually message templates),
   - stable **`E####`** codes from [`data/diagnostics/registry.yaml`](../../data/diagnostics/registry.yaml).

2. **Version matrix:** Same inputs under options `(version, strict)` matching `TestCommon` patterns in the Java repo (`code_v4`, `code_strict_v4_`, …).

3. **Optional JVM bridge crate** (`leekscript_runner_jvm`): keeps parity tests cheap before a Rust VM exists; not required for end-user `lek` once the native runner matches.

---

## Secondary oracle: frozen corpus

A **frozen corpus** is a version-controlled set of `.leek` sources plus **expected artifacts** that do not depend on running the JVM every time:

- **Token snapshots** (optional): golden lexer output for regression in the Rust lexer only.
- **AST/CST snapshots** (optional): parser recovery and shape.
- **Diagnostic snapshots:** expected `(E####`, `reference`, span)` lists for `lek check`.

**Rules:**

- Corpus files live under a dedicated tree (e.g. `tests/corpus/` or `data/corpus/`) with a `README` describing update policy.
- **Updating** the golden output requires either (a) JVM reference run recorded in the commit message, or (b) an explicit “intentional language/toolchain change” note.
- Frozen tests catch **Rust-only** regressions quickly; they do **not** replace JVM oracle tests when the Java implementation changes—refresh corpus when upgrading the submodule.

---

## Relationship between the two

| Concern | JVM oracle | Frozen corpus |
|--------|------------|----------------|
| Trust source | Reference implementation | Last agreed snapshot |
| Speed | Slower (JVM, classpath) | Fast (pure Rust) |
| When it changes | Java `Error` / behavior updates | On purpose or after JVM refresh |
| Best for | Semantic truth, API drift | Day-to-day CI, lexer/parser |

**Workflow:** Land a behavior change in Java first (or in lockstep), update **`data/diagnostics/registry.yaml`** if `Error` or `E####` mappings change, then refresh corpus snapshots and JVM differential baselines.

---

## Document status

Operational detail (exact Gradle invocations, submodule pins) belongs in the repo `README` or CI config once the workspace exists.
