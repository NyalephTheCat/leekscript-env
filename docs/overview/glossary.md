# Glossary

**Audience:** readers of `docs/` who hit domain terms.

**Scope:** Short definitions used across the workspace. **Normative language behavior** will eventually live in **`docs/spec/`**; this page is a **navigational glossary**, not the language definition.

| Term | Meaning (in this workspace) |
|------|-----------------------------|
| **LeekScript** | The scripting language processed by `lek` and `leekscript_run` (lexical/syntax/HIR/interpret). **Normative spec** (in progress): [spec/README.md](../spec/README.md). |
| **`.leek`** | Source file extension for LeekScript programs. |
| **`lek`** | The main CLI crate: `init`, `config`, `registry`, `check`, `run`, `fmt`. |
| **`Leek.toml`** | Project manifest: language version, strictness, formatter options, optional `[generator]`, `[signatures]`, etc. Reference: **[leek-toml.md](../reference/leek-toml.md)**. |
| **Directive / preamble** | Leading `// leek-*` lines in a `.leek` file (max scanned lines match `PREAMBLE_MAX_LINES` in `leekscript_run`). Parsed by `leekscript_directives`. See **[directives.md](../reference/directives.md)**. |
| **HIR** | High-level IR: `leekscript_hir::HirFile` and related nodes — lowered from the concrete/Rowan syntax tree for resolve, types, and interpret. |
| **Rowan** | Library for green/red syntax trees; `leekscript_syntax` defines `LeekLanguage` and the parse output consumed by HIR lowering. |
| **Registry (`registry.yaml`)** | YAML listing diagnostic and reference ids (`E####`, lexer/interpreter string ids). Loaded via `leekscript_diagnostics`; path overridable with **`LEEK_REGISTRY`** (see [environment.md](../guides/environment.md)). |
| **`lek registry --verify-emit-refs`** | Check that every reference id emitted by the HIR/interpreter surface is present in the registry (CI-quality gate). |
| **`compile_source`** | `leekscript_run` API: directives → lexer → parse → HIR → resolve → typecheck (as implemented), returning `CompiledUnit` or diagnostics. |
| **`interpret_hir`** | Execute a `HirFile` with the in-tree interpreter (`leekscript_run::interp`). **Default semantics** aim to match the **game VM**; other entry points / flags may trade fidelity for limits, stats, traces, or throughput. |
| **Reference submodules** | `leek-wars/`, `leek-wars-generator/`, `ai/` — upstream **reference** trees; **not** Cargo dependencies of the core crates. Used for parity, data fetch, fuzz assets, etc. |
| **MSRV** | *Minimum supported Rust version.* **Unspecified** for now; see **[project-charter.md](project-charter.md)** and **[platforms-and-msrv.md](../operations/platforms-and-msrv.md)**. |
| **`leekgen`** | Binary from `leek_wars_gen`: scenario run/sim, fuzz drivers, compare, experiments, meta subcommands. |
| **Scenario** | Fight scenario description (JSON / TOML for `leekgen`). See **[scenario-format.md](../reference/scenario-format.md)**. |
| **Rust engine vs Java engine** | `leek_wars_gen` runs fights in-process by default (Rust + `leekscript_run`). The JVM generator is used for **comparison and validation**, not as the primary implementation path. |
| **`lw_meta`** | Crate / `lw-meta` CLI: Leek Wars HTTP API client (e.g. rankings), typed exports; used by `leekgen meta …`. |
| **`LEEK_GENERATOR_CWD`** | Filesystem root for resolving generator-relative paths (AI assets, `generator.jar` discovery). |
| **`LEEKGEN_REPRO_DIR`** | Directory for fuzz repro artifacts (scenario + meta) when minimizing parity issues. |
| **Parity** | Comparing Rust vs JVM fight outcomes under defined modes — see **[correctness-and-parity.md](../architecture/correctness-and-parity.md)** and `leek_wars_gen::parity`. |
| **Experiment (subsystem)** | `leek_wars_gen::experiment`: TOML specs, sweeps, caching, optimization hooks — see crate `lib.rs` module list. |
| **LSP** | *Planned:* a **language server** for LeekScript in this ecosystem (see [project-charter.md](project-charter.md)); not a shipped product of upstream generator. |

## Future improvements

- Link each term to the **spec chapter** or **crate doc** once `docs/spec/README.md` and per-crate READMEs exist.
- Add **abbreviation table** (SARIF, etc.) when those topics get real docs.

---

*Revision: restored table; cross-links to environment and operations docs.*
