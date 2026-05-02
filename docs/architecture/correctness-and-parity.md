# Correctness and parity (Rust vs Java generator)

**Audience:** contributors running **A/B harnesses**, interpreting **mismatches**, and extending **compare modes**.

**Scope:** How **`leek_wars_gen`** compares fight **outcomes** between the **Rust engine** and **`generator.jar`**, and what “equal” means in each mode. **Non-goals:** claiming full behavioral equivalence everywhere (see [project charter](../overview/project-charter.md)).

## Why parity exists

The **official Leek Wars generator** (JVM) is the historical **reference** for scenario execution. This workspace’s **Rust engine** is primary for speed and batch work, but **divergences are bugs** unless explicitly documented. Parity tooling answers: *for this scenario and seed, do both engines agree under a chosen definition of equality?*

## Outcome normalization

**`parity::normalize_outcome_json`** parses outcome JSON and:

- Removes top-level timing keys: **`analyze_time`**, **`compilation_time`**, **`execution_time`** (nanoseconds vary run-to-run).
- Normalizes **`logs`**: e.g. replaces **`▶`** with **`?`** so encoding/font differences do not fail compares.

**`outcomes_equal_ignore_timing`** compares **fully normalized** values for structural equality. **`diff_normalized_outcomes`** produces a bounded text diff (see **`MAX_NORMALIZED_DIFF_LINES`** in `parity.rs`).

Full normalized comparison includes **`logs`** and the full **`fight`** object (**`actions`**, **`ops`**, **`map`**, **`dead`**, …) — not only top-level fields.

## Compare modes (`harness::CompareMode`)

Used by the **scenario harness** / **`leekgen-compare`** flows:

| Mode | Meaning |
|------|---------|
| **`FullNormalized`** (default) | Normalize timing + logs, then **full** JSON equality. |
| **`WinnerDuration`** | Only **`winner`** and **`duration`** at the top level. |
| **`ActionsMinimal`** | Filtered action codes; **Java** action sequence must be a **subsequence** of **Rust** (see `minimal_action_code_filter`). |
| **`ActionsExact`** | Exact **`fight.actions`** equality after top-level normalization. |
| **`OpsExact`** | Exact **`fight.ops`** equality after top-level normalization. |

## Compare results

**`CompareResult`** (see `harness.rs`) distinguishes **match**, **semantic mismatch** (with optional normalized diff string), **winner/duration mismatch**, **actions/ops mismatch**, **engine errors** (**`EngineRunMismatch`**), and **non-JSON outcome** (**`OutcomeNotJson`**).

**`ScenarioHarnessReport::comparison_failed`** treats everything except clear matches (and “no A/B comparison” notes) as a failure for batch summaries.

## Entry points

- **`leekgen-compare`** binary (see root [README.md](../../README.md)): stable RNG fuzz vs JVM, scenario harness, artifact export.
- **Integration tests** under **`crates/leek_wars_gen/tests/`** (often require **`JAVA_HOME`** and generator checkout).
- Library: **`harness`**, **`parity`**, **`compare_fuzz_cli`**.

## Related

- [generator-and-engine.md](generator-and-engine.md)
- [fuzzing.md](fuzzing.md)
- [environment.md](../guides/environment.md)
- [scenario-format.md](../reference/scenario-format.md)

---

*Revision: operational parity doc; extend when new `CompareMode` variants land.*
