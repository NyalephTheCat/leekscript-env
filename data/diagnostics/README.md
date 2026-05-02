# Diagnostic registry

- **`registry.yaml`** — Maps Java `leekscript.common.Error` variants to stable **`E####`** codes, plus toolchain-only entries (`id` field). Operational reference: **[docs/reference/diagnostics-registry.md](../../docs/reference/diagnostics-registry.md)**.

Regenerate only when the Java enum changes; **never reassign** existing codes.
