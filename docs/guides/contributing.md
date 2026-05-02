# Contributing

**Audience:** people opening pull requests against this workspace.

**Scope:** Expectations for code quality, tests, and diagnostic registry hygiene. **Non-goals:** social governance (use your team’s issue tracker / chat); this doc is technical.

## Before you push

1. **Format** — Rust code is formatted with **rustfmt** (edition 2021):

   ```bash
   cargo fmt --all
   ```

   Match existing Rust style in the touched crates; use **`cargo fmt`** and **`cargo clippy`** as the baseline.

2. **Clippy** (required for CI):

   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   ```

   If a lint is genuinely wrong for a line, prefer a **narrow** `#[allow(...)]` with a short comment over disabling clippy broadly.

3. **Tests** — at minimum, run tests for crates you touched:

   ```bash
   cargo test -p <crate>
   ```

   For wide changes, `cargo test` at the workspace root. Some tests need **JAVA_HOME** or submodules; see [environment.md](environment.md) and [local-development.md](local-development.md).

4. **Diagnostic registry** — if you add or rely on new **`reference`** / **`E####`** ids in the lexer, HIR, resolver, types, or interpreter, ensure they exist in **`data/diagnostics/registry.yaml`** (see [diagnostics-registry.md](../reference/diagnostics-registry.md)) and run:

   ```bash
   cargo run -p lek -- registry --verify-emit-refs
   ```

## LeekScript and HIR changes

- Prefer **tests** or **fixtures** that lock behavior (VM alignment is the default contract — see [project charter](../overview/project-charter.md)).
- Large semantic changes should eventually be reflected in **`docs/spec/`** once the language spec exists; until then, crate-level comments and tests are the source of truth.

## Documentation

- Update **`docs/`** when you add user-visible flags, env vars, or architectural boundaries.
- Keep **[docs/README.md](../README.md)** in sync if you add a new top-level doc page.
- **Spec chapters** (`docs/spec/`): follow **[00-conventions-and-notation.md](../spec/00-conventions-and-notation.md)** and update tests/registry when semantics change.
- **Deprecations:** for CLI flags, **`Leek.toml`**, or diagnostic ids, document the migration in PR description and relevant **`docs/reference/`** pages.

## Related

- [local-development.md](local-development.md)
- [rust-toolchain-crates.md](../architecture/rust-toolchain-crates.md)
- [`lek` CLI](../reference/lek-cli.md)

---

*Revision: initial contributing guide.*
