# Rust toolchain: crates and pipeline

**Audience:** contributors implementing or reviewing the compiler/formatter/interpreter path, and auditors mapping **implementation to intended behavior**.

**Scope:** Workspace crate roles and the **`lek check` / `lek run`** compile pipeline as implemented in `leekscript_run`. **Non-goals:** full semantic specification (planned `docs/spec/`), full generator/fight architecture (see `leek_wars_gen` crate docs and future `docs/architecture/generator-and-engine.md`).

## Workspace package metadata

- **Edition:** Rust **2021** (`[workspace.package]` in root `Cargo.toml`).
- **License:** MIT (workspace default).

## Layered view (summary)

| Layer | Crates (representative) |
|-------|-------------------------|
| **CLI** | `lek` |
| **Orchestration** | `leekscript_run` (`compile_source`, `interpret_hir`, pipeline phases) |
| **Analysis** | `leekscript_resolve`, `leekscript_types`, `leekscript_signatures` |
| **IR** | `leekscript_hir` |
| **Surface** | `leekscript_lexer`, `leekscript_parser`, `leekscript_syntax` |
| **Cross-cutting** | `leekscript_span`, `leekscript_diagnostics`, `leekscript_directives`, `leekscript_config` |
| **Format** | `leekscript_fmt` |

Simulation and meta tooling (`leek_wars_gen`, `lw_meta`) sit **beside** this pipeline and **embed** `leekscript_run` for executing AI scripts in fight simulation. See **[generator-and-engine.md](generator-and-engine.md)** for engines, scenarios, and `leekgen`. The charter covers **VM-faithful** defaults vs batch tooling—**[project charter](../overview/project-charter.md)**.

## End-to-end data flow (`compile_source`)

The pipeline phases exposed as `CompilePhase` in `leekscript_run` align with how diagnostics are attributed:

1. **Directives** — `leekscript_directives::parse_file_preamble` on the leading lines of the file (see [reference/directives.md](../reference/directives.md)).
2. **Lexer** — `leekscript_lexer::Lexer` (ordering compatible with Java `LexicalParser` — see future ADR/spec notes).
3. **Parser** — `leekscript_parser::parse_file_green` producing a Rowan syntax tree (`leekscript_syntax::LeekLanguage`).
4. **HIR** — `leekscript_hir::lower_file` lowers the tree to `HirFile`.
5. **Resolve** — `leekscript_resolve::resolve_hir_with_extra_globals` (and related); optional extra globals from signature TOML / CLI.
6. **Types** — `leekscript_types::check_hir_types`.

`lek check` stops after a successful compile (no execution). `lek run` invokes **`interpret_hir`** (and related entry points) on the resulting `HirFile`.

### Interpreter: game VM alignment vs extended modes

**Default:** execution is intended to match **in-game Leek Wars VM behavior** (the semantic baseline for correctness). Tests and parity tooling should treat that as the ordinary contract.

**Opt-in divergences:** the interpreter and CLI may expose **flags, manifest options, or API entry points** (e.g. limits/stats, strictness, host hooks) that deliberately **differ** from the VM in exchange for faster iteration, richer diagnostics, traces, or batch throughput. Such modes should be **explicit** (documented in help text and eventually in `docs/spec/`) so users know when they are not getting a strict VM replay.

### Configuration and includes

- **`CompileOptions`** carries manifest path, CLI overrides for language version and strict mode, paths for **`include("…")`** resolution, snippet origins for diagnostics, and **`signature_globals`** from signature TOML.
- **`ModuleExpansionCache`** deduplicates expanded includes within a compilation; comments in `pipeline.rs` note a future `import` mechanism reusing the same cache.

### Public API surface (library consumers)

`leekscript_run` re-exports the main types and functions for embedders:

- **Compile:** `compile_source`, `CompileOutcome`, `CompiledUnit`, `CompileDiagnostic`, `CompileOptions`, include resolution helpers, signature helpers (`sig_workspace`).
- **Execute:** `interpret_hir`, `interpret_hir_with_host`, limit/stats/strict variants, `InterpretError`, `Value`, host traits.

For library embedding (API surface, threading, stability), see **[embed-toolchain.md](embed-toolchain.md)**.

**Change process:** after touching lexer/HIR/interpreter reference ids, run `lek registry --verify-emit-refs` (see [contributing.md](../guides/contributing.md)).

## Crate-by-crate responsibilities (concise)

| Crate | Responsibility |
|-------|----------------|
| `leekscript_span` | Byte spans, positions for diagnostics. |
| `leekscript_diagnostics` | Load and represent registry-backed diagnostics. |
| `leekscript_config` | Find/load `Leek.toml`, validation. |
| `leekscript_directives` | Preamble directives, fmt hints. |
| `leekscript_lexer` | Token stream. |
| `leekscript_parser` | Grammar parse to green tree. |
| `leekscript_syntax` | Language kind, AST-facing shims. |
| `leekscript_fmt` | Formatting (`lek fmt`). |
| `leekscript_hir` | Lowered IR. |
| `leekscript_resolve` | Scope/name resolution. |
| `leekscript_signatures` | External signature TOML for globals/functions (e.g. Leek Wars AI stubs). |
| `leekscript_types` | Type checking pass. |
| `leekscript_run` | Pipeline + interpreter. |
| `lek` | User-facing CLI wrapping the above. |

## Related binaries and research crates

- **`leekscript_fuzz`**: fuzzing helpers for the toolchain (relationship to repo-root **`fuzz/`** to be documented under a dedicated fuzzing page).
- **`leekscript_bench`**: benchmarks — crate **[`leekscript_bench`](../../crates/leekscript_bench)**.

## Design and maintenance notes

### Strengths of current shape

- **Phase-tagged diagnostics** (`CompilePhase`) make it easy to attribute failures to lexer vs parser vs HIR vs resolve vs types — good for UX and CI JSON consumers.
- **HIR as single analysis IR** simplifies `resolve` + `types` + `interpret` sharing one structure.

### Possible future improvements (non-committal)

- **Incremental compilation:** pipeline today is per-invocation; an on-disk cache keyed by file hash would need explicit invalidation rules (directives, manifest, includes).
- **Parser–formatter convergence:** `lek` long-about notes formatter is **token-based** with “full parser later” — document formatter guarantees vs parse pipeline to avoid drift.
- **Stable embedding API:** consider a narrow `lek_api` or feature-gated surface if `leekscript_run` internals churn.
- **Spec linkage:** each `CompilePhase` should eventually map to normative clauses in `docs/spec/` and to tests ([appendix E — conformance tests index](../spec/appendices/E-conformance-tests-index.md)).

---

*Revision: added interpreter default (game VM) vs opt-in extended modes; link to charter for product goals.*

