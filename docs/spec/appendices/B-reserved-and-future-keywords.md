# Appendix B — Reserved and future keywords

**Normative** for lexer classification by **language version**; **parse support** may lag (see **implementation note (this repository)** in [03-lexical-grammar.md](../03-lexical-grammar.md)).

## Word operators (not plain identifiers)

| Spelling | Notes |
|----------|--------|
| **`and`**, **`or`**, **`xor`** | Logical / bitwise word operators (`word_eq` for **v≤2**). |
| **`instanceof`** | Operator token when **v ≥ 2**. |
| **`is`** | Equality sugar; pairs with **`not`** for **`is not`**. |

## Keywords (`Kw` enum)

All spellings below are lexed as **`Kw`** when **`classify_word`** matches the keyword classifier. **Version gates** are noted.

**Generally available (with `word_eq`)**  
`as`, `var`, `global`, `return`, `for`, `if`, `while`, `in`, `break`, `continue`, `do`, `else`, `include`, `not`, `null`, `function`, `true`, `false`

**v ≥ 2**  
`constructor`, `final`, `static`, `instanceof` (as word op), `super`, **`class`** (exact `class` only), `extends`, `new`, `private`, `protected`, `public`, `this`, `void`

**v ≥ 3** (additional reserved words)  
`abstract`, `await`, `import`, `export`, `goto`, `switch`, `catch`, `const`, `char`, `enum`, `eval`, `case`, `float`, `double`, `byte`, `try`, `with`, `yield`, `finally`, `interface`, `long`, `let`, `native`, `package`, `implements`, `int`, `short`, `throws`, `throw`, `transient`, `volatile`, `default`, `synchronized`, `typeof`

*Informative:* Some **`v ≥ 3`** words are reserved to match **reference** / future syntax; full statement semantics are not all implemented in **this** toolchain yet.

## Ignored directive names

**`allow`**, **`push`**, **`pop`** in **`// leek-*`** preambles are **not** keywords in source code.

---

*Revision: derived from the workspace keyword classifier; resync on lexer changes.*
