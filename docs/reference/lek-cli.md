# `lek` CLI reference

**Audience:** users and integrators invoking the **LeekScript** command-line tool.

**Scope:** Subcommands, notable flags, exit codes, and JSON output. **Authoritative for every flag:** `cargo run -p lek -- <subcommand> --help` (or the installed binary).

## Subcommands

| Command | Purpose |
|---------|---------|
| **`lek registry`** | Load **`registry.yaml`** (default or **`LEEK_REGISTRY`** / `--path`); optional **`--verify-emit-refs`** to ensure every HIR/interpreter **`reference`** id is registered. |
| **`lek config`** | Validate **`Leek.toml`**; walks up from cwd unless **`--path`**. |
| **`lek init`** | Scaffold **`Leek.toml`** and optional **`example.leek`**; **`--force`** overwrites manifest. |
| **`lek check`** | Directives → lexer → parse → HIR (+ resolve/types); files, dirs of **`*.leek`**, or **`-`** for stdin. |
| **`lek run`** | Same pipeline as **`check`**, then interprets HIR. |
| **`lek fmt`** | Token-based formatter; stdout by default, **`--write`** in place, **`--check`** CI mode. |

## Common flags (check / run)

- **`--manifest`**: explicit **`Leek.toml`** (else discover from cwd).
- **`--language-version`**, **`--strict`**, **`--no-strict`**: override preamble/manifest.
- **`--signatures`**: extra signature TOML (merges with **`[signatures]`** in manifest).
- **`--message-format human|json`**: stderr lines vs **JSON on stdout** (see below).
- **`--stdin-path`**: path label when **`files`** include **`-`** (stdin).

## `fmt` constraints

- Multiple file/dir inputs require **`--write`** or **`--check`** (stdout only supports one logical stream).
- **`--write`** cannot be used with stdin **`-`**.

## Exit codes

| Code | Typical meaning |
|------|-----------------|
| **0** | Success (no compile errors; **`fmt --check`** sees no changes needed). |
| **1** | Compile/diagnostic failure, registry/load failure, IO error, **`fmt`** format error, or **`fmt --check`** would reformat. |
| **2** | **CLI misuse** or invalid input combo (e.g. **`--stdin-path`** without **`-`**, no **`.leek`** files found, **`fmt`** multi-arg without **`--write`/`--check`**, **`fmt --write`** with stdin). |

## JSON output (`check` / `run`)

With **`--message-format json`**, a single JSON object is printed to **stdout** (diagnostics may still drive exit **1**). Both commands use **`schema_version`: `4`** today (bump when shape changes).

Top-level keys include **`schema_version`**, **`command`** (`"check"` or `"run"`), **`files`**, **`diagnostics`**. **`files`** entries report **`status`**: `ok`, `error`, or `io_error`. **`run`** adds interpreted **`result`** when execution succeeds.

## Environment

- **`LEEK_REGISTRY`**: override path to **`registry.yaml`** (see [environment.md](../guides/environment.md)).

## Related

- [Error messages and UX](../engineering/error-messages-and-ux.md)
- [Diagnostics registry](diagnostics-registry.md)
- [Crate `lek`](../../crates/lek)

---

*Revision: operational summary; `--help` remains canonical for full flag lists.*
