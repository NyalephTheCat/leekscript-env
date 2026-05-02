# Project charter and scope

**Audience:** new contributors, auditors, and maintainers setting boundaries for what this repository promises.

**Scope:** Mission, in-scope work, explicit non-goals, and relationship to Leek Wars and upstream generator code. **Out of scope:** specifying the game server protocol or full upstream product behavior (link to upstream docs instead).

## Mission

This repository hosts the **LeekScript toolchain** and **Rust-first simulation / batch tooling** built around it:

- **Language tooling:** parse, analyze, format (token-based today), and **interpret** LeekScript, with a **machine-readable diagnostic registry** and **HIR-centric** pipeline.
- **Goals beyond the official Java generator:** a **faster** path for **batch fights** (testing, benchmarking, experiments), room for **tooling the official stack does not provide** (e.g. **LSP for LeekScript** in the future), and other enhancements while keeping **behavior aligned with the game VM by default**.
- **Simulation stack:** Rust fight engine and CLI (`leekgen`), scenario formats compatible with reference assets, **meta** HTTP helpers (`lw_meta`), and **experiment / optimization** workflows.

The Rust workspace is intended to **grow more capable** than the official generator for developer workflows; parity with the reference implementation is the **default semantic baseline**, not the ceiling.

## Relationship to Leek Wars and upstream repos

- **LeekScript** is the scripting language used in the Leek Wars ecosystem. The **interpreter defaults to alignment with the in-game VM**; flags, CLI options, and library APIs may **opt into** divergences that improve speed, observability, or extra information where that trade-off is intentional (document such modes in CLI help, `docs/spec/`, and architecture pages as they stabilize).
- **`leek-wars/`**, **`leek-wars-generator/`**, and **`ai/`** (submodules) are **reference and data sources**. They are **not** dependencies of the main Rust code paths. They are used for **comparing results**, **fetching up-to-date game data**, fuzz reproduction, and similar **adjacent** workflows—not for shipping the core toolchain.

## In scope

- Rust crates under `crates/` (CLI `lek`, compiler pipeline, diagnostics, generator, benches, fuzz helpers).
- Workspace-owned **`data/`** (e.g. diagnostic registry YAML) and **`tests/`** fixtures.
- **`fuzz/`** (nightly `cargo-fuzz` harnesses; excluded from the default workspace).
- Documentation under **`docs/`** (see **[docs/README.md](../README.md)** for navigation).

## Out of scope (unless explicitly added later)

- Authoritative **product documentation** for Leek Wars or the Java generator — link upstream.
- **Formal verification** of the Rust implementation.
- **Hosting** or **operating** game infrastructure.

## Toolchain and stability

- **MSRV (minimum supported Rust version):** **intentionally unspecified** for now; maintainers may introduce a pinned MSRV or `rust-toolchain.toml` later. Until then, expect **recent stable** Rust to be the practical target (edition **2021** per workspace `Cargo.toml`).
- **Releases and version skew:** Crate **`version = "0.1.0"`** fields are placeholders; **Git revision**, **`data/`** artifacts, and **submodule** commits matter more than those numbers today. **crates.io** is **not** the distribution path for the short/medium term (workspace crates set **`publish = false`**). See [Release and versioning](../operations/release-and-versioning.md).

## Quality posture

Documentation in `docs/` should stay **accurate and navigable**: reproducible commands, clear boundaries for submodules, and honest notes where behavior differs from the VM on purpose.

**Practical setup:** [Local development](../guides/local-development.md), [Environment](../guides/environment.md), [Contributing](../guides/contributing.md). **Submodules:** [External repositories](external-repositories.md).

---

*Revision: charter updated with reference-submodule policy, VM-default interpreter stance, extended goals (batch, LSP, etc.), and MSRV note.*
