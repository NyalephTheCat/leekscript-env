# Embedding the LeekScript toolchain (library use)

**Audience:** Rust crate authors calling **`leekscript_run`** (or siblings) without the **`lek`** binary.

**Scope:** Public entry points, **stability expectations**, **threading**, and **feature flags** as they exist today. **Non-goals:** a stable C ABI or dynamic loading (future only).

## Primary crate: `leekscript_run`

The **`lek`** CLI is a thin wrapper; libraries should depend on **`leekscript_run`** for compile + interpret.

### Compile pipeline

| Item | Role |
|------|------|
| **`compile_source`**, **`CompileOptions`**, **`CompileOutcome`**, **`CompiledUnit`**, **`CompileDiagnostic`**, **`CompilePhase`** | Parse through typecheck; success yields **`HirFile`** + Rowan root + metadata. |
| **`resolve_include_file`**, **`ModuleExpansionCache`**, **`ExpandedSourceUnit`** | `include("…")` expansion and caching. |
| **`compile_signature_leek`**, **`parse_sig_leek`**, **`SigLeekUnit`** | Signature / `.sig.leek` helpers for global declarations. |
| **`sig_workspace`** | Bundled / merged workspace signatures (`CORE_SIG_LEEK`, `LEEKWARS_SIG_LEEK`, merge helpers). |

### Resolution (standalone)

| Item | Role |
|------|------|
| **`resolve_hir`**, **`resolve_hir_with_extra_globals`**, **`ResolveDiagnostic`** | Name resolution on an existing **`HirFile`** when not using the full **`compile_source`** wrapper. |

### Interpretation

| Item | Role |
|------|------|
| **`interpret_hir`**, **`interpret_hir_with_host`**, **`interpret_hir_with_strict`**, **`interpret_hir_with_limits_and_stats`** | Tree-walk interpreter; optional **`InterpreterHost`** for native hooks (**`call_native`**, debug logs). |
| **`InterpretSession`** | Persistent session after top-level init (Leek Wars AI–style multi-turn); see **`from_hir_init`**, **`from_hir_leek_wars_ai_*`**, **`run_leek_wars_turn_stmts`** in `interp/mod.rs`. |
| **`Value`**, **`InterpretError`**, **`ExecAbort`**, **`InterpretStats`** | Runtime model and errors. |
| **`INTERP_EMITTED_REFERENCES`**, **`interpret_reference_display_message`** | Stable **`reference`** strings for errors (registry / CLI copy). |

### HIR and helpers

**`HirFile`**, **`HirStmt`**, **`HirExpr`**, … are re-exported from **`leekscript_hir`**. Lexer display helpers: **`lexer_reference_display_message`**.

### `InterpreterHost`

Implement **`InterpreterHost`** to satisfy Leek Wars–style **natives** (`getLife`, …) and optional **`debug*`** routing. The object is passed as **`Option<Box<dyn InterpreterHost>>`** — **object-safe, mutable per call**.

## Cargo features (`leekscript_run`)

**There are no `[features]`** on **`leekscript_run`** today (`crates/leekscript_run/Cargo.toml`). The crate always pulls the full pipeline dependencies listed in that manifest.

Other workspace crates may define features later; re-check their **`Cargo.toml`** when embedding them directly.

## Threading and re-entrancy

- **Runtime values** use **`Rc`** / **`RefCell`** internally (e.g. arrays, maps, instances). **`Value`** and **`InterpretSession`** are **not `Send`** and are **not meant for cross-thread sharing**.
- **Practical pattern:** one thread (or one **exclusive** owner) per **`InterpretSession`**; run **`compile_source`** on any thread but treat each **`CompiledUnit`** / session as **single-threaded** afterward unless you prove no shared `Rc` escapes.
- **Re-entrancy:** do not assume **`InterpreterHost`** is called from a single stack frame only; host implementations should avoid deadlocking with interpreter locks they do not control.

## Stability

- Treat **`leekscript_run`’s `pub use` surface** as the **intended** embedding API. **`interp::`**, **`pipeline::`** modules are **not** public; avoid relying on `pub(crate)` internals.
- **Semver:** workspace crates are **0.x** / evolving; pin **git revisions** or **path** versions for production embedders until a semver story is published.
- **Diagnostics:** use **`CompileDiagnostic`** / **`InterpretError`** **`reference`** fields with **`data/diagnostics/registry.yaml`** for stable **`E####`** mapping in tooling.

## Errors and UI

- **Compile:** phase (`CompilePhase`), **`reference`**, **`span`**, **`message`**; optional **`snippet_origin`** for includes.
- **Run:** **`InterpretError`** with **`reference`** and message; map via registry + **`interpret_reference_display_message`** where needed.

## Future: FFI / dynamic loading

No stable **C ABI** or **dynamic library** embedding is defined. If added, expect a dedicated ADR and a small **foreign-facing** surface distinct from internal `HIR` types.

## Related

- [Library usage (guides entry)](../guides/library-usage.md) — short pointer to this page for onboarding paths.
- [rust-toolchain-crates.md](rust-toolchain-crates.md)
- [directives.md](../reference/directives.md), [leek-toml.md](../reference/leek-toml.md)
- [spec README](../spec/README.md) (language semantics, when drafted)

---

*Revision: aligned with `leekscript_run` `lib.rs` exports; update if features or `Send` bounds change.*
