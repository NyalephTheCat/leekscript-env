# Data directory and test fixtures

**Audience:** contributors changing diagnostics, benchmarks, or shared test inputs.

**Scope:** What lives under **`data/`**, how it relates to **`tests/fixtures/`**, and **regeneration / ownership** expectations. **Non-goals:** full LeekScript language semantics (see planned `docs/spec/`).

## `data/` layout

| Path | Contents | Consumers |
|------|-----------|-----------|
| **`data/diagnostics/registry.yaml`** | Stable **`E####`** codes mapped to Java `Error` names (`reference`), optional **`id`** for toolchain-only diagnostics, **`band`** grouping. | `leekscript_diagnostics`, `lek registry`, all crates emitting diagnostics. |
| **`data/diagnostics/README.md`** | Short pointer to the registry. | Humans. |
| **`data/signatures/`** | Bundled **`.sig.leek`** files (`core.sig.leek`, `leekwars.sig.leek`) for global/function stubs (Leek Wars–style APIs). | `leekscript_run` (`sig_workspace`, tests), tooling that merges signature units. |
| **`data/bench_corpus/`** | Large **`.leek`** corpus: **`java_vm/`** (snippets derived from Java VM test exports), **`generated/`** (synthetic programs). | `leekscript_bench`, local perf work — **not** loaded by default `cargo test`. |

Override registry path at run time with **`LEEK_REGISTRY`** (see [environment.md](../guides/environment.md)).

## Workspace `tests/fixtures/`

Small **`.leek`** files used for smoke and CLI examples (e.g. **`smoke.leek`**, error samples like **`unclosed.leek`**). These are **lightweight**; keep them stable for documentation and quick `lek check` / `lek run` invocations.

**Per-crate tests** live next to each crate (`crates/*/tests/`); many integration tests build paths relative to **`CARGO_MANIFEST_DIR`** or the repo root.

## Sync and policy

- **Registry:** Do **not** reassign existing **`E####`** codes. New Java `Error` variants or toolchain references require new rows; see [diagnostics-registry.md](../reference/diagnostics-registry.md). The YAML header’s **`reference_source`** points at the upstream Java enum path for traceability.
- **Bench corpus:** Regenerating or extending **`data/bench_corpus`** should stay reproducible (script or documented procedure — add here when one exists).
- **Signatures:** Changes to **`*.sig.leek`** affect resolve/type behavior for AI and game-shaped code; coordinate with tests that embed or merge signatures.

Fight **scenario** files (JSON/TOML for `leekgen`) are documented in **[scenario-format.md](../reference/scenario-format.md)**; upstream-style examples usually live under the **`leek-wars-generator/test/scenario`** submodule path.

## Related

- [scenario-format.md](../reference/scenario-format.md)
- [diagnostics-registry.md](../reference/diagnostics-registry.md)
- [local-development.md](../guides/local-development.md)
- [repository map](../overview/repository-map.md)

---

*Revision: initial data/fixtures architecture.*
