# `leek_wars_gen`: experiments, trace, cache, and metrics

**Audience:** auditors and contributors using **batch runs**, **optimization**, or **fight telemetry**.

**Scope:** Rust-only helpers in **`crates/leek_wars_gen`** around scenarios and outcomes. **Out of scope:** official outcome JSON schema (still shaped like the Java generator); see [scenario format](../reference/scenario-format.md).

## Fight trace (Rust sidecar)

- **`TraceConfig`** / **`TraceEvent`** (`src/fight/trace.rs`): optional **telemetry** during a Rust engine run (`enabled`, **`max_events`** default **10_000**).
- Passed via **`FightRunOptions::trace`** (`run_scenario_path_with_options`). Events are **not** part of standard outcome JSON—see [correctness-and-parity.md](correctness-and-parity.md) for parity and replay context.
- **`trace_summarize`** (`experiment/trace_summarize.rs`): summarize trace streams (e.g. NDJSON) for human-readable rollups.

## Experiment subsystem (`experiment/`)

- **Declarative TOML** specs: scenario sweeps, tunable grids, batch execution—module docs in **`src/experiment/mod.rs`**, sample **`examples/experiment_sample.toml`**.
- **`planner` / `batch`:** expand specs into **`RunTask`** units and execute (`run_experiment`, `execute_run_task`, NDJSON **`Manifest`** / **`RunRecord`**).
- **`metrics::RunMetrics`:** small stable struct (**`winner`**, **`duration`**, **`error`**) parsed from outcome JSON for optimizers and aggregates.
- **`cache`:** content-addressed keys (**SHA-256** over scenario JSON + arm + tunables + crate version) for **`*.outcome.json`** paths—skip recomputation when **`nocache`** is not set on the engine path that uses it.
- **`optimize`:** coordinate / hill-climb style hooks over tunables (see module `optimize.rs`).
- **`aggregate`:** combine batch results for reporting.

## Java engine flags (parity)

- **`engine`:** `RunRequest` can pass **`nocache`** through to **`generator.jar`** (`--nocache`) where supported—see `src/engine/mod.rs`.

## Related

- [Generator and engine](generator-and-engine.md)
- [Correctness and parity](correctness-and-parity.md)
- [Fuzzing](fuzzing.md)
- Crate **[`leek_wars_gen`](../../crates/leek_wars_gen)**

---

*Revision: trace caps, experiment modules, cache keys, metrics—implementation map.*
