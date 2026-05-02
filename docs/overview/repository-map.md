# Repository map

**Audience:** someone cloning the repo who needs a **mental model of top-level directories** and what is “workspace core” vs reference-only trees.

**Scope:** Layout and pointers. **Non-goals:** per-file inventories of `data/` or `tests/` (see future `docs/architecture/data-and-fixtures.md`).

## Reference submodules (not part of the main Rust dependency graph)

`leek-wars/`, `leek-wars-generator/`, and `ai/` are **Git submodules** used as **upstream reference and assets**: parity checks, reproducing scenarios, **fetching current data**, fuzz corpora, etc. **Workspace crates do not depend on them at build time** for normal `lek` / `leekgen` development—they are optional on disk unless you run those workflows. See **[project-charter.md](project-charter.md)** for the product intent.

## Top-level layout

| Path | Role |
|------|------|
| **`Cargo.toml`** | Workspace manifest: members are Rust crates under `crates/`; `fuzz/` is **excluded** from default `cargo build` / `cargo test`. |
| **`crates/`** | All primary Rust packages: `lek`, `leekscript_*`, `leek_wars_gen`, `lw_meta`, `leekscript_bench`, etc. |
| **`data/`** | Machine-readable artifacts (e.g. **`data/diagnostics/registry.yaml`** — diagnostic `E####` / reference ids). Override path with env **`LEEK_REGISTRY`**. |
| **`tests/`** | Workspace-level integration tests and fixtures (alongside per-crate `tests/`). |
| **`fuzz/`** | `cargo-fuzz` targets (nightly). Includes repro minimization that expects **`LEEKGEN_REPRO_DIR`**, **`LEEK_GENERATOR_CWD`**, etc. |
| **`scripts/`**, **`tools/`** | Helper scripts and tooling (index TBD). |
| **`ai/`** | Git submodule — AI assets / experiments; **reference**, not a Cargo dependency of core crates. |
| **`leek-wars/`** | Git submodule — upstream **game / web** tree; **reference** (e.g. data, context), not linked into main library code. |
| **`leek-wars-generator/`** | Git submodule — Java generator checkout; **`generator.jar`**, scenarios, test AI. Set **`LEEK_GENERATOR_CWD`** when comparing to JVM, resolving generator-relative paths, or minimizing fuzz repros. |
| **`Leek.toml`** | Example / workspace manifest for **`[language]`**, **`[fmt]`**, **`[generator]`**, signatures, etc. Used by `lek config` and `leekgen` when discovered by directory walk-up. |
| **`.github/workflows/`** | **CI** (fmt, clippy, test, registry verify) and **dependencies** (`cargo-deny`, `audit-check`) — see [ci-and-quality-gates.md](../operations/ci-and-quality-gates.md). |
| **`deny.toml`** | **`cargo-deny`** policy (licenses, advisories, bans) — see [CI & quality gates](../operations/ci-and-quality-gates.md). |
| **`README.md`** | Quick build and CLI examples; points to **`docs/README.md`** for doc navigation. |
| **`.envrc`** | Optional **direnv** hook (e.g. `RUSTFLAGS` for linker choice). Not required for all contributors. |
| **`.cargo/`** | Cargo config (document any reproducibility-relevant flags in a future operations page). |
| **`.config/`** | Local developer config (referenced from `.envrc` comment). |

## Submodule summary (`.gitmodules`)

| Submodule | URL (origin) |
|-----------|----------------|
| `leek-wars` | `github.com:leek-wars/leek-wars.git` |
| `leek-wars-generator` | `github.com:leek-wars/leek-wars-generator.git` |
| `ai` | `github.com:NyalephTheCat/leekwars-ai.git` |

Initialize submodules when you need **parity runs**, **JVM comparison**, **live data fetch** workflows that expect those trees, or **fuzz** setups that read generator-relative paths:

```bash
git submodule update --init --recursive
```

For **compiler / interpreter-only** work (`lek check`, `lek run`, most unit tests), a clone **without** submodules can be sufficient.

## Crate index (workspace members)

From **[Cargo.toml](../../Cargo.toml)** (workspace `members`):

- **CLI / product surface:** `lek`
- **Pipeline:** `leekscript_span`, `leekscript_diagnostics`, `leekscript_config`, `leekscript_directives`, `leekscript_lexer`, `leekscript_parser`, `leekscript_syntax`, `leekscript_fmt`, `leekscript_hir`, `leekscript_resolve`, `leekscript_signatures`, `leekscript_types`, `leekscript_run`
- **Quality / research:** `leekscript_fuzz`, `leekscript_bench`
- **Simulation / meta:** `leek_wars_gen`, `lw_meta`

**Excluded from workspace:** `fuzz/` (separate `cargo fuzz` project).

## Documentation map

Canonical doc index: **[docs/README.md](../README.md)**. **Why submodules exist:** **[external-repositories.md](external-repositories.md)**. Day-to-day contributor pages: **[local-development.md](../guides/local-development.md)**, **[environment.md](../guides/environment.md)**, **[contributing.md](../guides/contributing.md)**. **Data:** **[data-and-fixtures.md](../architecture/data-and-fixtures.md)**. **Tooling reference:** **[lek-cli.md](../reference/lek-cli.md)**, **[leek-toml.md](../reference/leek-toml.md)**, **[directives.md](../reference/directives.md)**, **[diagnostics-registry.md](../reference/diagnostics-registry.md)**. **Simulation:** **[generator-and-engine.md](../architecture/generator-and-engine.md)**, **[leek-wars-gen-experiments-and-trace.md](../architecture/leek-wars-gen-experiments-and-trace.md)**, **[scenario-format.md](../reference/scenario-format.md)**. **Operations:** **[platforms-and-msrv.md](../operations/platforms-and-msrv.md)**, **[ci-and-quality-gates.md](../operations/ci-and-quality-gates.md)**, **[release-and-versioning.md](../operations/release-and-versioning.md)**. **Engineering:** **[observability.md](../engineering/observability.md)**, **[error-messages-and-ux.md](../engineering/error-messages-and-ux.md)**, **[performance.md](../engineering/performance.md)**. **ADRs:** **[adr/README.md](../adr/README.md)**. **Crate paths:** **[crates/README.md](../crates/README.md)**. **Language spec:** [spec/README.md](../spec/README.md).

## Future improvements

- **Single diagram:** add a Mermaid “repo + data flow” figure shared with [`rust-toolchain-crates.md`](../architecture/rust-toolchain-crates.md) and generator docs.
- **`scripts/` / `tools/` index:** one table listing scripts and tools under those directories.

---

*Revision: clarified reference-only role of game/generator/ai submodules.*

