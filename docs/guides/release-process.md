# Release process (workspace)

**Audience:** maintainers and contributors who touch **version-sensitive** surfaces (registry, spec appendices, submodule pins).

**Scope:** A practical **checklist** aligned with today’s **Git-first** distribution. **Non-goals:** product release management for Leek Wars or upstream Java generator—record their revisions in PR descriptions or **[external repositories](../overview/external-repositories.md)** when relevant.

## Policy context

Read **[Release & versioning](../operations/release-and-versioning.md)** first: the workspace is **Git-only** for the short and medium term, **`publish = false`** on members, and meaningful integration anchors are **commits** (and submodule pointers), not semver on `0.1.0` placeholders.

This guide is **how to behave** when preparing a change set that others will pin; it is not a crates.io release runbook.

## Checklist (before merging or announcing a “snapshot”)

1. **CI** — [CI & quality gates](../operations/ci-and-quality-gates.md): formatting, clippy, tests, registry verify, dependency checks as configured.
2. **`data/` and diagnostics** — if registry YAML or diagnostic codes changed, ensure [Diagnostics registry](../reference/diagnostics-registry.md) intent still holds and regenerate spec appendices when applicable (`python3 scripts/gen_spec_appendices.py`; see [spec README](../spec/README.md)).
3. **Signatures / builtins** — if `data/signatures/` or builtin contracts moved, regen appendix F and align [spec ch. 11](../spec/11-builtins-and-api-surface.md) / [registry ops](../reference/registry.md).
4. **Submodules / parity** — if submodule revisions or generator expectations changed, note that in the PR and any affected **[correctness and parity](../architecture/correctness-and-parity.md)** discussion.

## Optional: Git tags

When maintainers adopt **annotated tags** (e.g. `v0.2.0`), extend [Release & versioning](../operations/release-and-versioning.md) with the exact tagging steps. Until then, **document the commit SHA** in release notes or PR descriptions when a consumer needs a fixed point.

## Related

- [Contributing](contributing.md) — PR expectations.
