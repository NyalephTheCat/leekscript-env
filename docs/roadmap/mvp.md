# MVP vertical slice and follow-on work

This document records **agreed direction** for the first shippable toolchain and what comes next.

---

## MVP (single vertical slice)

**Ship:** **`lek check`** and **`lek fmt`** on one or more `.leek` files in a workspace that may contain **`Leek.toml`**.

**Included in MVP:**

| Layer | Deliverable |
|-------|-------------|
| **Config** | Discover and parse `Leek.toml` ([schema](../design/leek-toml.md)); merge with CLI flags. |
| **Directives** | Resolve file preamble `// leek-*` for `version`, `strict`, basic `fmt` keys ([directives](../design/directives.md)). Local `leek-push`/`leek-pop` can be **stubbed** (parse-only, no effect) if needed to hit the deadline. |
| **Lexer + parser** | Enough to drive **diagnostics** on real snippets (error recovery optional but desirable for IDE). |
| **Diagnostics** | Emit **reference id** + **`E####`** from [`data/diagnostics/registry.yaml`](../../data/diagnostics/registry.yaml); stable JSON or `--message-format` for tests. |
| **Oracle** | **JVM differential** on a **small** curated set of snippets in CI ([parity testing](../design/parity-testing.md)). |
| **Corpus** | **Frozen** diagnostic snapshots for the same snippets (fast PR feedback). |
| **Formatter** | **`lek fmt`** via [`leekscript_fmt`](../../crates/leekscript_fmt) on a lossless **rowan** tree from [`leekscript_syntax`](../../crates/leekscript_syntax) (flat `SOURCE_FILE` + trivia + lex tokens today; grammar nodes next). `[fmt]` + `// leek-fmt:`; refuses invalid lex/delimiter input. |

**Explicitly out of MVP:**

- `lek run` / VM / bytecode
- **Grammar-structured** typed AST (`EXPR` / `STMT` / … nodes under `SOURCE_FILE`) — formatting can become fully syntax-aware once the parser builds nested rowan nodes
- Full LSP (a thin `lek check --json` used by an editor script is acceptable)

**Rationale:** Proves config + directives + dual diagnostic IDs + parity story without committing to execution semantics. Everything else builds on the same driver crate (`leekscript_build`).

---

## Phase 2 (typical order)

1. **CST-backed `lek fmt`** — range formatting and richer style rules once the parser exposes a stable tree; optional `leek-fmt` **regions** (nested overrides).
2. **LSP** — `textDocument/publishDiagnostics` + same pipeline as `lek check`.
3. **`leek run`** or **embedded VM** — behind a feature flag until parity is acceptable.
4. **Full directive semantics** — nested `leek-push`/`leek-pop`, `leek-allow` per line.

---

## Phase 3

- Generator integration (`leek-wars-generator` validation path).
- Optional **JVM bridge** only if native VM lags and CI needs JVM-backed runs.

---

## Workspace bootstrap

The repository root is a workspace for the LeekScript crates. **`lek init`** writes a starter `Leek.toml` (and optional `example.leek`). **`lek check`** runs the **lexer** on `.leek` files, resolves **`E####`** codes via the registry, and reads **`language.version`** from `Leek.toml` when present. **`lek fmt`** applies **`[fmt]`** / **`// leek-fmt:`** and prints reformatted source (or **`--write`** / **`--check`**). Example: `cargo run -p lek -- check tests/fixtures/smoke.leek`, `cargo run -p lek -- fmt tests/fixtures/smoke.leek`.

---

## Document status

Revise this file when MVP scope changes (e.g. if fmt is pulled into MVP).
