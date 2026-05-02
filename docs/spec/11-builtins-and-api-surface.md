# Builtins and API surface

**Normative** at the level of **name availability** and **callability**; **per-function** semantics are defined by **VM export parity** tests, bundled **signature** Leek sources, and the **interpreter** builtin tables (including host hooks).

## Global namespace

These names **MUST** be treated as **pre-defined globals** for name resolution:

1. Every identifier in the **stdlib global identifier list** maintained alongside the resolve pass (re-exported by the interpreter crate).
2. **`Infinity`**, **`PI`**, **`E`** (additionally seeded in the resolve global map).
3. Names injected from **merged signature** bundles (bundled **stdlib-oriented** and **game-host** signature sources, plus workspace TOML) when compilation merges signature globals.

Calling an **undefined** or **removed** function **MUST** produce **`FUNCTION_NOT_AVAILABLE`** or **`REMOVED_FUNCTION_REPLACEMENT`** as implemented.

## Full signature catalog (*informative*)

Every bundled **`function`** and documented **`global`** constant appears in **[appendices/F-builtin-signatures-catalog.md](appendices/F-builtin-signatures-catalog.md)** (generated from those signature sources). Regenerate after signature edits with **`python3 scripts/gen_spec_appendices.py`** from the repository root.

## API families (*informative*)

The stdlib surface is grouped below for navigation; **spellings and arity** match the stdlib global list and appendix F.

| Family | Examples | Notes |
|--------|-----------|--------|
| **Constants / `typeof`** | `TYPE_NULL`, `TYPE_ARRAY`, `PI`, `E`, `COLOR_*`, `SORT_*` | Declared in signature sources; some duplicated in resolve seeds. |
| **Math** | `abs`, `sin`, `cos`, `sqrt`, `floor`, `atan2`, `pow` | Mix of `integer` / `real` overloads per signatures. |
| **Strings** | `length`, `charAt`, `contains`, `split`, `replace` | Cost hints in signature doc comments (`@cost`). |
| **Arrays** | `arrayMap`, `arrayFilter`, `arraySort`, `arraySlice`, `arrayConcat` | Purity varies; some allocate. |
| **Maps / assoc** | `count`, `assocSort`, key/value helpers | Right-hand of `in` / iteration per [08-expressions.md](08-expressions.md). |
| **Bits / encoding** | `bitCount`, `bitReverse`, `binString` | Integral semantics. |
| **Debug / meta** | `debug`, `debugC`, `getOperations`, `getTurnOperations` | Observable; interacts with limits ([13-interpreter-behavior.md](13-interpreter-behavior.md)). |
| **Fight / leek / chips** | `attack`, `getLife`, `getLeek`, `useChip`, … | **Host-visible**; large set in the game-host signature layer. |

## Signatures and types

Static **signatures** for tooling (`lek check`, IDE) come from:

- **Bundled signature-definition sources** (stdlib-oriented layer + game-host layer),
- optional workspace registry / TOML (see [registry.md](../reference/registry.md)).

**Runtime** arity and type checks **MUST** match the interpreter’s builtin dispatch tables, which track the **reference implementation**. Where a signature declaration and the interpreter disagree, **interpreter + export parity tests** are authoritative until fixed.

### Doc comments (*informative*)

Signature sources use **`@brief`**, **`@param`**, **`@return`**, and **`@cost`** (operations). These are **not** normative for this spec but are the best human-readable contract alongside upstream documentation.

## Constness and purity

Whether a builtin is **pure** or **observable** (I/O, RNG, game state) is **not** fully normative in this chapter; treat **fight-related** calls as **host-visible** side effects.

## Versioning API additions

New builtins **SHOULD** be added to the stdlib global list, **signature** sources, and **interpreter** builtin tables (or host) in one change set, then the **spec appendix generator** run to refresh appendix F. Language **version** gates belong in lexer/parser if the **spelling** is new syntax; for new **global functions**, parity tests **SHOULD** cover **reference** availability.

## Tooling-only hooks (*informative*)

**`lek registry`** / **`verify-emit-refs`** validates diagnostic **`reference`** strings against the bundled registry — this does **not** add new user-visible builtins but affects CI for the catalog above.

---

*Revision: builtins chapter; catalog in appendix F.*
