# Local development

**Audience:** contributors running the workspace on their machine.

**Scope:** Common `cargo` workflows, smoke commands, and pointers to tests and fuzzing. **Canonical env var list:** [environment.md](environment.md).

## Prerequisites

- **Rust:** recent **stable** toolchain (`rustup`). The workspace uses **edition 2021**. **MSRV is not pinned** yet — see [platforms and MSRV](../operations/platforms-and-msrv.md).
- **Optional:** **nightly** Rust + [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) for harnesses under `fuzz/`.
- **Optional:** **JDK** + **`JAVA_HOME`** for JVM parity tests and `generator.jar` workflows (see [project charter](../overview/project-charter.md) — Rust paths are primary).

## Clone and build

From the repository root:

```bash
cargo build
```

Workspace members are listed in the root `Cargo.toml`; `fuzz/` is **excluded** from the default workspace (separate `cargo fuzz` project).

Initialize **Git submodules** only when you need reference trees (parity, scenarios on disk, API fetch against real layouts). See [repository map](../overview/repository-map.md).

## Everyday commands

These mirror the root [README.md](../../README.md); prefer that file for copy-paste examples if it drifts.

| Goal | Command (examples) |
|------|---------------------|
| CLI help | `cargo run -p lek -- --help` |
| Diagnostic registry + ref coverage | `cargo run -p lek -- registry --verify-emit-refs` |
| Validate manifest | `cargo run -p lek -- config` |
| Check / run a fixture | `cargo run -p lek -- check tests/fixtures/smoke.leek` / `run …` |
| Format | `cargo run -p lek -- fmt path/to/file.leek` (`--write` to save) |
| Generator CLI | `cargo run -p leek_wars_gen --bin leekgen -- …` |

Override the diagnostics registry file with **`LEEK_REGISTRY`** (see [environment.md](environment.md)).

## Testing

```bash
# Full workspace tests (can take a while)
cargo test

# Focus one crate
cargo test -p leekscript_run
cargo test -p leek_wars_gen
```

Some **`leek_wars_gen`** integration tests expect **`JAVA_HOME`** and a generator checkout for JVM parity; others are Rust-only. If a test fails with a missing JVM or path, check the test name and [environment.md](environment.md).

The **`leekscript_run`** Java VM export suite uses embedded expectations; optional path override: **`LEEKSCRIPT_TEST_RESOURCES`** (defaults to `leek-wars-generator/leekscript/src/test/resources` under the repo).

## Fuzzing

See **[fuzzing.md](../architecture/fuzzing.md)** for **`fuzz/`** targets vs stable **`leekgen-compare`**. Repro minimization: **`LEEKGEN_REPRO_DIR`**, **`LEEK_GENERATOR_CWD`** — [environment.md](environment.md). Quick commands also appear in the root **README.md**.

## Debugging and internals

- **Interpreter / pipeline:** `crates/leekscript_run` (`pipeline.rs`, `interp/`).
- **Fight engine:** `crates/leek_wars_gen`.
- Optional **`LEEK_ASTAR_DEBUG`** (any value set) enables extra pathfinding diagnostics in fight code — see [environment.md](environment.md).

## Related docs

- [environment.md](environment.md) — shell, `.envrc`, linker flags, env vars
- [contributing.md](contributing.md) — checks before a PR
- [rust-toolchain-crates.md](../architecture/rust-toolchain-crates.md) — pipeline layout
- [data-and-fixtures.md](../architecture/data-and-fixtures.md) — `data/`, `tests/fixtures/`
- [fuzzing.md](../architecture/fuzzing.md), [correctness-and-parity.md](../architecture/correctness-and-parity.md)

---

*Revision: initial local-dev guide; keep aligned with root README command blocks.*
