# External Git submodules (reference)

**Audience:** anyone cloning the repo and wondering **why these trees exist** and **when they are required**.

**Scope:** **`leek-wars/`**, **`leek-wars-generator/`**, and **`ai/`** as **submodules**. **Canonical policy:** [project charter](project-charter.md) — they are **not** Cargo dependencies of the core toolchain; they support **parity**, **fixtures**, **meta/data**, and **experiments**.

## Submodule list (see `.gitmodules`)

| Path | Upstream role | Typical uses in this workspace |
|------|----------------|--------------------------------|
| **`leek-wars-generator/`** | Official **Java generator**, **`generator.jar`**, **`test/scenario`**, **`test/ai`**, LeekScript Java tests / resources | Scenario JSON, AI `.leek` paths, JVM parity, **`LEEK_GENERATOR_CWD`** resolution, Java VM export corpus paths |
| **`leek-wars/`** | Game / web app source | Reference context; occasional scripts or assets — **not** compiled into `lek` / `leekgen` |
| **`ai/`** | Separate AI / experiment assets | Fuzz or experiment overlays when configured |

## What is *not* implied

- You do **not** need submodules for every **`cargo test`** (many crate tests are self-contained).
- You **do** need **`leek-wars-generator`** checked out for: scenario paths in docs, **`leekgen`** examples that point at `test/scenario`, JVM compare tests, and **`LEEKSCRIPT_TEST_RESOURCES`** defaults that point into the generator tree.

```bash
git submodule update --init --recursive
```

## Documentation elsewhere

- [repository-map.md](repository-map.md) — top-level layout
- [scenario-format.md](../reference/scenario-format.md) — scenario JSON/TOML
- [environment.md](../guides/environment.md) — `LEEK_GENERATOR_CWD`, `JAVA_HOME`
- Upstream product docs: each submodule’s own **README** (not duplicated here)

---

*Revision: integration overview; not a mirror of upstream documentation.*
