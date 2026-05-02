# Types and subtyping

**Normative** for **type syntax** and **checks that exist today**. **Full static typing** is *not* normative yet.

## Type syntax (`HirTypeExpr`)

Type expressions in source lower to:

- **`Named`** — `integer`, `real`, `string`, `boolean`, `any`, `Object`, `Class`, `void`, or a user **class** name.
- **`Nullable`** — postfix **`?`**.
- **`Union`** — `A | B | …`.
- **`Generic`** — `Base<Args…>` with optional **function** return position **`=> T`**.

These forms appear in **casts** (`expr as Type`), **parameter** / **return** annotations, and **field** declarations.

## Runtime value kinds

The tree interpreter’s **`Value`** model aligns with **reference** Leek values *informatively*:

- **`null`**
- **boolean**, **integer**, **real**, **string**
- **array**, **map** (bracket map / `new Map`), **object** literal maps (**map** vs **object** kinds in the **reference** type system), **set**, **function**, **class** instances, and host-specific types.

**Bracket map** `[k: v, …]` and **`{ k: v, …}` object literal** are distinct at runtime (distinct **map** vs **object** kinds in the **reference** model).

## Assignability and coercion

Assignment and compound assignments apply **reference-style coercion** in the interpreter, extended by **`strict`** mode:

- When **`strict`** is **enabled** and language version supports it, some assignments **MUST** preserve stricter numeric / tag rules (assignment / lvalue logic in the interpreter).
- When **`strict`** is **disabled**, behavior **MUST** match the default (game-like) coercion rules exercised against the **reference** VM.

*Informative:* Details are intentionally distributed across the interpreter; parity is enforced by **export parity tests** and scenario compares.

## Compile-time type analysis

The workspace **type-check pass** currently performs **minimal** analysis:

- It walks HIR and rejects **provably invalid `as` casts** from obvious local shapes (literals, certain constructors).

A **`TypeDiagnostic`** **MUST** be surfaced during compilation in the **Types** phase when such a cast is detected.

### Static errors (policy)

Until each rule has a stable **`E####`**, implementations **SHOULD** emit diagnostics with **`reference`** strings and spans consistent with [diagnostics-registry.md](../reference/diagnostics-registry.md). New rules **MUST** add or reuse registry entries (see [appendices/C-diagnostic-codes-mapping.md](appendices/C-diagnostic-codes-mapping.md)).

## Generics

Generic spelling exists for **arrays**, **sets**, **maps**, **functions**, and user classes. **Full generic static checking** is **not** specified here; runtime behavior follows **reference**-style erasure / checks as implemented.

## `instanceof`

Right-hand side of **`instanceof`** is a **type name**, not a runtime value expression (HIR: **`HirBinOp::Instanceof`**).

---

*Revision: types chapter; expand inference when static typing grows.*
