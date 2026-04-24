# Toolchain directives (`// leek-*`)

Line comments interpreted by `lek`, LSP, and formatter—not by the Java reference lexer. Authoritative numbering for toolchain-only diagnostics: **`E7200–E7399`** in [diagnostic-codes](diagnostic-codes.md).

For language semantics, see [language spec §11](../spec/leekscript-language.md#11-source-directives-rust-toolchain) (section title may differ; search “directives” in the spec).

---

## Syntax

- After `//`, optional spaces, then `leek-` prefix.
- **Key–value:** `// leek-<name>: <value>` or `// leek-<name> = <value>`.
- **Flag:** `// leek-<name>`.

---

## Scopes

| Scope | Mechanism | Typical keys |
|-------|-----------|--------------|
| **Config** | [`Leek.toml`](leek-toml.md) | `version`, `strict`, `fmt.*`, `lint.*`, `experimental.features` |
| **File** | Preamble at top of file (recommended: first 64 lines or before first non-comment token) | `leek-version`, `leek-strict`, `leek-fmt`, `leek-experimental` |
| **Local** | `// leek-push:` … `// leek-pop` stack, or line-scoped allows | `leek-fmt` overrides, `leek-allow: E####`, future lint toggles |

**Language mode** (`leek-version`, default strictness): **config + file only** unless a future RFC explicitly allows mid-file changes.

---

## Precedence

Narrowest wins for a given key (see [architecture doc](../architecture/rust-toolchain-crates.md#cli--directives-precedence-recommended)):

1. CLI  
2. Innermost open **local** region (`leek-push` / `leek-pop`)  
3. **File** preamble  
4. **`Leek.toml`**  
5. Toolchain defaults  

---

## Common directives

| Directive | Example | Scope |
|-----------|---------|--------|
| `leek-version` | `// leek-version: 4` | config, file |
| `leek-strict` | `// leek-strict: true` | config, file |
| `leek-fmt` | `// leek-fmt: width=80, indent=2` | config, file, local |
| `leek-experimental` | `// leek-experimental: foo, bar` | config, file |
| `leek-allow` | `// leek-allow: E3006` (line or previous line) | local (lint) |
| `leek-push` / `leek-pop` | `// leek-push: leek-fmt=off` … `// leek-pop` | local |

Unknown directive name → **`E7201`** (`unknown_leek_directive`). Invalid value → **`E7202`** (`leek_directive_invalid_value`).

---

## Push/pop grammar (illustrative)

```text
// leek-push: leek-fmt=off
// … verbatim region …
// leek-pop
```

Nested pushes stack; each `pop` restores the previous layer.

---

## Reference compiler

The Java lexer **discards** `//` comments; directives are **no-ops** for `javac`/Gradle unless the Java toolchain gains pragma support.
