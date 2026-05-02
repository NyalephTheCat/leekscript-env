# Error messages and UX

**Audience:** implementers emitting **diagnostics** and anyone polishing **CLI** output.

**Scope:** Conventions for **`lek`** (and similar tools). **Out of scope:** in-game Leek Wars UI copy.

## Stable identifiers

- **`E####`:** primary user-facing **code**; must exist in **`data/diagnostics/registry.yaml`** when used in shipped paths. See [diagnostics-registry.md](../reference/diagnostics-registry.md).
- **`reference`:** stable id for tooling and `verify-emit-refs`; pair with `E####` in the registry.

Human-readable **`message`** text may evolve; codes and references should not change meaning without a documented migration (PR description + updates to **`docs/reference/`** as needed).

## Human vs JSON

- **Human (`--message-format human` default):** line-oriented stderr, spans when available—keep messages **actionable** (what failed, where).
- **JSON (`--message-format json`):** one object on stdout for `check`/`run` with **`schema_version`**, **`files`**, **`diagnostics`**—stable field names matter for CI; see [lek-cli.md](../reference/lek-cli.md).

## Wording guidelines

- Lead with **what** is wrong, then **why** if non-obvious.
- Avoid leaking **secrets** (paths may contain usernames; don’t echo tokens).
- Prefer **consistent** terms with [spec](../spec/README.md) and [glossary](../overview/glossary.md).

## Internationalization

- **Not implemented** today; messages are **English**. If i18n is added later, keep **`E####`** / **`reference`** locale-independent.

## Related

- [Observability](observability.md)
- [Spec appendix C — diagnostic mapping](../spec/appendices/C-diagnostic-codes-mapping.md)

---

*Revision: diagnostic stability + human/JSON split.*
