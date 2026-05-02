# Directives and pragmas

**Normative** for **`// leek-*`** preambles as consumed by the **compilation pipeline**.

## Placement

Directives **MUST** appear in the **file preamble**: leading lines that are blank, line comments, or block comments. The scan **MUST** stop at the first **non-comment, non-blank** source line. At most **`PREAMBLE_MAX_LINES`** (**64**) lines **MUST** be scanned.

Directives **after** real code **MUST** be **ignored** (treated as ordinary comments).

## Syntax

**`// leek-<name>[:|=] <value>`**

- Whitespace **MAY** precede **`//`** on the line.
- **`<name>`** is a directive name; unknown names **MUST** be diagnosed (**`unknown_leek_directive`**) except for names explicitly listed as ignored.
- **`<value>`** depends on the directive; invalid values **MUST** use **`leek_directive_invalid_value`**.

## Normative directives

| Directive | Semantics |
|-----------|-----------|
| **`leek-version`** | Integer **1–99**. Sets this file’s requested **language version** when the pipeline applies preamble (CLI / manifest may override per precedence in **`lek`** / compile options). |
| **`leek-strict`** | Boolean-ish value or omitted (means **true**). Sets **strict** interpretation for this unit when honored. |
| **`leek-fmt`** | Formatter hints (**`key=value`**, comma-separated). **MUST NOT** change lexical tokenization of the program; affects **`lek fmt`** only. |
| **`leek-experimental`** | Comma-separated feature labels. **MUST** parse and attach to compile metadata; **MUST NOT** alone enable unsafe execution. |

## Ignored names (forward compatible)

The names **`allow`**, **`push`**, **`pop`** **MUST** be accepted and **silently ignored** (no diagnostic).

## Interaction with lexer/parser

The preamble is interpreted **before** the main lexer pass on the concatenated source. Directives **MUST NOT** use a different character encoding than the rest of the file.

## Operational reference

For copy-paste examples and diagnostic tables, see [directives.md](../reference/directives.md).

---

*Revision: directives chapter; keep in sync with the directive parser implementation.*
