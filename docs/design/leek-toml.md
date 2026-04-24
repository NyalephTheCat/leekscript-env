# `Leek.toml` schema (v1)

Project-level configuration for `lek`, the LSP, and CI. **File-level** and **local** `// leek-*` directives override these values per the precedence in [directives](directives.md#precedence).

**Location:** Search upward from a source file for `Leek.toml`; workspace roots may pin a single manifest (exact search rules live in `leekscript_config`).

---

## Format

- **Format:** [TOML v1](https://toml.io/en/v1.0.0).
- **Encoding:** UTF-8.
- **Optional** top-level key: `package` (table) for name/version metadata; tools may ignore it.

---

## Top-level keys (v1)

### `[language]`

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `version` | integer | `4` | Language version (`LeekScript.LATEST_VERSION` semantics). |
| `strict` | boolean | `false` | [Strict mode](../spec/leekscript-language.md#4-strict-mode) for analysis. |

### `[fmt]`

Formatter defaults for `lek fmt`. Keys are **open-ended**; unknown keys are **warnings** unless `lek` is run with `--deny-unknown-keys`.

| Key | Type | Example | Description |
|-----|------|---------|-------------|
| `width` | integer | `100` | Preferred line width. |
| `indent` | integer | `4` | Indentation width (spaces). |
| `tab_width` | integer | `4` | Tab display width if tabs appear. |
| `use_tabs` | boolean | `false` | Prefer tabs vs spaces (policy TBD by formatter). |

### `[lint]`

| Key | Type | Description |
|-----|------|-------------|
| `level` | string | Global floor: `"allow"` \| `"warn"` \| `"deny"` (default `"warn"`). |
| `deny` | array of string | Stable codes or glob: `E3xxx`, `E####`, or groups (TBD). |
| `allow` | array of string | Suppress these codes even if `level` would report them. |

### `[experimental]`

| Key | Type | Description |
|-----|------|-------------|
| `features` | array of string | Feature flags (same names as `// leek-experimental:`). Default empty. |

---

## Example

```toml
[package]
name = "my-ai"
version = "0.1.0"

[language]
version = 4
strict = false

[fmt]
width = 100
indent = 4

[lint]
level = "warn"
allow = ["E8000"]

[experimental]
features = []
```

---

## Diagnostics

Invalid TOML or invalid key types → **`E7001`** (`invalid_leek_toml`) per [`data/diagnostics/registry.yaml`](../../data/diagnostics/registry.yaml).

---

## Versioning

Bump **`schema_version`** inside the file when breaking schema changes occur (field renames, required keys). v1 omits `schema_version`; treat absence as `1`.
