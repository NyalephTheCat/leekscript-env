# Statements and control flow

**Normative** for **`HirStmt`** execution.

## Expression and empty

**`expr;`** evaluates **`expr`** and discards the value. **`;`** alone is **`Empty`**.

## Blocks

**`{ stmts… }`** — New lexical scope for bindings (per resolve rules).

## Variable declaration

**`var`** / typed locals initialize to **`null`** when no initializer is given.

## Assignment statement

Same operators as **`AssignExpr`** but as a statement; no value used.

## `if`

**`if (cond) then [else else]`** — **`else`** binds to innermost **`if`**.

## Loops

- **`while (cond) body`**
- **`do body while (cond);`**
- **`for (init; cond; update) body`** — **`cond` omitted** means **always true**.
- **`for (name in container)`** — Iteration protocol for **arrays** and other iterables; non-iterable → **`NOT_ITERABLE`**. Iterating an **unbounded interval** (where detected) → **`CANNOT_ITERATE_UNBOUNDED_INTERVAL`**. **`@`** on loop variable aliases array cells when applicable (HIR).
- **`for (key : value in container)`** — Index + element form for arrays / maps per **reference** rules.

**`break`** / **`continue`** — **`BREAK_OUT_OF_LOOP`** / **`CONTINUE_OUT_OF_LOOP`** if no enclosing loop.

## `switch`

**`switch (discr) { case … default … }`** — **Fall-through** between **`case`** arms (**reference** style). **`case`** labels are expressions evaluated in order; **`default`** optional.

## Exception handling

**`try { … } catch (e) { … } finally { … }`** — **Reference** ordering (**`catch`** before **`finally`**). **`throw expr;`** or **`throw;`** (if allowed) raises.

## `return`

- **`return;`** / **`return expr;`**
- **`return ? expr;`** — Return only if **`expr`** is truthy (**`if_truthy`** in HIR).
- **`return @ expr;`** — By-ref return (**`by_ref`**) with **`v1`** container sharing semantics per **reference**.

## `global`

**`global [type] a = …, b, …;`** — Defines globals on the outer environment.

## `include`

**`include("relative-or-resolved path");`** — Compile-time expansion when the compilation options supply a **source path**; included statements record origins in HIR metadata.

---

*Revision: statements chapter.*
