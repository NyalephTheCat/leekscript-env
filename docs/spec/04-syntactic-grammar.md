# Syntactic grammar

**Normative** at the level of **accepted program shapes** (statements and expressions). Low-level productions are maintained in the workspace **parser**; this chapter summarizes structure and **precedence** so the spec is readable without duplicating every production.

## Start symbol

A **Leek file** is a sequence of **items** (statements and declarations) parsed as a **source file** root in the concrete syntax layer, then lowered to **`HirFile`** (a list of **`HirStmt`**).

## Items (top level)

Top-level constructs **MUST** include at least:

- expression statement,
- **`var` / typed variable** declaration,
- **`function`** declaration,
- **`class`** declaration (**`v ≥ 2`**),
- **`global`** declaration,
- control flow statements where grammatically allowed at top level,
- **`include("…")`** (expanded before or during compile when a source path is set).

## Declarations

### Variable declaration

**`var` *Name* [`=`*init*`]`*`;`*

**Typed declaration:** *Type* *Name* [`=`*init*`]`*`;`*

Omitted initializer **MUST** yield **`null`** for **`var`**.

### Function declaration

**`function` *Name* `(` *parameters* `)` *Block*

Optional return type and strict-mode annotations follow the parser/HIR.

### Class declaration (**`v ≥ 2`**)

**`class` *Name* [`extends` *Super*] `{` *members* `}`

Members: fields, **`static`** fields, methods, **`constructor`**, with **`public` / `protected` / `private`** and **`final`** as implemented.

## Statements

The statement forms in **`HirStmt`** are the **authoritative enum** of supported statements:

- block, empty, expression, assignment, **`if`**, **`while`**, **`do` … `while`**, **`for`**, **`for` … `in`**, **`for` *key* `:` *value* `in`**, **`switch`**, **`try` / `catch` / `finally`**, **`throw`**, **`break`**, **`continue`**, **`return`** (including **`return ?`** and **`return @`** forms), **`global`**, **`include`**.

See [09-statements-and-control-flow.md](09-statements-and-control-flow.md).

## Expressions

Expression forms are exactly those in **`HirExpr`**. See [08-expressions.md](08-expressions.md).

## Precedence and associativity

Binary operators **MUST** resolve with the precedence implemented in the **parser** (internal precedence tables). *Informative:* The spec does not duplicate every row — when in doubt, **add a regression test** and cite it in [appendices/E-conformance-tests-index.md](appendices/E-conformance-tests-index.md).

Notable groups (high to low, *informative sketch*):

- postfix: calls, indexing, member access, slices, **`++` / `--`**
- unary: **`!`**, **`-`**, **`~`**, **`typeof`**, **`@`**
- multiplicative / additive
- shifts
- relational and **`in` / `not in` / `instanceof`**
- equality / **`is`**
- bitwise AND / XOR / OR
- logical **`&&`** / **`||`** (short-circuit)
- nullish **`??`**
- ternary **`?:`**
- assignment expressions **`=`**, **`+=`**, …

**`is` / `is not`** — Parsed as binary / chained forms matching historical test-suite sugar.

## Ambiguity resolution

- **`/`** vs regex: disambiguation follows **reference** lexer/parser conventions; **this implementation** **MUST** agree with the **reference** on disambiguation.

## Directives

Source **MAY** begin with **`// leek-*`** preamble lines; see [12-directives-and-pragmas.md](12-directives-and-pragmas.md).

---

*Revision: syntactic chapter; consolidate precedence table into appendix A when stable.*
