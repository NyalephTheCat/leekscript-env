# Performance

**Audience:** contributors optimizing **compile**, **interpret**, or **fight** throughput.

**Scope:** How to measure and regress responsibly. **Out of scope:** SLAs or cloud sizing.

## Benchmarks

- Crate **`leekscript_bench`** — times the Rust interpreter against the reference **Java** runner on matched programs (`AI.export` parity). Requires **JDK** and a built **`leekscript.jar`** in **`leek-wars-generator`** (see crate **`src/main.rs`** module docs). Run with **`cargo run -p leekscript_bench --release -- …`** and **`--help`** for flags.
- **`leek_wars_gen`** experiments and batch runs — profile before micro-optimizing; parity correctness first (see [correctness and parity](../architecture/correctness-and-parity.md)).

## Workflow

1. Establish a **baseline** (commit SHA, `cargo +stable --version`, CPU governor if relevant).
2. Change one thing; compare **median** or stable statistic over multiple iterations.
3. For interpreter/fight changes, run **targeted tests** and parity/fuzz where applicable.

## Policy (lightweight)

- Avoid **large** regressions in default **`lek check`** / **`lek run`** without discussion.
- Document **intentional** trade-offs (e.g. extra checks for parity) in PRs and, if user-visible, in **`docs/`**.

## Related

- [Generator and engine](../architecture/generator-and-engine.md) (throughput-oriented tooling)
- [Fuzzing](../architecture/fuzzing.md) (stress, not primarily perf)

---

*Revision: bench entry points; expand when CI stores trend data.*
