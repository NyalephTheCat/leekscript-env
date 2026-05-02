# File preamble directives (`// leek-*`)

**Audience:** authors of `.leek` files and tooling that runs before the main lexer.

**Scope:** Line-comment directives at the **start of a file**, as implemented in **`leekscript_directives`**. **Canonical implementation:** `crates/leekscript_directives` (`parse_file_preamble`). **Normative language rules** will eventually live in **`docs/spec/12-directives-and-pragmas.md`**; this page is the **operational** reference.

**Historical note:** Older docs pointed at `docs/design/directives.md`. That path is **retired**; content lives here under **`docs/reference/`**.

## Scan window

- Only **leading** lines count: blank lines, `//` line comments, and `/* … */` block comments (including multi-line blocks).
- Scan stops at the first line that is **not** empty, not a comment, or not entirely inside a block comment.
- At most **`max_lines`** physical lines are examined (**64** in `lek` / `leekscript_run`, via `PREAMBLE_MAX_LINES`).
- Directives **after** real code are **ignored** (preamble ended).

## Syntax

- Form: **`// leek-<name>[:|=] <value>`** (colon or equals after the name).
- The prefix must be **`// leek-`** (after whitespace on the line). Unknown `leek-*` names produce a diagnostic (`unknown_leek_directive`).

## Supported directives

| Directive | Value | Effect |
|-----------|--------|--------|
| **`leek-version`** | Integer **1–99** | Sets language version for this file (CLI / manifest may still override per pipeline rules). |
| **`leek-strict`** | Optional: `true` / `false` / `1` / `0` / `yes` / `no`. Omitted value ⇒ **true**. | Strictness flag for tooling. |
| **`leek-fmt`** | Comma-separated **`key=value`** | Formatter hints: **`width`**, **`indent`**, **`tab_width`**, **`use_tabs`** (see below). Used by **`lek fmt`** / future LSP; **does not** change `lek check` lexing. |
| **`leek-experimental`** | Comma-separated feature names | Stored for tooling; **does not** change lexing. Must list at least one non-empty name. |

### `leek-fmt` keys

| Key | Type |
|-----|------|
| `width`, `indent`, `tab_width` | Positive integer |
| `use_tabs` | Boolean literal (`true` / `false`, etc.) |

### Ignored names (no diagnostic)

These are recognized and **silently ignored** (reserved / forward-compatible): **`allow`**, **`push`**, **`pop`**.

## Diagnostics

Invalid values use registry id **`leek_directive_invalid_value`**. Unknown directive names use **`unknown_leek_directive`**. See [diagnostics-registry.md](diagnostics-registry.md).

## Relation to `Leek.toml`

Manifest defaults and formatter settings live in **`Leek.toml`**; preamble can narrow or override **language** / **fmt** hints per file where the pipeline applies preamble results. See [leek-toml.md](leek-toml.md) and `CompileOptions` / `FmtPreamble` usage in `leekscript_run`.

## Related

- [rust-toolchain-crates.md](../architecture/rust-toolchain-crates.md) — pipeline phase **Directives**
- [leek-toml.md](leek-toml.md)

---

*Revision: operational reference; keep in sync with `leekscript_directives::parse_file_preamble`.*
