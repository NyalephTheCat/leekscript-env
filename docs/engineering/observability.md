# Observability (logging, debug, tracing)

**Audience:** contributors debugging **CLIs**, **interpreter**, or **fight engine**.

**Scope:** What exists **today** for visibility into behavior. **Out of scope:** a full production logging stack (no standard `tracing`/`RUST_LOG` across `lek` yet).

## Current state

| Area | Mechanism |
|------|-----------|
| **`lek`** | Human-oriented messages on **stderr** (`emit_message`, `emit_diagnostic`); **`--message-format json`** puts structured check/run output on **stdout** (see [lek-cli.md](../reference/lek-cli.md)). No `RUST_LOG` integration in `lek` today. |
| **Panics / Rust errors** | Standard Rust backtrace: **`RUST_BACKTRACE=1`** (or `full`) when investigating panics. |
| **Fight pathfinding** | **`LEEK_ASTAR_DEBUG`** — if set (any value), enables extra A* debug behavior in `leek_wars_gen` (see [environment.md](../guides/environment.md)). |
| **`leekgen` simulation** | CLI options for simulation **verbosity** / output style (pretty vs raw, brief vs verbose)—see **`leekgen run --help`** / **`sim --help`**. |
| **`lw_meta`** | HTTP retries and backoff are configurable via env (see [environment.md](../guides/environment.md)); not a structured request log. |

## JSON diagnostics (`lek check` / `lek run`)

Use **`--message-format json`** for machine-readable bundles suitable for tooling or tests. **`schema_version`** is part of that contract—bump it when the JSON shape changes (document in PRs).

## Future improvements

- Optional **`tracing`** + **`RUST_LOG`** for pipeline phases (lex/parse/HIR) behind a feature flag.
- **NDJSON** or stderr **progress** for long `leekgen` batches (coordinate with experiment manifests).

## Related

- [Environment variables](../guides/environment.md)
- [Error messages and UX](error-messages-and-ux.md)

---

*Revision: honest “today vs future” observability; lek stderr/JSON and env debug.*
