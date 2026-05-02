# `Leek.toml` reference

**Audience:** project authors and tool maintainers.

**Scope:** Top-level sections validated by **`leekscript_config`** and **`lek config`**, plus the **`[generator]`** table consumed by **`leek_wars_gen`**. This describes **tooling configuration**, not the LeekScript language.

**Implementation:** `crates/leekscript_config` (`LeekManifest`, validation). Unknown top-level keys are **rejected**.

## Discovery

`Leek.toml` is found by walking **upward** from the current working directory (or from a path passed to `lek config`). Same discovery is used when the compile pipeline needs manifest-backed **language** settings.

## Allowed top-level keys

Exactly these names are permitted at the root (see `ALLOWED_TOP_LEVEL` in `leekscript_config`):

| Key | Type | Purpose |
|-----|------|---------|
| **`schema_version`** | integer | Optional. Only **`1`** is accepted if present; other values error. |
| **`package`** | table | Opaque package metadata (not deeply validated by `leekscript_config`). |
| **`language`** | table | Default language version and strictness (see below). |
| **`fmt`** | table | Formatter options (see below). |
| **`lint`** | table | Lint level and allow/deny lists (see below). |
| **`experimental`** | table | Experimental feature flags list. |
| **`signatures`** | table | Optional path to extra signature TOML (see below). |
| **`generator`** | table | **`leekgen`** paths and output mode (see below). Parsed as generic TOML by `leek_wars_gen`; keys are **not** validated by `leekscript_config` at manifest load time. |

## `[language]`

| Field | Type | Validation |
|-------|------|------------|
| **`version`** | integer | If set, must be in **1–99** (inclusive). |
| **`strict`** | boolean | Optional. |

CLI flags on `lek check` / `lek run` can override manifest values where supported.

## `[fmt]`

Known keys validated today:

| Key | Type | Rule |
|-----|------|------|
| **`width`**, **`indent`**, **`tab_width`** | integer | Must be **positive** if present. |
| **`use_tabs`** | boolean | — |

Other keys are accepted without extra validation (forward-compatible).

## `[lint]`

| Field | Type | Rule |
|-------|------|------|
| **`level`** | string | If set, must be **`allow`**, **`warn`**, or **`deny`**. |
| **`deny`**, **`allow`** | array of strings | Optional lists of diagnostic codes (e.g. `E8000`). |

## `[experimental]`

| Field | Type |
|-------|------|
| **`features`** | array of strings |

## `[signatures]`

| Field | Type |
|-------|------|
| **`path`** | string — path to a signatures file, **relative to the manifest directory** unless absolute. |

Used to pre-declare globals/functions for **`leekscript_resolve`** (e.g. Leek Wars AI surface).

## `[generator]` (`leekgen`)

Read by **`leek_wars_gen`** when resolving its config. Precedence for the **generator root** is:

1. Explicit **CLI** root (if the subcommand provides it)
2. **`LEEK_GENERATOR_CWD`**
3. **`generator_root`** in this table (path relative to manifest directory unless absolute)
4. Default: **current working directory** (typically workspace root when using `cargo run`)

| Key | Type | Default when omitted |
|-----|------|----------------------|
| **`generator_root`** | string (path) | CWD-based resolution above |
| **`scenarios_dir`** | string (path) | `<root>/test/scenario` |
| **`ai_dir`** | string (path) | `<root>/test/ai` |
| **`output`** | string | **`pretty`** — also accepted: **`json`**, **`ndjson`** |

Paths in this section are resolved **relative to the directory containing `Leek.toml`** unless absolute.

### Example

```toml
[generator]
generator_root = "leek-wars-generator"
scenarios_dir = "leek-wars-generator/test/scenario"
ai_dir = "leek-wars-generator/test/ai"
output = "pretty"
```

## Preamble directives

Per-file **`// leek-*`** lines (version, strict, fmt hints, experimental flags) are documented in **[directives.md](directives.md)**. They are applied in the **Directives** phase before lexing.

## Related

- [directives.md](directives.md)
- [environment.md](../guides/environment.md) — `LEEK_GENERATOR_CWD`
- [data-and-fixtures.md](../architecture/data-and-fixtures.md) — bundled signatures under `data/signatures/`
- Root [Leek.toml](../../Leek.toml) — example manifest in this repo

---

*Revision: reference aligned with `leekscript_config` + `leek_wars_gen::config`; update if validation or keys change.*
