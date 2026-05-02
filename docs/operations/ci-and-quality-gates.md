# CI and quality gates

**Audience:** maintainers and contributors defining or satisfying merge requirements.

**Scope:** **GitHub Actions** at the workspace root and **local** equivalents. **Non-goals:** submodule workflows (`leek-wars`, `leek-wars-generator` have their own CI).

## GitHub Actions (this repo)

| Workflow | File | When | What |
|----------|------|------|------|
| **CI** | [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) | Push & pull request | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `lek registry --verify-emit-refs` |
| **Dependencies** | [`.github/workflows/dependencies.yml`](../../.github/workflows/dependencies.yml) | Push/PR touching lockfile or `deny.toml`, **weekly** schedule, **manual** | **`cargo-deny`** **0.19.4** (`cargo deny check`), **`rustsec/audit-check`** |

Dependency jobs also run when **`Cargo.toml`** / **`Cargo.lock`** / **`deny.toml`** change. Adjust path filters in the workflow if you add more manifests.

## Local gates (before push)

| Gate | Command | Notes |
|------|---------|-------|
| Format | `cargo fmt --all -- --check` | Or format and commit. |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | Stricter **pedantic** linting is a **future goal**. |
| Tests | `cargo test` | Or scoped `-p` for faster iteration. |
| Registry coverage | `cargo run -p lek -- registry --verify-emit-refs` | Catches missing **`E####` / reference** ids. |
| Dependencies | `cargo deny check` | Needs `cargo install cargo-deny` (use **0.19.4+** for current RustSec DB). |
| Audit | `cargo audit` | Optional; CI runs `audit-check` as well. |

## `deny.toml`

Workspace **[`deny.toml`](../../deny.toml)** configures **`cargo-deny`**: allowed **SPDX** licenses, **duplicate** crate warnings, **advisory** policy. One advisory may be **ignored** with documented reason until a dependency upgrade clears it—see file comments.

## Future improvements

- **Matrix** builds (Windows, macOS) once support is committed.
- **Clippy pedantic / nursery** as an optional or staged job.
- **`cargo vet`** if the project adopts supply-chain signing.

---

*Revision: root CI + dependency workflows; deny.toml wired; pedantic noted as future.*
