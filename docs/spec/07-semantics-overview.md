# Semantics overview

**Normative** for evaluation in **this implementation’s** tree interpreter.

## Values and effects

Expression evaluation **produces** a **value** (or completes abruptly — see **Abrupt completion**). Statements **sequence** effects: bindings, mutation, control transfer, host calls.

## Order of evaluation

- **Function arguments** — Evaluated **left-to-right** before the call.
- **Binary operators** — Generally **left then right** except short-circuit (**`&&`**, **`||`**, **`??`**) where the right-hand side **MUST NOT** be evaluated if the left determines the result.
- **Member / index / call chains** — **Left-to-right** with **reference**-like semantics.

## Abrupt completion

These **MUST** propagate until handled:

- **`return`** — Exits the innermost function with an optional value (including **`return ?`** conditional return).
- **`break` / `continue`** — Affect the innermost enclosing loop (or error if none).
- **`throw`** — Raises a value; unwinds through **`try`/`catch`/`finally`**.
- **Resource exhaustion** — Operation count, RAM quota, stack depth (see [13-interpreter-behavior.md](13-interpreter-behavior.md)).

Uncaught **`throw`** **MUST** surface as **`UNCAUGHT_THROW`** (or host-mapped equivalent).

## Determinism

For a fixed **program**, **language version**, **strict** flag, **limits**, **host** implementation, and **random seed** (if the program uses randomness), execution **SHOULD** be deterministic. *Informative:* Floating-point may still exhibit platform nuances; parity tests constrain observable behavior.

## Equality

- **`==` / `!=`** — **Reference**-style equality on Leek values.
- **`===` / `!==`**

> **Implementation note (this repository):** In the current tree interpreter, **`===` / `!==`** lower to the **same** comparison as **`==` / `!=`**. This **MAY** differ from ECMAScript-style strict equality. Future alignment follows the **reference implementation**.

## Truthiness

Condition contexts (**`if`**, **`while`**, **`for`**, **`?:`**, **`return ?`**) use **reference** truthiness (`null` false, `false` false, **`0`** false, **`""`** false in VM parity, etc.) as implemented.

## Typeof

Unary **`typeof`** returns **numeric type codes** consistent with the **reference** **`typeOf`** constants (see interpreter).

---

*Revision: semantics overview.*
