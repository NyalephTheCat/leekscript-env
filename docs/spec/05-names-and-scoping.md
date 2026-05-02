# Names and scoping

**Normative** for behavior implemented by the workspace **name resolution** and **interpreter** passes.

## Binding forms

These constructs introduce bindings:

| Construct | Scope | Notes |
|-----------|--------|--------|
| **`var` / typed local** | Innermost enclosing block or function body | Initialized per declaration rules. |
| **`function` name** | Enclosing scope (hoisted per implementation) | Recursion allowed. |
| **`for` init** | `for` body | Classic `for` init clause. |
| **`for (name in …)`** | Loop body | **`var`** / type form declares; plain name assigns to existing binding. |
| **`for (key : value in …)`** | Loop body | Same declaration rules for each name. |
| **`class` name** | Enclosing scope | **`v ≥ 2`**. |
| Field / method / constructor | Class body | Visibility applies (see [06-types-and-subtyping.md](06-types-and-subtyping.md)). |
| **`catch (e)`** | Catch block | Exception binding. |
| **`global`** | Outermost / global object | Installed in runtime global scope. |
| Function parameters | Function body | **`@name`** marks **by-ref** parameter (see ch. 10). |

## Name resolution

- **Unqualified identifiers** resolve lexically outward: block → function → outer functions → globals (including stdlib and **signature-injected** globals).
- **`this`** — Instance receiver in methods; restricted contexts emit **`THIS_NOT_ALLOWED_HERE`** at runtime when misused.
- **`super`** — Superclass dispatch (**`v ≥ 2`**) as implemented.
- **`class`** (value) — **`HirExpr::ClassSelf`**: enclosing user class as a value (**`v ≥ 2`**), **reference** parity for `class['x']` patterns.

## Shadowing

Inner bindings **MAY** shadow outer names. Exact shadowing and duplicate diagnostics **MUST** match the resolve pass (duplicate declarations in the same scope are errors).

## Forward references

*Informative:* Function declarations are callable within their scope per the implementation (align with **reference** hoisting rules). **Class** and **`var`** forward reference behavior **MUST** match interpreter tests.

## Forbidden constructs

- Use of an identifier with **no binding** — static error from resolve (**`VARIABLE_NOT_EXISTS`** at runtime if resolution was bypassed).
- **Invalid assignment targets** (e.g. assigning to a non-lvalue) — parse or lowering error.

## Builtin and signature globals

Names listed in the **stdlib global identifier list** and merged **signature** names are always considered defined in the global namespace for resolution (see [11-builtins-and-api-surface.md](11-builtins-and-api-surface.md)).

## Classes (**`v ≥ 2`**)

A **`class`** declaration introduces a **type name** and a scope for **fields**, **`static`** fields, **methods**, and **`constructor`**.

- **Inheritance** — **`extends`** names a superclass; **`super`** resolves superclass members.
- **Visibility** — **`public`**, **`protected`**, **`private`** on members match **reference** accessibility as implemented.
- **Final fields** — Assign-once semantics; violating assignment → **`CANNOT_ASSIGN_FINAL_FIELD`**.

Instance layout and method dispatch **MUST** match the **VM export parity** suite for covered programs.

---

*Revision: names and scoping chapter.*
