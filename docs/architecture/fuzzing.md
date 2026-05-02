# Fuzzing

**Audience:** contributors running or extending **fuzz harnesses**.

**Scope:** **`fuzz/`** (`cargo-fuzz`, **nightly**) vs **stable** fuzz drivers in **`leekgen-compare`**. **Non-goals:** CI configuration for fuzz (not yet in root workflows — see [ci-and-quality-gates.md](../operations/ci-and-quality-gates.md)).

## Workspace layout

The **`fuzz/`** directory is a **separate Cargo package** (`leekscript-env-fuzz`) and is **`exclude`**d from the root workspace `Cargo.toml`. Run commands from **`fuzz/`** with **`cargo +nightly fuzz …`**.

## Nightly targets (`fuzz/fuzz_targets/`)

| Target | Role |
|--------|------|
| **`leekscript_toolchain`** | Arbitrary UTF-8 (cap ~64 KiB) → `leekscript_fuzz::source_parses_any_version`; if it parses, runs **`leekscript_run::compile_source`**. Exercises lexer/parser/HIR/resolve on mutated sources. |
| **`leek_wars_gen_rust_engine`** | Scenario JSON from **`leek-wars-generator`** corpus + **`FuzzInput`** mutations (stats, cells, map, turns, …); runs **`run_scenario_path`** / AI overlay. Needs **`LEEK_GENERATOR_CWD`** or in-repo default path to **`../leek-wars-generator`**. |
| **`leekgen_compare_repro`** | **Minimization** of an existing repro bundle: requires **`LEEKGEN_REPRO_DIR`** and **`LEEK_GENERATOR_CWD`**. Panic ⇒ repro still fails so libFuzzer shrinks input. |

Dependencies (see **`fuzz/Cargo.toml`**): **`libfuzzer-sys`**, **`leekscript_fuzz`**, **`leekscript_run`**, **`leek_wars_gen`**.

### Setup

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run leekscript_toolchain
```

Corpus seeds may live under **`fuzz/corpus/<target>/`**.

## Stable / RNG fuzz (`leekgen-compare`)

On **stable Rust**, long-running parity fuzzing can use the built-in driver (no libFuzzer):

```bash
cargo run -p leek_wars_gen --bin leekgen-compare -- --fuzz --fuzz-parity --fuzz-n 100
```

This path is documented in the root **README.md**; implementation lives in **`compare_fuzz_cli`** + **`fuzz`** modules inside **`leek_wars_gen`**.

## Env vars (repro and engine fuzz)

| Variable | Used by |
|----------|---------|
| **`LEEK_GENERATOR_CWD`** | Rust engine fuzz target, repro replay, scenario AI resolution |
| **`LEEKGEN_REPRO_DIR`** | `leekgen_compare_repro` minimization |

See [environment.md](../guides/environment.md).

## Related

- [correctness-and-parity.md](correctness-and-parity.md)
- [generator-and-engine.md](generator-and-engine.md)
- [local-development.md](../guides/local-development.md)

---

*Revision: fuzz architecture; update when new `[[bin]]` targets are added.*
