# LeekScript fuzzing (`cargo-fuzz`)

One harness (**`pipeline`**) drives strict parse, signature parse, recovery parse, semantic diagnostics, formatting, and VM compile/run (with an operation budget).

## Setup

```bash
cargo install cargo-fuzz
rustup toolchain install nightly
```

Fuzz builds pass `-Z sanitizer=address` and other nightly-only flags; use `cargo +nightly fuzz …` if your default toolchain is stable.

## Run

`cargo-fuzz` looks for `./fuzz/` at the Git workspace root by default. This project keeps fuzzing **next to** `leekscript`, so pass `--fuzz-dir` (from the repository root):

```bash
cargo +nightly fuzz run pipeline --fuzz-dir crates/leekscript-rs/fuzz
```

Or from `crates/leekscript-rs/fuzz/`:

```bash
cargo +nightly fuzz run pipeline --fuzz-dir .
```

### Tips

- Inputs are clamped in the harness to 256 KiB UTF-8 lossy text; raise libFuzzer’s cap to match: `-max_len=262144` (default is often 4096).
- Start with a small cap while iterating: `cargo +nightly fuzz run pipeline --fuzz-dir crates/leekscript-rs/fuzz -- -max_len=8192`
- Replay a crash: `cargo +nightly fuzz run pipeline --fuzz-dir crates/leekscript-rs/fuzz -- -runs=1 path/to/artifact`
- Heavy work (format + VM): `-timeout=10` or higher if libFuzzer reports slow units
- Corpus: `corpus/pipeline/` should stay **handwritten** `seed_*.leek` / `seed_*.sig.leek` only in version control. Running fuzz adds libFuzzer-owned files whose names are 40-character hex digests; those are listed in `.gitignore` so they are not committed again. Extra seeds take ideas from `ai/previous/` in this repo (benchmark phases, maps built in nested loops, entity-like classes, small static helpers)—without copying that project’s sources.
- Optional keyword dictionary (`leek.dict`): language tokens plus **VM stdlib native names** and **prelude globals** (aligned with `leekscript::vm::stdlib`), so mutations often form real calls like `abs(1)` or `TYPE_NUMBER`.  
  `cargo +nightly fuzz run pipeline --fuzz-dir crates/leekscript-rs/fuzz -- -dict=crates/leekscript-rs/fuzz/leek.dict`

### CI smoke

- **Type-check only** (stable Rust, no sanitizer): from `crates/leekscript-rs/fuzz/`, run `cargo build`.
- **One libFuzzer iteration** (needs nightly + `cargo-fuzz`): run `./smoke.sh`.
