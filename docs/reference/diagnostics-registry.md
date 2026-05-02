# Diagnostics registry (`registry.yaml`)

**Audience:** contributors adding diagnostics, reviewers checking stable codes.

**Scope:** The YAML file under **`data/diagnostics/registry.yaml`**, how **`lek`** loads it, and **policy** for **`E####`** stability. **Non-goals:** full mapping of every error message string (see crate emit sites and tests).

## Location and overrides

- **Default:** `data/diagnostics/registry.yaml` relative to the workspace (or path resolved next to the built `lek` binary per loader rules — see `crates/lek/src/check.rs` and `leekscript_diagnostics`).
- **Override:** set **`LEEK_REGISTRY`** to an alternate YAML file (see [environment.md](../guides/environment.md)).

## Schema (version 1)

Loaded by **`leekscript_diagnostics::Registry`**:

| Field | Meaning |
|-------|---------|
| **`schema_version`** | Must be **`1`**. |
| **`reference_source`** | Informative path to the upstream Java **`Error`** enum (traceability). |
| **`entries`** | List of mappings. |

Each **entry** may include:

| Field | Meaning |
|-------|---------|
| **`code`** | Stable **`E####`** string (required for normal entries). |
| **`reference`** | Java / semantic **`Error`** name (e.g. `OPENING_PARENTHESIS_EXPECTED`). |
| **`id`** | Toolchain-only string id (e.g. `unknown_leek_directive`) when there is no Java `reference`. |
| **`band`** | Optional grouping (parse, include, user_fn_decl, …) for organization. |

**Duplicate `code` values** are rejected at load time.

## Operational checks

```bash
cargo run -p lek -- registry --verify-emit-refs
```

This ensures every **reference** id emitted by the HIR/interpreter (and related lists maintained in `lek`) appears in the registry. Run after adding new lexer, resolve, type, or interpreter diagnostics.

## Policy

- **Never reassign** an existing **`E####`** to a different meaning.
- Add **new** rows for new Java `Error` variants or new toolchain references.
- Regeneration from upstream should be done **carefully**; the header comment in `registry.yaml` records the Java source path.

## Related

- [data-and-fixtures.md](../architecture/data-and-fixtures.md)
- [contributing.md](../guides/contributing.md)
- Language spec appendix **[C — Diagnostic codes mapping](../spec/appendices/C-diagnostic-codes-mapping.md)** (generated table; run `python3 scripts/gen_spec_appendices.py` after YAML changes)

---

*Revision: operational registry reference; YAML remains source of truth.*
