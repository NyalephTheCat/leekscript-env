# LeekScript language specification

This document specifies the LeekScript surface language as implemented by the reference compiler in [`leek-wars-generator/leekscript`](https://github.com/leek-wars/leek-wars-generator) (Java). A Rust implementation should match observable behavior of that implementation for the same `(language version, strict mode)` pair unless an **experimental** flag or **source directive** explicitly opts into different rules (see [§11 Toolchain directives](#11-toolchain-directives)).

**Normative reference (Java packages):**

| Area | Primary types |
|------|----------------|
| Lexer | `leekscript.compiler.LexicalParser`, `Token`, `TokenType` |
| Parser / analysis | `leekscript.compiler.WordCompiler`, `leekscript.compiler.bloc.*`, `leekscript.compiler.expression.*`, `leekscript.compiler.instruction.*` |
| Operators | `leekscript.compiler.expression.Operators` |
| Types | `leekscript.common.Type` and related `*Type` classes |
| Diagnostics | `leekscript.common.Error`, `leekscript.compiler.AnalyzeError` |
| Runtime | `leekscript.runner.AI`, `leekscript.runner.Session`, values under `leekscript.runner.values` |
| Builtins | `leekscript.runner.LeekFunctions`, `LeekConstants` (large generated/registry data) |

---

## 1. Compilation units

- A **compilation unit** is a sequence of Unicode code points. The Java implementation holds source in `String` (UTF-16); another implementation should document Unicode handling (including surrogate pairs in raw source text).
- Compilation is organized in passes in `WordCompiler`: tokenization, a **first pass** that collects declarations (includes, globals, functions, forward class names), then a **second pass** that parses the full program structure and builds instructions, followed by **semantic analysis** (`preAnalyze` / `analyze` on blocks).

---

## 2. Lexical structure

### 2.1 Whitespace

Space (`U+0020`), carriage return, line feed, tab, and non-breaking space (`U+00A0`) are skipped.

### 2.2 Comments

- **Line comment:** `//` runs to (but does not include) the line terminator; the newline is consumed.
- **Block comment:** `/*` … `*/`. In **version before 2**, if the character immediately after `/*` is `/`, the lexer consumes that slash and ends the comment early (see `tryParseComments()`).

**Note:** In the reference compiler, ordinary `//` comments are discarded in the lexer and are **not** interpreted as toolchain directives. [§11](#11-toolchain-directives) defines the toolchain interpretation layer.

### 2.3 Identifiers and token kinds

Character classes for identifiers follow `tryParseIdentifier()` / `tryParseNumber()`: ASCII letters and digits, extended Latin blocks, `_`, `ÿ`, plus numeric exponent parts.

**Important:** Words that are not reserved keywords are emitted as `TokenType.STRING` (the parser treats these as identifiers). Reserved words emit dedicated `TokenType` values (`VAR`, `IF`, `CLASS`, …).

### 2.4 Case rules

- **Version ≤ 2:** Keyword and punctuation matching is **case-insensitive** (`wordEquals`, `charEquals` in `LexicalParser`).
- **Version ≥ 3:** Matching is **case-sensitive**.

### 2.5 Reserved word list

The lexer carries a full reserved list in `LexicalParser.reservedWords`:

`abstract`, `and`, `as`, `await`, `break`, `byte`, `case`, `catch`, `char`, `class`, `const`, `constructor`, `continue`, `default`, `do`, `double`, `else`, `enum`, `eval`, `export`, `extends`, `false`, `final`, `finally`, `float`, `for`, `function`, `global`, `goto`, `if`, `implements`, `import`, `in`, `instanceof`, `int`, `interface`, `let`, `long`, `native`, `new`, `not`, `null`, `or`, `package`, `private`, `protected`, `public`, `return`, `short`, `static`, `super`, `switch`, `synchronized`, `this`, `throw`, `throws`, `transient`, `true`, `try`, `typeof`, `var`, `void`, `volatile`, `while`, `with`, `xor`, `yield`

Whether each word becomes a keyword token or a `STRING` identifier depends on **language version** (see [§4](#4-language-versions)).

**Word forms mapped to operators:** `and` → `&&`, `or` → `||`, `xor` remains the `xor` operator token.

### 2.6 Literals

- **Numbers:** Scanned in `tryParseNumber()`. Underscores may appear in numeric text and are stripped before parsing (see `WordCompiler` number path). Hex (`0x`) and binary (`0b`) prefixes are accepted in the expression parser.
- **Strings:** `'` or `"` delimiters; backslash toggles “escape” state so an escaped quote does not close the string (`tryParseString()`).
- **Special single-character “identifiers”:** `∞` (lemniscate), `π` (pi), mapped to dedicated token types and then to numeric constants in the expression parser.

### 2.7 Punctuation and operator characters (lexer)

Multi-character operators are matched **longest first**; the **order of strings** in `tryParseOperator()` is significant when prefixes overlap.

Representative spellings tokenized as `TokenType.OPERATOR` (non-exhaustive; see source):

`:`, `&&`, `&=`, `&`, `||`, `|=`, `|`, `++`, `+=`, `+`, `--`, `-=`, `-`, `**=`, `**`, `*=`, `*`, `/=`, `/`, `\=`, `\`, `%=`, `%`, `===`, `==`, `=`, `!==`, `!=`, `!`, `<<<=`, `<<<`, `<<=`, `<<`, `<=`, `<`, `>>>=`, `>=`, `>`, `^=`, `^`, `~`, `@`, `??=`, `??`, `?`

**Arrows:** `=>` and `->` → `TokenType.ARROW` (return types, lambdas).

**Range:** `..` → `TokenType.DOT_DOT` (intervals).

**Dot:** `.` is only recognized as an operator token from **version ≥ 2** (`tryParseExact('.', TokenType.DOT)`).

**Note:** The reference lexer source **comments out** some `>`-prefixed multi-character operators in the same array (see line comments in `LexicalParser` near `">>>="`). Implementations must match the reference tokenizer **exactly** for parity; do not assume C-style `>>` / `>>=` appear as single tokens unless your tokenizer mirrors the current Java table.

Other punctuation: `,` (`VIRG`), `;` (`END_INSTRUCTION`), `(` `)` `[` `]` `{` `}` as in `TokenType`.

### 2.8 Logical negation

- The keyword **`not`** produces `TokenType.NOT` and is compiled as logical negation (`Operators.NOT`).
- The **`!` character** is lexed as an operator; in prefix unary position the parser maps the non-null assertion code to **`Operators.NOT`** (`WordCompiler.readExpression`: `NON_NULL_ASSERTION` → `NOT` for unary prefix).

---

## 3. Operator semantics and precedence

Binary and unary operators are classified in `Operators`. **Precedence** is given by `Operators.getPriority(int)` where **larger numbers bind tighter** (subexpressions with higher priority are evaluated first). Ternary (`?` / `:`) and assignment-family operators use the lowest precedence (0–1).

| Precedence (representative) | Operators / forms |
|------------------------------|-------------------|
| 17 | `[` `]` indexing, `(` `)` |
| 16 | `.` member access |
| 15 | postfix `++` `--`, `!` (non-null assertion form), unary `+` `-` prefix, `@` reference (deprecated in v2+) |
| 14 | unary `!` / logical not forms, `~` |
| 13 | `new` |
| 12 | `**` |
| 11 | `*` `/` `\` (integer division) `%` |
| 10 | `+` `-` |
| 9 | `<<` `>>` `>>>` |
| 8 | `<` `<=` `>` `>=` `instanceof` `in` `not in` |
| 7 | `==` `!=` `===` `!==` |
| 6 | `&` (bitwise and) |
| 5 | `^` (bitwise xor) |
| 4 | `\|` (bitwise or) |
| 3 | `&&` |
| 2 | `\|\|` `??` |
| 1 | `?` `:` (ternary) |
| 0 | `=` and all `op=` assignments including `**=` `??=` shifts, etc. |

The keyword / token **`xor`** (logical xor) maps to `Operators.XOR`. The reference `getPriority` switch does not list `XOR`; precedence for `xor` chains is therefore **defined by the reference parser’s behavior and tests**, not by this table alone.

**Version-specific:** In **version 1**, `^=` is treated as **power assignment** (`POWERASSIGN`), not bitwise xor assignment (`Operators.getOperator`).

**Deprecated / warnings:**

- **`@` reference** on parameters: warning `REFERENCE_DEPRECATED` from version ≥ 2.
- **`===` / `!==`:** warning `TRIPLE_EQUALS_DEPRECATED` from version ≥ 4 (`LeekExpression.analyze()`).

---

## 4. Language versions

`LeekScript.LATEST_VERSION` is **4** (`LeekScript.java`). Version affects lexing, parsing, analysis, and which standard functions exist (`minVersion` / `maxVersion` on definitions, checked in `LeekFunctionCall`).

### 4.1 Summary of differences

| Topic | v1 | v2+ | v3+ | v4+ |
|-------|----|-----|-----|-----|
| Case sensitivity | Insensitive | Insensitive through v2; **v3+ sensitive** | — | — |
| `.` member access | Not a dedicated `.` token | Yes | — | — |
| OOP keywords (`class`, `new`, `super`, …) | Limited | Expanded | More keywords | — |
| `^=` meaning | Power assign | Bitwise xor assign | — | — |
| `===` / `!==` | Allowed | Allowed | Allowed | **Deprecated** (warning) |
| Arity vs stdlib | Relaxed in older checks | — | Stricter parameter counts vs definitions | — |
| **Numeric width (language model)** | **32-bit** | **32-bit** | **32-bit** | **64-bit** |

LeekScript treats **integer** values as **32-bit** in versions **&lt; 4** and **64-bit** from **version 4** onward (aligned with the evolution of the reference runtime’s number lattice).

**Parity note:** The `java_vm_suite` snapshots checked by `leekscript_run` were taken from the JVM reference; many cases still observe **64-bit** literals and `Integer.MIN_VALUE` / `MAX_VALUE` on older versions because the reference stores numeric values as Java `long` in those paths. A future tree interpreter can gate wrapping, literals, and `Integer.*` bounds on `language_version` to match the **32-bit / 64-bit** rule above, then refresh or dual-record expectations as needed.

Many keywords are only recognized from **v2** or **v3** (see `LexicalParser.tryParseIdentifier()`).

### 4.2 Version 1 quirk

If an expression consists only of **`not`** with no operand in the sense of the v1 parser, the compiler may rewrite it to a **local variable** reference using the operator token as a name (`WordCompiler.readExpression` branch for `getVersion() == 1`). Preserving this behavior matters only for legacy parity.

---

## 5. Strict mode

`Options.strict` propagates into `AIFile` and the analysis pipeline. Effects include:

- **Unused variables:** reported as `UNUSED_VARIABLE` when strict (`AbstractLeekBlock` and related).
- **Stricter typing** for some declarations and expressions (see comments in `LeekVariableDeclarationInstruction`, `LeekExpression`, and `Type.elementAccess` overloads that take `strict`).

Exact rules remain **defined by the Java implementation**; this document lists representative effects only.

---

## 6. Type system (static)

The analyzer uses `leekscript.common.Type` and compound/function/array/map/set/interval wrappers.

### 6.1 Primitive and built-in type names

As parsed in `WordCompiler.eatPrimaryType()` / `eatOptionalType()`:

- `void`, `null`, `boolean`, `any`, `integer`, `real`, `string`
- `Class`, `Object`
- `Array<T>`, `Set<T>`, `Map<K,V>` with angle-bracket parameter lists
- `Function<…>` with parameter types, optional `->` return type inside the generic list
- User **class** types: if a `class` with that name was forward-declared, the identifier resolves to that class’s type.

### 6.2 Compound types

- **Union:** `T | U | …` using the `|` operator between types (`LeekCompoundType`).
- **Nullable:** `T?` appends `null` to the compound (`?` after a type fragment).

Function types and generics are validated during analysis; errors include `TYPE_EXPECTED`, `CLOSING_CHEVRON_EXPECTED`, `INCOMPATIBLE_TYPE`, `IMPOSSIBLE_CAST`, etc.

---

## 7. Program structure

### 7.1 Top-level declarations (first pass)

`WordCompiler.firstPass()` walks tokens at the main level and:

- Parses **`include("name")`** (string literal AI name) — only valid in the main block (enforced again in `includeBlock`).
- Parses **`global`** declarations with optional initializers.
- Registers **`function name ( … )`** signatures (parameter count).
- Registers **`class "ClassName"`** forward declarations (`TokenType.STRING` class name in the first-pass branch).

### 7.2 Second pass: statements

`compileWord()` dispatches on the next token. At top level or inside blocks, statements include (non-exhaustive; see `WordCompiler`):

- Empty statement: `;`
- **`var`** / typed variable declarations (after optional type prefix)
- **`global`** (with placement rules)
- **`return`** with optional `?` marker and optional expression
- **`for`**, **`while`**, **`if`**, **`else`**, **`do`**, **`switch`**
- **`include`** (main only)
- **`function`** user function definition
- **`class`** body (version ≥ 2, main block) — full class declaration
- **`break`**, **`continue`**
- **`break`** / **`continue`** must appear in breakable constructs
- Expression statements: any expression followed by `;`

**Typed leading form:** If the parser can read a **type** (`eatType`) and the next token is an identifier (`STRING`), it treats the construct as a **variable declaration**; otherwise it rewinds and parses an expression statement (`compileWord` save/restore pattern).

### 7.3 Functions

`function name ( [type] [@] param, … ) [ => returnType ] { … }`

- Parameters may use deprecated `@` “by reference” marker (warning in v2+).
- Return type after `=>` / `->` (`ARROW`) is optional in the grammar but required for typed returns when specified.
- User functions may only appear in the **main** block (`FUNCTION_ONLY_IN_MAIN_BLOCK`).

### 7.4 Classes

- **Forward declaration (first pass):** introduces the class name before the body is parsed.
- **Definition (second pass):** `class "Name"` then optional `extends` parent, then `{` members `}`.

Inside a class, members are introduced with optional `public` / `private` / `protected`, optional `static`, optional `final`, **`constructor`**, or methods/fields parsed via `endClassMember`. Names of members are **string tokens** (identifier spelling) per the lexer.

### 7.5 `for` loops

`for (` … `)` supports multiple shapes in `forBlock()`:

- **C-style:** init; condition; update with optional typed/`var` declarations.
- **Iterator:** `for (x in container)` and `for (key : value in container)` (with independent optional declarations for key and value).
- References with `@` on loop variables follow the same deprecation rules.

### 7.6 `include`

Syntax: `include ( "OtherAI" )` where the argument is a **string literal** token. Resolution and semantics depend on the virtual file system (`LeekScript.getFileSystem()`, `mMain.includeAI`).

---

## 8. Expressions (overview)

Expressions are built in `readExpression()` / `LeekExpression` and include:

- Literals, variables, `this`, `super`, `class`, `null`, `true`, `false`
- Parentheses, casts, function calls, method calls, operators from [§3](#3-operator-semantics-and-precedence)
- **Arrays / maps / intervals:** `[` … `]`, `{` key `:` value … `}` (object literals, v2+), interval forms with `DOT_DOT`
  - **Map vs Object:** Bracket keyed literals (`[ … ]` with `:` entries), `new Map(…)`, and map-typed APIs use a **Map** value. Curly **object** literals `{ … }` produce a distinct **Object** value. They are not the same runtime kind (`instanceof "Map"` vs `instanceof "Object"`, and equality treats them separately); there is no single “map shape” that unifies both.
- **Anonymous functions:** `function (…) [ => type ] { … }`
- **`new`** (v2+)
- **Sets:** `<` … `>` set literals (`readSet`)

---

## 9. Standard library and runtime

### 9.1 Registration

Native functions and constants are registered in **`LeekFunctions`** and **`LeekConstants`**. Each callable may expose multiple **versions** with possibly different types and arity; the analyzer picks the applicable overload and enforces **version gates** and **argument counts** (from v3 onward for count checks in the cited `LeekFunctionCall` logic).

### 9.2 Execution model (Java reference)

LeekScript compiles to **Java** (`JavaWriter`, `AI` subclasses). At runtime, `AI` tracks:

- **Operations** (`mOperations`) against `MAX_OPERATIONS` (default 20_000_000)
- **Memory** (`mRAM`) against `MAX_RAM` (documented in `AI.java` as a quad-based limit)

Errors during execution map to `Error` values such as `TOO_MUCH_OPERATIONS`, `OUT_OF_MEMORY`, `DIVISION_BY_ZERO`, `ARRAY_OUT_OF_BOUND`, etc.

A Rust runtime must either replicate these limits and errors for parity or document divergences.

---

## 10. Diagnostics (`Error` enum)

The compiler and runtime share a large `leekscript.common.Error` enumeration. Categories include:

- **Parse / structure:** `OPENING_PARENTHESIS_EXPECTED`, `END_OF_INSTRUCTION_EXPECTED`, `NO_BLOC_TO_CLOSE`, …
- **Names:** `VARIABLE_NAME_UNAVAILABLE`, `UNKNOWN_VARIABLE_OR_FUNCTION`, …
- **Functions:** `INVALID_PARAMETER_COUNT`, `FUNCTION_NOT_AVAILABLE`, `REMOVED_FUNCTION`, `DEPRECATED_FUNCTION`, …
- **Types:** `INCOMPATIBLE_TYPE`, `IMPOSSIBLE_CAST`, `ASSIGNMENT_INCOMPATIBLE_TYPE`, …
- **Classes / OOP:** `PRIVATE_METHOD`, `PROTECTED_FIELD`, `SUPER_NOT_AVAILABLE_PARENT`, …
- **Runtime / platform:** `TOO_MUCH_OPERATIONS`, `OUT_OF_MEMORY`, `AI_TIMEOUT`, `CODE_TOO_LARGE`, …

**Reference ids** (enum names above) stay the parity contract with the Java implementation and tests.

The toolchain also assigns **stable diagnostic codes** (`E####`) for filtering, docs URLs, and CI—see [**Diagnostic codes**](../design/diagnostic-codes.md). Each user-facing diagnostic should carry **both** the reference id (when applicable) and an `E####` code from the registry described there.

---

## 11. Source directives (Rust toolchain)

The Java lexer **does not** interpret pragma comments. For **formatter**, **parser**, **LSP**, and **CLI**, **source directives** pin version, formatting, and experimental behavior.

Full toolchain details—**`Leek.toml`**, **local push/pop**, and **precedence**—are in [**`directives.md`**](../design/directives.md) and [**`leek-toml.md`**](../design/leek-toml.md).

### 11.1 Syntax

- Line comments whose text after `//` (optional spaces) begins with `leek-`.
- **Key–value:** `// leek-<name>: <value>` or `// leek-<name> = <value>`.
- **Flag:** `// leek-<name>`.

**Scope:** Defaults to the **whole file**; tools may restrict some directives to a **preamble** (e.g. first 64 lines or before the first non-comment token).

Example:

```text
// leek-version: 4
// leek-strict: false
// leek-fmt: width=100, indent=4
// leek-experimental: pipeline-syntax
```

### 11.2 `leek-version`

Selects the **language version** integer. Recommended precedence: **CLI → project file → file directive → default (`LATEST_VERSION`)**.

### 11.3 `leek-strict`

`true` / `false` — toggles [strict mode](#5-strict-mode).

### 11.4 `leek-fmt`

Formatter options (width, indentation, etc.); exact keys belong to the formatter crate.

### 11.5 `leek-experimental`

Feature flags for extensions **not** in the Java reference; each flag should be documented in project design docs.

### 11.6 Java reference

Standard code without experimental flags should behave like the Java toolchain **modulo documented parity gaps**. Directives are **no-ops** for the Java compiler unless it gains support later.

---

## 12. Conformance

An implementation is **standard-conformant** for version *V* with strictness *S* if, for inputs that use neither experimental flags nor toolchain-only directives, it matches the reference compiler’s accept/reject behavior and observable runtime results within documented resource limits.

**Regression oracle:** tests under `leek-wars-generator/leekscript/src/test` and the version matrix helpers in `TestCommon` (`code_v1_4`, `code_v4`, `code_strict_v4_`, etc.).

---

## 13. Document status

This specification is **descriptive**, derived from reading the Java sources. Ambiguities should be resolved by:

1. The behavior of the reference compiler at `LATEST_VERSION`, and  
2. Executable tests in the `leekscript` module.

Future revisions may add formal grammars (BNF), a complete keyword-by-version matrix, and exhaustive builtin signatures once extracted mechanically from `LeekFunctions`.
