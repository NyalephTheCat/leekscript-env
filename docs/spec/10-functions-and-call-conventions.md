# Functions and call conventions

**Normative** for calls in **this implementation’s** interpreter.

## Declaration forms

- Named **`function`** declarations (statements).
- Anonymous **`function (…) { … }`** and **arrow** functions (expressions).

## Parameters

- **Positional** arguments map to parameters left-to-right.
- **Default arguments** — When omitted, the **default expression** is evaluated in the **callee** environment (**reference** optional parameters).
- **Arity** — Too few / too many arguments → **`INVALID_PARAMETER_COUNT`** (unless defaults or variadic rules apply as implemented).

## By-reference (`@`)

- **Parameter `@name`** — For **`v1`**, container values **MAY** be passed by reference (**reference** **`pass_parameter_value`** parity).
- **Return `@`** — Returns a reference to a container cell per **`Return { by_ref: true }`**.

Exact reference semantics **MUST** match **VM export parity** scenarios.

## `this` in functions

Non-method functions **MUST NOT** use **`this`** meaningfully; misuse is a runtime error where detected.

## Methods

Instance calls **`obj.method(args)`** bind **`this`** to **`obj`** for the method body. **Static** methods dispatch on the class value.

## Constructors

**`new ClassName(args)`** runs **`constructor`** with **`this`** bound to the new instance.

## Closures

Function values **MAY** capture outer variables; mutation of captured bindings follows the tree interpreter’s interior-mutability model (*informative:* embedding is not thread-safe — see [embed-toolchain.md](../architecture/embed-toolchain.md)).

## Recursion

Recursion **MUST** be supported; stack overflow is **implementation-defined** (may appear as internal error or abort under limits).

## Include and multi-file

**`include`** does not create modules with separate top-level scopes beyond statement concatenation — it is textual composition at the HIR level after expansion (see pipeline documentation).

---

*Revision: functions chapter.*
