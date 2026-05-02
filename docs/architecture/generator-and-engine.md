# Generator, fight engine, and `leekgen`

**Audience:** contributors working on simulation, parity, or batch experiments.

**Scope:** Crate **`leek_wars_gen`**, its **Rust vs Java** engines, and how **`leekscript_run`** fits in. **Non-goals:** duplicating [scenario-format.md](../reference/scenario-format.md) field-by-field.

**Charter context:** the **Rust engine is primary** for day-to-day runs; the **JVM generator** is for **comparison**, repros, and upstream compatibility—see [project charter](../overview/project-charter.md).

## Crate role (`leek_wars_gen`)

From the crate root docs:

- **Scenario:** JSON/TOML compatible with **`generator.jar`** / official tooling.
- **Engines:** **`RustEngine`** (in-process, **`leekscript_run`**) vs **`JavaEngine`** (`java -jar …`).
- **Parity:** normalize outcome JSON (e.g. strip volatile fields) for comparisons.
- **Fight / sim:** **`fight`** runs scenarios with an interpreter **`FightHost`** (simplified vs full upstream in documented areas).
- **Experiments:** TOML specs, sweeps, cache, NDJSON manifests, **`optimize`** (coordinate / hill-climb); **`pvp`** batches from live API ids — see **[leek-wars-gen-experiments-and-trace.md](leek-wars-gen-experiments-and-trace.md)** (trace caps, **`RunMetrics`**, cache keys).
- **Meta:** HTTP via **`lw_meta`** (`leekgen meta …`) — see crate **[`lw_meta`](../../crates/lw_meta)**.

## Configuration resolution

Generator paths (**root**, **scenarios_dir**, **ai_dir**, **output** format) resolve with precedence:

1. CLI root override  
2. **`LEEK_GENERATOR_CWD`**  
3. **`[generator]`** in **`Leek.toml`** (walk-up discovery)  
4. Defaults (e.g. **`root/test/scenario`**, **`root/test/ai`**)

See [leek-toml.md](../reference/leek-toml.md) and `crates/leek_wars_gen/src/config.rs`.

## Engines

| Engine | When | Notes |
|--------|------|--------|
| **Rust** | Default for `leekgen run`, sim, fuzz, most experiments | Executes AI **`.leek`** through **`leekscript_run`** inside the fight loop. |
| **Java** | Explicit compare / parity / harness tests | **`generator.jar`** from **`LEEK_GENERATOR_JAR`** or search path; needs **`JAVA_HOME`** for many tests. |

**`RunRequest`** mirrors flags understood by **`com.leekwars.Main`** for Java argv forwarding.

## Major modules (entry points for reading code)

| Area | Module / path |
|------|----------------|
| Scenario serde | `scenario`, `scenario_io` |
| World / turns | `fight` (`world`, `run`, `trace`, …) |
| Parity / fuzz | `parity`, `fuzz`, `compare_fuzz_cli`, `harness` |
| Experiments | `experiment` (`spec`, `planner`, `batch`, `cache`, `optimize`, `meta`, …) |
| Output | `output` |
| Meta HTTP | re-export **`lw_meta`** |

## Relationship to the LeekScript toolchain

- **`lek`**: compile/check/run **standalone** `.leek` for tooling.
- **`leekgen`**: runs **full fights**; AI scripts are **embedded** in scenarios and executed via **`interpret_hir`** (and related) with a game-shaped **host**.

For lexer → HIR → interpret **pipeline** only, see [rust-toolchain-crates.md](rust-toolchain-crates.md).

## Related

- [leek-wars-gen-experiments-and-trace.md](leek-wars-gen-experiments-and-trace.md)
- [correctness-and-parity.md](correctness-and-parity.md)
- [fuzzing.md](fuzzing.md)
- [scenario-format.md](../reference/scenario-format.md)
- [environment.md](../guides/environment.md)
- **[`lw_meta`](../../crates/lw_meta)** (API client / `lw-meta` CLI)
- Root [README.md](../../README.md) — `leekgen` examples

---

*Revision: architecture overview; deepen per submodule as needed.*
