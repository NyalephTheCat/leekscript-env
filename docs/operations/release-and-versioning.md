# Release and versioning

**Audience:** maintainers, integrators embedding this repository, and reviewers asking how **versions** and **breaking changes** are (or will be) governed.

**Scope:** **This Git workspace** (`lek`, libraries, `leekgen`, `lw_meta`). **Out of scope:** Leek Wars product releases, upstream `leek-wars` / `leek-wars-generator` tagging policy (record their submodule revisions when comparing behavior).

## Current state (repository)

| Topic | Status |
|-------|--------|
| **Distribution** | **Git-only** for the **short and medium term** (maintainer intent): consume this repo at a chosen **commit** or **branch**. **crates.io** is **not** planned in that window; workspace manifests set **`publish = false`** so `cargo publish` does not accidentally ship crates. |
| **Crate `version` fields** | Workspace members use **`0.1.0`** in each crate’s `Cargo.toml` as a **placeholder**. Those numbers are **not** yet a semver contract for external dependents. |
| **Root `rust-toolchain.toml`** | **Absent** — see [Platforms & MSRV](platforms-and-msrv.md). |
| **Language spec label** | **Unassigned** — no `LeekScript x.y` string; see [spec README](../spec/README.md). |
| **Changelog** | No root **`CHANGELOG.md`** policy is enforced yet; **Git history** is the practical record. |

## What “version” means today

1. **Git revision** (commit SHA, or branch pointer) is the meaningful **integration anchor** for anyone building from source.
2. **Diagnostic and builtin contracts** evolve with **`data/`** (e.g. registry YAML) and generated spec appendices — see [registry operations](../reference/registry.md).
3. **Library embedding** (`leekscript_run`, etc.) is described in [embed-toolchain.md](../architecture/embed-toolchain.md); stability expectations there should stay consistent with any future semver policy.

## Intended direction (not yet all policy)

Near term, **Git revision** remains the integration story; **`publish = false`** stays unless the project explicitly opts into **crates.io** later.

When the project matures, maintainers may additionally adopt some or all of:

- **Pinned MSRV** and optional **`rust-toolchain.toml`** (see [platforms-and-msrv.md](platforms-and-msrv.md)).
- **A named language spec version** (independent of Rust crate versions), with a short **spec changelog** in or beside `docs/spec/`.
- **Semantic versioning** and **crates.io** (or another registry) for selected crates, with a **breaking-change policy** for public APIs (`lek` CLI flags, `leekscript_run` exports) — **longer-term**; would remove or narrow **`publish = false`** only for crates intended for publication.
- **Git tags** (e.g. `v0.2.0`) aligned with release notes or a root **`CHANGELOG.md`**.

Until then, treat **compatibility** as **commit + submodule revisions + data/** artifacts, not as semver on `0.1.0` alone.

## Related

- [Release process (checklist)](../guides/release-process.md) — practical steps before merge or tagging.
- [CI and quality gates](ci-and-quality-gates.md) — what merges are expected to satisfy.
- [Project charter](../overview/project-charter.md) — scope and stability posture.

---

## Future improvements

- **Revisit `publish = false`** only when a crates.io (or other registry) policy is adopted; document the change in this file.
- **Automate** a minimal **release checklist** (bump versions, regen spec appendices).

*Revision: Git-only short/medium-term intent; `publish = false` on workspace crates.*
