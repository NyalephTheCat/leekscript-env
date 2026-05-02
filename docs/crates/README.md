# Workspace crates (paths)

**Audience:** jumping from a **crate name** to its **source tree** under `crates/`.

Narrative roles and pipeline order: **[rust-toolchain-crates.md](../architecture/rust-toolchain-crates.md)**.

| Crate | Path |
|-------|------|
| `lek` | [`crates/lek`](../../crates/lek) |
| `leekscript_bench` | [`crates/leekscript_bench`](../../crates/leekscript_bench) |
| `leekscript_config` | [`crates/leekscript_config`](../../crates/leekscript_config) |
| `leekscript_diagnostics` | [`crates/leekscript_diagnostics`](../../crates/leekscript_diagnostics) |
| `leekscript_directives` | [`crates/leekscript_directives`](../../crates/leekscript_directives) |
| `leekscript_fmt` | [`crates/leekscript_fmt`](../../crates/leekscript_fmt) |
| `leekscript_fuzz` | [`crates/leekscript_fuzz`](../../crates/leekscript_fuzz) |
| `leekscript_hir` | [`crates/leekscript_hir`](../../crates/leekscript_hir) |
| `leekscript_lexer` | [`crates/leekscript_lexer`](../../crates/leekscript_lexer) |
| `leekscript_parser` | [`crates/leekscript_parser`](../../crates/leekscript_parser) |
| `leekscript_resolve` | [`crates/leekscript_resolve`](../../crates/leekscript_resolve) |
| `leekscript_run` | [`crates/leekscript_run`](../../crates/leekscript_run) |
| `leekscript_signatures` | [`crates/leekscript_signatures`](../../crates/leekscript_signatures) |
| `leekscript_span` | [`crates/leekscript_span`](../../crates/leekscript_span) |
| `leekscript_syntax` | [`crates/leekscript_syntax`](../../crates/leekscript_syntax) |
| `leekscript_types` | [`crates/leekscript_types`](../../crates/leekscript_types) |
| `leek_wars_gen` | [`crates/leek_wars_gen`](../../crates/leek_wars_gen) |
| `lw_meta` | [`crates/lw_meta`](../../crates/lw_meta) |

Module-level API notes are usually in each crate’s `src/lib.rs` (or `main.rs` for binaries).

---

*Revision: path table only; dropped per-crate markdown duplicates under `docs/crates/`.*
