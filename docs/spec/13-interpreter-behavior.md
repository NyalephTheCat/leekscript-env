# Interpreter behavior

**Normative** for the primary **interpret HIR** entry points and their variants (strict, limits/stats, custom host).

## Pipeline (*informative*)

Typical phases: **directives → lexer → parse → HIR → resolve → types → interpret**. See architecture documentation for how workspace packages compose.

## Strict mode

When **strict** is **enabled** (preamble / manifest / API):

- Assignment compatibility **MAY** be stricter (tag checks, integer coercion rules).
- Array bounds **MAY** use stricter errors where the loose mode would coerce or clamp differently.

Exact rules **MUST** match the interpreter’s assignment logic and **VM export parity** tests for the chosen **language version**.

## Resource limits

The interpreter **MAY** enforce:

- **Operation counter** — Charges per executed opcode / builtin step; exceeding limit → **`TOO_MUCH_OPERATIONS`**.
- **RAM quota** (“quads”) — Charges for allocations (strings, maps, arrays, clones); exceeding → **`OUT_OF_MEMORY`**.
- **Per-turn / per-fight** accounting — When a host provides turn boundaries, builtins like **`getOperations`** / **`getTurnOperations`** reflect counters (builtin implementation).

Limits **MAY** be **`None`** (unbounded) for local tooling.

## Host environment

**`InterpreterHost`** allows embedding (fight engine, I/O). Builtin implementations **MAY** call into the host for **random**, **debug**, **fight** actions, etc. Behavior **without** a custom host **MUST** match the default **stdlib** implementations in this repository.

## Abrupt completion and errors

- **`ExecAbort::Throw`** — Uncaught propagates as **`UNCAUGHT_THROW`** at top level.
- **`ExecAbort::Error`** — Carries **`InterpretError`** with stable **`reference`** where possible; see the interpreter crate’s published list of emitted reference ids.

## Determinism and `lek run`

CLI **`lek run`** **SHOULD** pass stable **language version**, **strict**, and **limits** so scripts behave predictably. Divergence from **in-game** execution (different host, different seeds) is **expected** for local runs; document scenarios in [correctness-and-parity.md](../architecture/correctness-and-parity.md). Fight **RNG**, **seeds**, and loaders are summarized in [scenario-format.md](../reference/scenario-format.md) and generator docs ([generator-and-engine.md](../architecture/generator-and-engine.md)).

## `include` resolution

**`include("path")`** resolves relative to the compile options’ **source path** (typically the real path of the compiling `.leek` file). Missing files or cycles **MUST** fail compilation with **`CompileDiagnostic`** (phase **Resolve** or earlier).

### Implementation note (this repository)

**`include`** expansion and per-statement **origin paths** in HIR are first-class in **this** pipeline; **export parity** tests **SHOULD** cover the same text concatenation semantics where applicable.

---

*Revision: interpreter behavior chapter.*
