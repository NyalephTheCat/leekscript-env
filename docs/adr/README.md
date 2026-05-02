# Architecture Decision Records (ADRs)

**Audience:** maintainers recording **significant, stable** technical choices.

**Scope:** **This repository** only. **Out of scope:** duplicating GitHub issue discussion—link issues from the ADR.

## When to write an ADR

Write one when a decision is **hard to reverse** or **affects many crates**, for example:

- Choice of IR (HIR), syntax tree technology, or lexer parity strategy.
- Public API shape for `leekscript_run` or stable CLI contracts.
- Diagnostic / registry stability policy, distribution of `data/`.

Skip ADRs for routine refactors, small bug fixes, or experiments that may be reverted without policy impact.

## Naming and location

- One file per decision: **`NNNN-short-title.md`** (four-digit sequence, zero-padded), e.g. **`0001-use-rowan-for-syntax.md`**.
- Use **[0001-rowan-for-concrete-syntax.md](0001-rowan-for-concrete-syntax.md)** as a structural example when adding **`0003-…`**.

## Index (accepted backfill)

| ADR | Title |
|-----|-------|
| [0001](0001-rowan-for-concrete-syntax.md) | Rowan for concrete syntax |
| [0002](0002-hir-as-primary-analysis-ir.md) | HIR as primary analysis IR |

## Lifecycle

| State | Meaning |
|-------|---------|
| **Proposed** | Under discussion; may change. |
| **Accepted** | Team agrees; implement accordingly. |
| **Superseded** | Replaced by a newer ADR (link both ways). |
| **Deprecated** | No longer recommended; history only. |

## Related

- [Documentation index](../README.md)

---

*Revision: ADR process bootstrap; backfill numbered ADRs as needed.*
