# Stable diagnostic codes (`E####`)

This document defines how **stable toolchain codes** (`E1001`, …) are allocated for `lek`, the LSP, and CI—alongside **reference ids** from `leekscript.common.Error` in the Java implementation (see [language spec §10](../spec/leekscript-language.md#10-diagnostics-error-enum)).

**Goals**

- Stable, grep-friendly codes for `allow`, deny lists, and documentation URLs.
- Enough **range separation** to add new diagnostics without renumbering unrelated areas.
- A clear place for **toolchain-only** diagnostics (no Java counterpart).
- **No code reuse:** once published, an `E####` keeps the same meaning for the life of a major toolchain version family (see [Stability](#stability)).

---

## Format

- Pattern: **`E`** followed by exactly **four** decimal digits: `E0000`–`E9999`.
- **`E0000`** is reserved (sentinel / “unset” in tooling; do not emit to users).
- Display convention: uppercase `E`, no space (`E1024`, not `e 1024`).

---

## Range allocation

| Range | Domain | Typical examples |
|-------|--------|------------------|
| **E0100–E0199** | Lexical scanning | invalid character, malformed numeric token, unclosed string |
| **E0200–E0399** | Syntactic structure | expected `(`, `}`, `;`, unexpected end of file, malformed block |
| **E0400–E0599** | Expression shape | incomplete expression, unexpected operator, value expected |
| **E0600–E0799** | Includes & AI graph | `include` placement, unknown AI name, load failures |
| **E1000–E1199** | Names & resolution | unknown variable/function, duplicate name, keyword misuse |
| **E1200–E1399** | User functions | declaration rules, parameter names, redefinition |
| **E2000–E2199** | Calls & builtins (API) | arity, version availability, removed/replaced functions |
| **E2200–E2399** | Operators & builtins (runtime shape) | invalid operator use, wrong dynamic type at call |
| **E3000–E3299** | Static types (general) | incompatible types, missing type, impossible cast |
| **E3300–E3599** | Flow & narrowing | comparison always true/false, useless cast, same-variable assign |
| **E3600–E3899** | Collections & intervals | indexability, iteration, interval bounds, `in` / `..` |
| **E4000–E4299** | Classes & OOP (structure) | duplicate field/method, extends cycle, constructor rules |
| **E4300–E4599** | Visibility & dispatch | private/protected field/method/constructor |
| **E4600–E4899** | `this`, `super`, `instanceof` | invalid use sites, wrong RHS for `instanceof` |
| **E5000–E5299** | Runtime values | division by zero, bad operand types, null/use errors |
| **E5300–E5599** | Runtime iteration & mutation | modification during iteration, entity died |
| **E5600–E5899** | Resource limits | operation budget, RAM, code size, stack overflow |
| **E5900–E5999** | Platform / loader | compile/write/load AI, disabled AI, interrupted |
| **E7000–E7199** | Manifest & config | invalid `Leek.toml`, unknown keys, path errors |
| **E7200–E7399** | Directives (`// leek-*`) | unknown directive, invalid value, disallowed scope |
| **E7400–E7599** | Formatter | conflicting fmt options, invalid region markers |
| **E7600–E7799** | Experimental gates | unknown flag, feature conflict |
| **E8000–E8799** | Warnings (deprecation & style) | deprecated `===`, deprecated `@`, deprecated function |
| **E8800–E8999** | Lint-only (optional) | pedantic lints not in reference compiler |
| **E9000–E9499** | Internal / ICE | compiler bug, assertion failure (not for parity tests) |
| **E9500–E9999** | Reserved | future expansion; do not allocate without updating this doc |

Gaps between sub-ranges are intentional so new diagnostics can be inserted **adjacent** to related codes without crossing domain boundaries.

---

## Relationship to `Error` (reference id)

The Java enum `leekscript.common.Error` is the **parity oracle** for names and behavior. The toolchain should:

1. Emit diagnostics with **both** `reference_id` (enum name) and **`E####`** when a mapping exists.
2. Maintain a single **registry** (see below) from `Error` → `E####` for the ~150 reference variants plus toolchain-only entries.

**Ordering policy:** Do **not** derive `E####` from the enum ordinal (`Error.ordinal()`). Ordinals change if enum members are reordered; stable codes must not.

---

## Registry (source of truth)

The canonical registry lives at **[`data/diagnostics/registry.yaml`](../../data/diagnostics/registry.yaml)** (148 Java `Error` mappings plus reserved toolchain-only entries). Each entry includes:

- `code`: stable `E####`
- `reference`: Java enum name, or `null` with `id:` for toolchain-only diagnostics
- `band`: classification (matches range tables above)

Example entry shape:

```yaml
  - code: E0101
    reference: INVALID_CHAR
    band: lexical
```

Build scripts or `build.rs` can generate Rust enums and test fixtures from this file.

---

## Stability

- **Patch releases:** Never change the meaning of an existing `E####`.
- **Minor releases:** May add new codes; may **deprecate** a diagnostic (message points to replacement code) but should not reuse the number.
- **Major releases:** May retire codes only with a documented migration table (rare).

---

## Severity

Stable codes are **orthogonal** to severity: the same code can be `error`, `warning`, or `hint` depending on context (e.g. strict mode, `# allow`).

---

## Quick reference: mapping clusters from Java `Error` (informative)

These are **suggested band targets** when filling the registry—not fixed until the first registry commit.

| Reference cluster | Target band |
|-------------------|-------------|
| `INVALID_CHAR`, `INVALID_NUMBER`, `STRING_NOT_CLOSED` | E01xx |
| `OPENING_*`, `CLOSING_*`, `NO_BLOC_*`, `END_OF_*` | E02xx–E03xx |
| `UNKNOWN_VARIABLE_OR_FUNCTION`, `VARIABLE_NAME_*` | E10xx–E11xx |
| `INVALID_PARAMETER_COUNT`, `FUNCTION_NOT_*`, `REMOVED_FUNCTION*` | E20xx |
| `INCOMPATIBLE_TYPE`, `TYPE_EXPECTED`, `IMPOSSIBLE_CAST`, … | E30xx |
| `PRIVATE_*`, `PROTECTED_*`, `CLASS_*`, `SUPER_*` | E40xx–E46xx |
| `DIVISION_BY_ZERO`, `ARRAY_OUT_OF_BOUND`, `TOO_MUCH_OPERATIONS`, … | E50xx–E56xx |
| `TRIPLE_EQUALS_DEPRECATED`, `REFERENCE_DEPRECATED`, `DEPRECATED_FUNCTION` | E80xx |
| Toolchain directive/config issues | E72xx |

---

## Document status

This is the **authoritative numbering plan** for the LeekScript toolchain. When the first `E####` is implemented, add a link from each code’s markdown page under `docs/errors/` (optional) and keep this file’s range table in sync when opening new bands.
