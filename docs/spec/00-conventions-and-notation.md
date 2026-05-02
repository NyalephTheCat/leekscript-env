# Conventions and notation

**Audience:** authors and readers of the **LeekScript language specification** (`docs/spec/`).

**Scope:** How normative language is written, how grammar fragments are formatted, and standard terms. **This chapter does not define LeekScript syntax** — only **meta-rules** for the rest of the spec.

## Requirement levels (RFC 2119)

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, **MAY** (and their lowercase forms in running prose when clearly used as keywords) are to be interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119.html) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174.html) (clarifying that these terms are **only** normative when capitalized in spec text, unless a section explicitly states otherwise).

- **MUST** / **REQUIRED** / **SHALL**: absolute requirement for conforming implementations.
- **MUST NOT** / **SHALL NOT**: absolute prohibition.
- **SHOULD** / **RECOMMENDED**: valid to omit only with good reason and full awareness of consequences.
- **SHOULD NOT** / **NOT RECOMMENDED**: valid only in rare circumstances.
- **MAY** / **OPTIONAL**: truly optional.

## Normative vs informative

- **Normative** sections define requirements for conforming implementations. They use RFC 2119 keywords as above.
- **Informative** sections (notes, rationale, examples marked *informative*) **do not** create conformance requirements. They **MAY** use “might”, “could”, or examples without RFC keywords.

Each major chapter **SHOULD** state at the top whether it is **normative** or **informative** (or split clearly into labeled subsections).

## Grammar notation

Unless a chapter says otherwise:

- **Productions** use the form **`symbol`** **`::=`** **`alternative₁`** **`|`** **`alternative₂`**.
- **Terminal tokens** appear in **`monospace`** or quoted literals (`"while"`, `";"`).
- **Nonterminals** use *Italic*`Name`* or `<Name>` consistently within a chapter.
- **`[ optional ]`**, **`{ repeated }`**, **`( grouping )`** follow common extended BNF habits; a consolidated sketch lives in **[appendices/A-grammar-summary.md](appendices/A-grammar-summary.md)**.
- **“one of”** sets may be written as **`one of`** `a` `b` `c` for readability.

## Metavariables

- **`N`**, **`k`**, **`n`**: integers in mathematical sense (bit width, counts).
- **`x`**, **`e`**, **`stmt`**: metavariables for expressions or statements in prose — not literal tokens.

## Implementation-defined vs unspecified

- **Implementation-defined:** behavior **MUST** be documented by the implementation (e.g. maximum source size if bounded). This workspace **SHOULD** record such choices in **[13-interpreter-behavior.md](13-interpreter-behavior.md)** or linked ops docs.
- **Unspecified:** behavior is not constrained; programs **MUST NOT** rely on it (e.g. order of iteration over hashes **unless** a later clause fixes **reference-implementation** parity).

## Diagnostics

Static and dynamic errors **SHOULD** eventually map to stable **`E####`** / **`reference`** ids (see [diagnostics-registry.md](../reference/diagnostics-registry.md) and **[appendix C](appendices/C-diagnostic-codes-mapping.md)**). Until every clause has a row, treat the registry and this spec’s clauses as **paired maintenance items**.

## Document process

Authoring workflow: follow the norms in this chapter, keep **`docs/spec/`** aligned with tests and **`data/diagnostics/registry.yaml`**, and regenerate appendices **C** / **F** with **`python3 scripts/gen_spec_appendices.py`** when registry or signature sources change.

---

*Revision: conventions chapter; BNF sketch in appendix A.*
