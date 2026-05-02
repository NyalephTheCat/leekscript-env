# ADR 0001: Rowan for concrete syntax

**Status:** Accepted  

**Date:** 2026-05-02  

**Deciders:** Maintainers (documentary backfill)

## Context

LeekScript needs a **lossless** concrete syntax representation for parsing, incremental editing, and a future **parser-backed** formatter. Alternatives include hand-rolled ASTs, `syn`-style derives, or other green-tree libraries.

## Decision

Use **[Rowan](https://github.com/rust-analyzer/rowan)** **red–green** trees for the concrete syntax layer in **`leekscript_syntax`**, with a typed facade over the root (`ast::SourceFile`, …).

## Consequences

### Positive

- Industry-proven model (rust-analyzer ecosystem), good fit for trivia and incremental rework.
- Aligns with long-term **LSP / IDE** direction if the project adds editor integration later.

### Negative / trade-offs

- Learning curve for contributors new to Rowan APIs.
- Formatter today remains **token-based** (`leekscript_fmt`) until it fully converges on the grammar tree.

### Follow-ups

- Document Rowan-specific invariants in **`leekscript_syntax`** as crate docs deepen.

## Alternatives considered

1. **Custom AST only** — rejected: harder to preserve trivia and evolve toward IDE features.
2. **`syn`** — not a fit: Rust-oriented, not LeekScript.

## References

- [rust-toolchain-crates.md](../architecture/rust-toolchain-crates.md)
- Future ADR candidate: “Rowan vs other syntax approaches”

---

*Backfill ADR; date reflects documentation landing, not first commit.*
