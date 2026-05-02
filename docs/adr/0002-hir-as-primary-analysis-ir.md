# ADR 0002: HIR as the primary analysis IR

**Status:** Accepted  

**Date:** 2026-05-02  

**Deciders:** Maintainers (documentary backfill)

## Context

The toolchain must **resolve names**, **type-check**, and **interpret** LeekScript. Options include analyzing Rowan nodes directly, lowering to a dedicated IR, or multiple IRs.

## Decision

Lower the parsed program to **`HirFile`** / **`HirStmt`** / **`HirExpr`** in **`leekscript_hir`** and run **resolve**, **types**, and **interpret** on that IR. **`lek check`** and **`lek run`** share this lowering path.

## Consequences

### Positive

- Single structured IR for analysis and execution; easier to test and specify (see **`docs/spec/`** references to `HirExpr` / `HirStmt`).
- Clear boundary between **concrete syntax** (Rowan) and **semantic** work.

### Negative / trade-offs

- Lowering must stay correct and complete; spec and tests must track new HIR variants.
- Future bytecode/VM backends would lower from HIR (or replace interpreter), not from Rowan directly.

### Follow-ups

- Any **second IR** (bytecode) should be documented in a new ADR if introduced.

## Alternatives considered

1. **Analyze Rowan only** — rejected: too tied to parse noise for scope and runtime.
2. **Multiple parallel IRs** — deferred: unnecessary complexity today.

## References

- [rust-toolchain-crates.md](../architecture/rust-toolchain-crates.md)
- [embed-toolchain.md](../architecture/embed-toolchain.md)

---

*Backfill ADR; date reflects documentation landing, not first commit.*
