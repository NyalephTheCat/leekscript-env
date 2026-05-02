# Appendix E — Conformance tests index (*informative*)

Maps spec areas to **tests and fuzz** in this repository. Expand as coverage grows.

| Spec area | Tests / targets |
|-----------|------------------|
| **Lexer / parse / HIR / resolve / types / run** | CLI package integration tests (corpus check, run, signatures, registry ref verification, …) |
| **Interpreter vs reference VM** | Export parity suite under the interpreter crate’s tests |
| **Scenario / generator parity** | Generator crate parity and harness tests (winner/actions/ops compares, stress) |
| **Fuzz** | Dedicated fuzz crate and nightly harness tree (see [fuzzing.md](../../architecture/fuzzing.md)) |
| **Experiment / trace** | Generator experiment/trace integration tests |

**Maintenance:** When adding a normative clause, **SHOULD** add or reference a test path here in the same PR.

**Generated appendices:** After edits to the **bundled diagnostic registry** or **signature** sources, run **`python3 scripts/gen_spec_appendices.py`** from the repository root (updates [C](C-diagnostic-codes-mapping.md) and [F](F-builtin-signatures-catalog.md)).

---

*Revision: conformance index.*
