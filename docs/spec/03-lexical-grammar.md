# Lexical grammar

**Normative** unless marked *informative*. Token spellings **MUST** match the workspace **lexical analyzer** unless an **implementation note (this repository)** says otherwise.

## Comments

- **`//` line comment** — From `//` to end of line (see [02-unicode-and-source-text.md](02-unicode-and-source-text.md)).
- **Block comment** — `/*` … `*/`, non-nesting (C-family style, as in the **reference implementation**).

Block comments **MAY** appear between tokens. They **MUST NOT** split a token (e.g. inside an identifier).

## Whitespace

Whitespace separates tokens where required by the grammar. Space, tab, and line terminators are whitespace.

## Identifiers

A **general identifier** is a maximal sequence of characters allowed by the lexer for `Ident` (letters, digits, `_`, and other classes as implemented — aligned with the **reference** lexical analyzer for parity).

### Keyword and word-operator classification

When the lexer classifies a word:

- For language version **`v ≤ 2`**, many reserved spellings use **ASCII case-insensitive** comparison against the expected keyword string (the **`word_eq`** helper in the keyword classifier).
- For **`v ≥ 3`**, classification uses **exact** string match for most reserved words.
- The token **`class`** is special: for **`v ≥ 2`**, only the exact spelling **`class`** (lowercase) is the class keyword; other casings are **identifiers** (**reference** parity).

Word forms that are **operators** in the **reference implementation** are **not** plain identifiers:

- **`and`**, **`or`**, **`xor`** → word-operators (logical / bitwise, per version rules).
- **`instanceof`** → word-operator when **`v ≥ 2`**.
- **`is`** → word-operator (historical test-suite equality sugar); composition **`is not`** is handled at parse level.

## Literals

### Numbers

- **Integer** — Decimal digits; no `.` or exponent in the **integer** token form (details in lexer).
- **Real** — Floating-point forms including exponent notation; also special literals **`∞`** (lemniscate) and **`π`** where the lexer accepts them.

### Strings

String literals use **single** or **double** quotes. Escape sequences and **unclosed** string errors **MUST** match lexer diagnostics (e.g. **`STRING_NOT_CLOSED`**, **`INVALID_CHAR`**).

### Booleans and null

**`true`**, **`false`**, **`null`** are keywords (with **`word_eq`** rules by version).

## Punctuation and operators

The lexer emits dedicated kinds for parentheses, brackets, braces, **`.`**, **`;`**, **`,`**, **`=>`**, and a general **`Operator`** class for symbolic operators (`+`, `-`, `**`, `===`, …). Exact spellings **MUST** match the lexer’s token enumeration and the parser’s expectation.

## Tokenization order

*Informative:* Longest-match and operator ordering follow this workspace’s lexer, which tracks the **reference** **`LexicalParser`** ordering. When adding tokens, maintain parity tests (see [appendices/E-conformance-tests-index.md](appendices/E-conformance-tests-index.md)).

## Reserved words list

A consolidated keyword table by language version is in [appendices/B-reserved-and-future-keywords.md](appendices/B-reserved-and-future-keywords.md).

### Implementation note (this repository) — reserved words without full syntax

Some tokens are reserved in the lexer for **`v ≥ 3`** (e.g. **`import`**, **`export`**, **`await`**) but may not yet have full statement forms in the **syntax analyzer**. Using them as keywords where the grammar has no production **MAY** still fail at parse time. Treat **parser** and **HIR** support as the effective language surface until a chapter normatively defines a construct.

---

*Revision: lexical chapter; expand literal regexes when extracted to machine-readable grammar.*
