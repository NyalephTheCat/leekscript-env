# Environment and shell configuration

**Audience:** contributors who hit linker issues, need env overrides, or want reproducible shells.

**Scope:** Workspace **`/.envrc`**, **`.cargo/config.toml`**, and **environment variables** read by crates. **Non-goals:** secrets for production services (only generic notes for local API use).

## direnv (optional)

If you use [direnv](https://direnv.net/), the repo ships **`.envrc`** at the workspace root. Today it sets **`RUSTFLAGS`** for linker selection (workaround for some Nix / `gcc-wrapper` setups):

```bash
export RUSTFLAGS="-C linker=gcc -C link-arg=-fuse-ld=bfd"
```

You can ignore direnv if your default `cargo` link already works. Comments in `.envrc` reference **`.config/config.toml`** for additional local tuning.

## Workspace Cargo config

**`.cargo/config.toml`** adds **`rustflags`** using **BFD ld** (`-fuse-ld=bfd`) for the workspace build. That merges with **`RUSTFLAGS`** from the environment. If you see link failures, compare with [local-development.md](local-development.md) and your global `~/.cargo/config.toml`.

## Environment variables (reference)

| Variable | Used by | Purpose |
|----------|---------|---------|
| **`LEEK_REGISTRY`** | `lek`, `leekscript_diagnostics` path | Path to **`registry.yaml`** instead of default `data/diagnostics/registry.yaml` (or bundled path resolution from the binary). |
| **`LEEK_GENERATOR_CWD`** | `leek_wars_gen`, fuzz repro, Java engine | Root of **leek-wars-generator** checkout: AI paths, `data/*.json`, `generator.jar` discovery. |
| **`LEEK_GENERATOR_JAR`** | `leek_wars_gen` | Explicit path to **`generator.jar`** (overrides relative search). |
| **`LEEKGEN_REPRO_DIR`** | `fuzz` target `leekgen_compare_repro` | Directory with **`scenario.json`** + **`meta.json`** for minimization. |
| **`JAVA_HOME`** | Parity / harness tests, benchmarks | JDK for spawning **`java`** when comparing to the official generator. |
| **`LEEKSCRIPT_TEST_RESOURCES`** | `leekscript_run` Java VM suite tests | Override directory for LeekScript test resources (default: `leek-wars-generator/leekscript/src/test/resources` under repo). |
| **`LEEK_ASTAR_DEBUG`** | `leek_wars_gen` fight pathfinding | If set (any value), enables debug behavior for A* troubleshooting. |
| **`LEEKWARS_API_BASE`** | `lw_meta` / `lw-meta` CLI | API base URL (default `https://leekwars.com/api/`). |
| **`LEEKWARS_TOKEN`** | `lw_meta` | Bearer token for authenticated endpoints (e.g. service catalog). **Secret** — do not log or commit. |
| **`LEEKWARS_MAX_ATTEMPTS`**, **`LEEKWARS_BACKOFF_INITIAL_MS`**, **`LEEKWARS_BACKOFF_MAX_MS`**, **`LEEKWARS_REQUEST_GAP_MS`** | `lw_meta` | Retry and pacing for 429/503 and multi-page fetches. |

Other HTTP clients may use standard **proxy** or **TLS** env vars. **Secrets:** do not commit **`LEEKWARS_TOKEN`**; use env or your team’s secret store. See crate **[`lw_meta`](../../crates/lw_meta)** for HTTP client behavior.

## Future improvements

- Centralize this table in **`docs/engineering/observability.md`** if **`RUST_LOG`** / tracing becomes standard for CLIs.

---

*Revision: `lw_meta` env vars and secrets handling.*
