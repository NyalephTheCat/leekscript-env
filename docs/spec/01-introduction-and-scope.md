# Introduction and scope

**Normative** except where marked *informative*.

## Purpose

This multi-chapter specification defines the **LeekScript** language as implemented and exercised in this repository: **lexical structure**, **syntax** (as reflected in the HIR), **static checks** that exist today, and **dynamic semantics** of the **tree interpreter**.

## Design goals

1. **Game VM alignment** — The default execution model in this workspace **SHOULD** match observable behavior of the **Leek Wars** in-game language (**reference implementation**). See [project charter](../overview/project-charter.md) and [correctness-and-parity.md](../architecture/correctness-and-parity.md).
2. **Auditable surface** — Syntax and semantics **MUST** be traceable to **HIR** node definitions, **lexer** token kinds, and **interpreter** behavior as defined in this repository.
3. **Explicit divergence** — Where **this implementation** differs from the **reference implementation**, this spec **MUST** call that out in an **implementation note (this repository)** (see below).

## In scope

- Source text, tokens, and parsing into the statement/expression IR used for analysis and interpretation.
- Name resolution rules consistent with the workspace resolve pass.
- Runtime values, operators, control flow, functions, classes, collections, and the **global builtin** namespace described by bundled registries and the interpreter.
- Compile pipeline phases: directives, lexer, parser, HIR, resolve, minimal type checks, interpretation.
- Resource limits and **strict** mode flags exposed by the interpreter API and CLI.

## Out of scope

- **Leek Wars** server, client, or matchmaking protocols.
- **Scenario / fight engine** formats, except where a LeekScript program observes host-provided values through defined APIs.
- Full formal verification or a complete static type system (the workspace currently performs **limited** compile-time typing; see [06-types-and-subtyping.md](06-types-and-subtyping.md)).

## Reference implementation and this specification

- **Reference implementation** — The canonical game-side language (lexer, values, builtins) is the **parity target** for behavior. This spec cites that behavior *informatively* only where needed; historical labels in comments or test names may refer to that lineage without binding this text to any one source tree.
- **This implementation** — The **normative technical baseline** for this document is the behavior of the **tooling and interpreter shipped in this repository** (see architecture documentation for how packages compose). If reference behavior is unclear or untested here, **observable behavior of this implementation** is authoritative for this repository until a spec change and tests close the gap.

### Implementation notes (this repository)

Throughout later chapters, **boxed notes** describe behavior that is **specific to this repository** or **known to differ** from the **reference implementation** (for example postfix `++`/`--` value in the tree interpreter). Implementations **MAY** converge over time; the notes capture the current contract for readers of this workspace.

## Language version

Programs are compiled with a **language version** **`v`** (integer, typically **1–99**), from manifest / CLI / API. **Lexical** classification of some words depends on **`v`** (see [03-lexical-grammar.md](03-lexical-grammar.md)). Features gated by **`v`** **MUST** be documented in the chapter that defines them.

## Conformance classes (*informative*)

Implementations **MAY** claim partial conformance:

| Class | Meaning |
|-------|--------|
| **Parse-only** | Lex + parse + HIR lowering; no execution. |
| **Resolve** | Parse + resolve; reports undefined names / duplicates. |
| **Interpret** | Full pipeline through interpretation with builtins and limits as documented. |

---

*Revision: terminology aligned with reference vs workspace implementation.*
