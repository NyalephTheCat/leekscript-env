# Platforms and MSRV

**Audience:** integrators and CI authors who need to know what Rust and OS targets are expected.

**Scope:** Current **practical** requirements. **Non-goals:** promising support for every tier-3 target (not asserted here).

## Rust toolchain

| Topic | Status |
|-------|--------|
| **Edition** | **2021** (`[workspace.package]` in root `Cargo.toml`). |
| **MSRV (minimum supported Rust version)** | **Unspecified** — maintainers have not pinned a minimum compiler version. Expect **current stable** to work; older versions may happen to compile but are not guaranteed. |
| **Pinned toolchain file** | There is **no** root `rust-toolchain.toml` today. One may be added when MSRV is chosen. |

When this project adopts an MSRV, update this page and consider **`rust-toolchain.toml`** plus a note in the [project charter](../overview/project-charter.md). **Broader versioning** (crate semver, spec label): [release-and-versioning.md](release-and-versioning.md).

## Operating systems and architectures

Development is routinely done on **Linux** (e.g. **x86_64-unknown-linux-gnu**). **macOS** and **Windows** are not explicitly excluded, but **CI coverage at the repo root** may be limited — see [CI and quality gates](ci-and-quality-gates.md).

Linker flags in **`.cargo/config.toml`** are written for **GNU ld (BFD)** on Linux; other platforms may need local overrides (see [environment guide](../guides/environment.md)).

## Nightly

**Fuzzing** (`fuzz/` directory, `cargo-fuzz`) requires **nightly** Rust. Normal `cargo build` / `cargo test` for workspace members use **stable**.

---

*Revision: initial platforms/MSRV page; charter is normative for “MSRV unspecified”.*
