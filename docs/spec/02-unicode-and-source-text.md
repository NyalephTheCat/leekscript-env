# Unicode and source text

**Normative** unless marked *informative*.

## Character repertoire

Source files **MUST** be interpreted as **UTF-8** encoded text (8-bit bytes). Invalid UTF-8 **MUST** be rejected before tokenization with an implementation-defined diagnostic.

*Informative:* Tooling uses byte offsets into UTF-8 for source spans.

## Normalization

This specification does **not** require Unicode normalization of source text. **Identifier equality** follows the rules in [03-lexical-grammar.md](03-lexical-grammar.md) (ASCII-focused keyword rules; general identifiers are compared as in the implementation).

## Line terminators

The lexer **MUST** accept:

- **U+000A** LINE FEED (`\n`),
- **U+000D** CARRIAGE RETURN (`\r`),
- **U+000D U+000A** CRLF as a single line break for line-counting purposes.

**Line comment** (`//`) runs from `//` through the end of the **physical line** (before a line terminator or end of file).

## End of file

**`Eof`** is a distinct token kind after the last meaningful input. No token **MAY** span past the end of the byte sequence.

## Maximum source size

Maximum source length (bytes or tokens) is **implementation-defined**. This workspace does not publish a hard upper bound in the spec; hosts **SHOULD** document practical limits.

## Files and includes

A **compilation unit** may be expanded from a root `.leek` file plus **`include("path")`** text inclusion when a **source path** is provided for resolution (see [10-functions-and-call-conventions.md](10-functions-and-call-conventions.md) and [13-interpreter-behavior.md](13-interpreter-behavior.md)). Included text is concatenated at the statement level before parse; spans may record **per-statement** origin paths on the HIR file metadata.

---

*Revision: initial chapter.*
