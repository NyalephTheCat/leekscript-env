# Diagnostic registry

- **`registry.yaml`** — Maps Java `leekscript.common.Error` variants to stable **`E####`** codes, plus toolchain-only entries (`E7001`, `E7201`, `E7202`, …). Policy: [`docs/design/diagnostic-codes.md`](../docs/design/diagnostic-codes.md).

Regenerate only when the Java enum changes; never reassign existing codes.
