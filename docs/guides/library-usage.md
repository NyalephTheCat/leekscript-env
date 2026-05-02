# Library usage (embedding)

**Audience:** Rust developers who depend on workspace crates **without** the `lek` CLI.

**Scope:** Where to find the **canonical** embedding guide and how this page relates to it. **Non-goals:** duplicating API tables or threading notes—those live in the architecture doc.

## Canonical documentation

All substantive guidance is in **[Embedding the LeekScript toolchain](../architecture/embed-toolchain.md)** (`docs/architecture/embed-toolchain.md`): `leekscript_run` entry points, compile vs interpret, diagnostics, threading, and stability expectations.

Use this **`guides/library-usage.md`** path when you want a **guides/** entry point (onboarding lists, external links); treat **embed-toolchain** as the single source of truth for technical detail.

## Quick orientation

- Add **`leekscript_run`** (path or git dependency) and call **`compile_source`** / **`interpret_hir`** as described in the embed doc.
- Configuration and file preamble behavior are documented under [Leek.toml](../reference/leek-toml.md) and [Directives](../reference/directives.md).

## Related

- [Local development](local-development.md) — building and testing the workspace.
- [Rust crates & pipeline](../architecture/rust-toolchain-crates.md) — where `leekscript_run` sits in the graph.
