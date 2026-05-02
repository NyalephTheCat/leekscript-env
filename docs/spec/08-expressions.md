# Expressions

**Normative** for each **`HirExpr`** variant. Runtime errors **MUST** use interpreter **`reference`** ids where specified.

## Literals

- **`Integer`**, **`Real`**, **`String`**, **`Bool`**, **`Null`** — Self-explanatory values.
- **`This`**, **`ClassSelf`** — Context-dependent (ch. 5–6).

## Identifiers

Resolve to binding or global; undefined → **`VARIABLE_NOT_EXISTS`**.

## Unary operators

| Op | Meaning |
|----|---------|
| **`-`** | Negation |
| **`!`** | Logical not |
| **`~`** | Bitwise not (integer) |
| **`typeof`** | Type code |
| **`@`** *expr* | Reference / by-ref wrapper (**`v1`** container passing semantics; see ch. 10) |

## Binary operators

Arithmetic: **`+ - * / % \`** (backslash **integer division**), **`**`** power.

Comparisons: **`< <= > >=`**, **`== != === !==`**, **`is` / `is not`**.

Logical: **`and` / `or` / `xor`** (word ops), **`&&` / `||`** short-circuit.

Bitwise: **`& | << >> >>>`**.

Other: **`instanceof`**, **`in`**, **`not in`**, **`??`** nullish coalesce.

Errors such as **division by zero** → **`DIVISION_BY_ZERO`**.

## Ternary

**`cond ? then : else`** — **`then` / `else`** evaluated only as needed after **`cond`**.

## Cast

**`expr as Type`** — Runtime cast with **reference** rules; impossible **static** casts **MAY** be rejected in **Types** phase (see ch. 6).

## Call

**`callee(args…)`** — **`callee`** evaluated, then arguments. **`NOT_CALLABLE`**, **`INVALID_PARAMETER_COUNT`**, **`WRONG_ARGUMENT_TYPE`** as appropriate. Unknown removed builtins → **`REMOVED_FUNCTION_REPLACEMENT`** / **`FUNCTION_NOT_AVAILABLE`**.

## Collection literals

- **`[ … ]`** — Array literal.
- **`[ k: v, … ]`** — Bracket **map** literal.
- **`{ k: v, … }`** — **Object** literal (**`v ≥ 2`**).

## `new`

**`new Type(args…)`** for **`Map`**, **`Set`**, **`Interval`**, user classes, etc. Behavior **MUST** match **reference** constructors as implemented.

## Member and index

- **`obj.field`** — Field read or method value.
- **`obj[index]`** — Indexing with **reference** negative-index rules for arrays (**`-1`** last element), then bounds check; strict mode **MAY** tighten errors (**`array_out_of_bound_strict`**).

## Slice

**`base[start:end:step]`** (bounds optional) — Half-open intervals and **`step`** rules per **reference** array slice semantics (HIR documents the mapping). **`step == 0`** treated as **`1`**.

## Function values

- **`FunctionLiteral`** — `function (…) { stmts }` or arrow with braced body.
- **`ArrowClosure`** — `(a, b) => expr` single-expression body.

Closures capture lexical environment as implemented.

## Assignment expression

**`lhs op= rhs`** / **`=`** — Value is **post-assign** place value (**reference**). **`lhs`** **MUST** be an lvalue.

## Increment / decrement

- **Prefix `++` / `--`** — Result is **value after** update.
- **Postfix `++` / `--`**

> **Implementation note (this repository):** In the **tree interpreter**, **postfix** **`++` / `--`** evaluates to **`null`**, not the pre-update value. The **reference implementation** may differ. Prefer **prefix** form when a value is needed.

---

*Revision: expressions chapter.*
