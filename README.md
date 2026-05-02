# leekscript-env

Workspace for the LeekScript toolchain (`lek`) and supporting libraries.

**Documentation:** [docs/README.md](docs/README.md) (index). **`lek` CLI:** [lek-cli.md](docs/reference/lek-cli.md). **Crate paths:** [docs/crates/README.md](docs/crates/README.md). Contributor setup: [local development](docs/guides/local-development.md), [environment variables](docs/guides/environment.md), [contributing](docs/guides/contributing.md). **Releases:** [release & versioning](docs/operations/release-and-versioning.md).

## Build

```bash
cargo build
cargo run -p lek -- init              # create Leek.toml (+ example.leek) in the current directory
cargo run -p lek -- registry --verify-emit-refs   # registry + all HIR/interpreter refs covered
cargo run -p lek -- config          # validate Leek.toml (walks up from cwd)
cargo run -p lek -- check tests/fixtures/smoke.leek   # same pipeline as `lek run` (parse + HIR)
cargo run -p lek -- run tests/fixtures/smoke.leek     # compile + interpret HIR
cargo run -p lek -- fmt tests/fixtures/smoke.leek     # format (stdout; use --write to save)
```

Override registry path: `LEEK_REGISTRY=/path/to/registry.yaml`.

## Generator CLI (`leekgen`, Rust-only)

`leekgen` runs the in-tree Rust fight engine (no JVM). It supports JSON **and** TOML scenarios.

```bash
# Run a scenario (pretty summary)
cargo run -p leek_wars_gen --bin leekgen -- run leek-wars-generator/test/scenario/scenario1.json

# Print raw outcome JSON
cargo run -p leek_wars_gen --bin leekgen -- run leek-wars-generator/test/scenario/scenario1.json --output json

# Readable playback derived from outcome JSON
cargo run -p leek_wars_gen --bin leekgen -- run leek-wars-generator/test/scenario/scenario1.json --sim

# Or use the simulation alias:
cargo run -p leek_wars_gen --bin leekgen -- sim leek-wars-generator/test/scenario/scenario1.json --group-turns --diff --limit 50

# Scenario utilities
cargo run -p leek_wars_gen --bin leekgen -- scenario list --root leek-wars-generator
cargo run -p leek_wars_gen --bin leekgen -- scenario validate leek-wars-generator/test/scenario --recursive
cargo run -p leek_wars_gen --bin leekgen -- scenario convert leek-wars-generator/test/scenario/scenario1.json --to toml

# Rust-only fuzz runner (scenario pool + optional AI mutation)
cargo run -p leek_wars_gen --bin leekgen -- fuzz --root leek-wars-generator --n 100
```

### `Leek.toml` defaults for `leekgen`

`leekgen` reads `[generator]` from the nearest `Leek.toml` found by walking up from the current directory.

```toml
[generator]
generator_root = "leek-wars-generator"
scenarios_dir = "leek-wars-generator/test/scenario"
ai_dir = "leek-wars-generator/test/ai"
output = "pretty" # or "json" / "ndjson"
```

## Fuzzing

- **Stable Rust**: use the built-in RNG fuzz driver (no sanitizers / libFuzzer).
  - This is the recommended way to do long-running parity fuzzing against the official generator on stable.

```bash
cargo run -p leek_wars_gen --bin leekgen-compare -- --fuzz --fuzz-parity --fuzz-n 100
```

- **Toolchain fuzz (LeekScript parser/compiler + syntax-aware mutator)**:

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
cargo +nightly fuzz run leekscript_toolchain
```

- **Rust fight engine fuzz (scenario JSON jitter + optional AI overlay)**:

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
cargo +nightly fuzz run leek_wars_gen_rust_engine
```

- **Minimize an existing `leekgen-compare --fuzz` repro bundle** (requires env vars):
  - **`LEEKGEN_REPRO_DIR`**: artifact directory containing `scenario.json` + `meta.json`
  - **`LEEK_GENERATOR_CWD`**: generator checkout root

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
LEEKGEN_REPRO_DIR=/path/to/artifact \
LEEK_GENERATOR_CWD=/path/to/leek-wars-generator \
  cargo +nightly fuzz run leekgen_compare_repro
```

## Crates

| Crate | Role |
|-------|------|
| `lek` | CLI binary |
| `leekscript_span` | Byte spans, line/column |
| `leekscript_lexer` | Lexer (Java `LexicalParser` order) |
| `leekscript_diagnostics` | `E####` registry loader |
| `leekscript_config` | `Leek.toml` parsing and validation |
| `leekscript_fmt` | Formatter (`lek fmt`) |
| `leekscript_hir` | Lowered IR after parse |
| `leekscript_resolve` | Lexical scope check (`VARIABLE_NOT_EXISTS`, …) |
| `leekscript_run` | `compile_source` + `interpret_hir` pipeline |
| `leekscript_syntax` | Rowan red–green tree (`SOURCE_FILE` + trivia + lex tokens); typed root `ast::SourceFile` (nested grammar nodes later) |

See `docs/architecture/rust-toolchain-crates.md` for the planned crate graph.
