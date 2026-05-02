# Registry operations (diagnostic coverage)

**Audience:** maintainers validating that **toolchain reference ids** and **`E####`** codes stay in sync.

**Scope:** Operational commands around **`data/diagnostics/registry.yaml`**. **YAML schema and policy:** [diagnostics-registry.md](diagnostics-registry.md). **Builtin / API *semantics*** belong in **`docs/spec/`** (chapter 11) and machine-readable **`data/`** as those docs grow.

## Verify emitted references

Ensures every reference string the **`lek` check/run** pipeline can emit is present in the registry:

```bash
cargo run -p lek -- registry --verify-emit-refs
```

Run this after adding lexer, HIR, resolve, type, or interpreter diagnostics. See [contributing.md](../guides/contributing.md).

## Builtin and HIR “coverage”

The meta documentation program may add **explicit coverage reports** (e.g. builtins vs `data/` YAML). Until documented here, **tests** and **`--verify-emit-refs`** are the main automated gates.

## Related

- [diagnostics-registry.md](diagnostics-registry.md)
- [data-and-fixtures.md](../architecture/data-and-fixtures.md)

---

*Revision: operational stub; expand when `lek registry` gains subcommands or reports.*
