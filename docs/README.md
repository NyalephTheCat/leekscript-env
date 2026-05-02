# LeekScript workspace documentation

**Audience:** contributors, integrators, and reviewers who need to navigate this repository without reading every crate first.

**Scope:** This tree describes the **Rust workspace** (`lek`, libraries, generator, meta tooling). **`leek-wars/`** and **`leek-wars-generator/`** submodules are **reference-only** (parity, data, repros)—see the [charter](overview/project-charter.md). Upstream READMEs remain authoritative for **product** documentation about the game and Java generator themselves.

## How to navigate

| Tier | Start here |
|------|------------|
| **Onboard** | [Project charter](overview/project-charter.md), [repository map](overview/repository-map.md), [External submodules](overview/external-repositories.md), [glossary](overview/glossary.md) |
| **Daily dev** | [Local development](guides/local-development.md), [Environment](guides/environment.md), [Contributing](guides/contributing.md), [Library usage](guides/library-usage.md), [Release process](guides/release-process.md) |
| **Toolchain** | [Rust crates & pipeline](architecture/rust-toolchain-crates.md), [Embed `leekscript_run`](architecture/embed-toolchain.md), [Data & fixtures](architecture/data-and-fixtures.md) |
| **Generator / `leekgen`** | [Generator & engine](architecture/generator-and-engine.md), [Experiments & trace](architecture/leek-wars-gen-experiments-and-trace.md), [Parity & correctness](architecture/correctness-and-parity.md), [Fuzzing](architecture/fuzzing.md), [Scenario format](reference/scenario-format.md) |
| **Reference (tooling)** | [`lek` CLI](reference/lek-cli.md), [Leek.toml](reference/leek-toml.md), [Preamble directives](reference/directives.md), [Diagnostics registry](reference/diagnostics-registry.md), [Registry operations](reference/registry.md) |
| **Operations** | [Platforms & MSRV](operations/platforms-and-msrv.md), [CI & quality gates](operations/ci-and-quality-gates.md), [Release & versioning](operations/release-and-versioning.md) |
| **Engineering** | [Observability](engineering/observability.md), [Error messages & UX](engineering/error-messages-and-ux.md), [Performance](engineering/performance.md) |
| **ADRs** | [Architecture decisions](adr/README.md) |
| **Quick commands** | Root [README.md](../README.md) |
| **Language spec** | [Spec index](spec/README.md), [Ch.0 conventions](spec/00-conventions-and-notation.md) |
| **Crate paths** | [crates/README.md](crates/README.md) — workspace members under `crates/`. |

## Current layout

```text
docs/
  README.md
  overview/
    project-charter.md
    repository-map.md
    external-repositories.md
    glossary.md
  guides/
    contributing.md
    local-development.md
    environment.md
    library-usage.md
    release-process.md
  architecture/
    rust-toolchain-crates.md
    embed-toolchain.md
    data-and-fixtures.md
    generator-and-engine.md
    correctness-and-parity.md
    fuzzing.md
    leek-wars-gen-experiments-and-trace.md
  spec/
    README.md
    00-conventions-and-notation.md
    …
  engineering/
    observability.md
    error-messages-and-ux.md
    performance.md
  adr/
    README.md
    0001-rowan-for-concrete-syntax.md
    0002-hir-as-primary-analysis-ir.md
  reference/
    lek-cli.md
    leek-toml.md
    directives.md
    diagnostics-registry.md
    registry.md
    scenario-format.md
  operations/
    release-and-versioning.md
    platforms-and-msrv.md
    ci-and-quality-gates.md
  crates/
    README.md
```

**`docs/spec/`** chapters **00–13** and appendices **A–C**, **E**, **F** are present; appendices **C** and **F** are regenerated from `data/` via `python3 scripts/gen_spec_appendices.py`.

## Future documentation improvements

1. **CI:** [ci-and-quality-gates.md](operations/ci-and-quality-gates.md) documents workflows; extend matrix / pedantic job when ready.
2. **Tracing:** evolve [observability](engineering/observability.md) if `RUST_LOG` / `tracing` land in `lek`.

---

*Revision: trimmed assurance/security/roadmaps/process layers and duplicate crate write-ups.*
