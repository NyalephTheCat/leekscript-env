# Fight scenario format (JSON / TOML)

**Audience:** authors of scenarios for **`leekgen`** and integrators comparing runs to the Java generator.

**Scope:** The document shape accepted by **`leek_wars_gen`** (`Scenario` in `crates/leek_wars_gen/src/scenario.rs`), file loading (**`scenario_io`**), and validation. This is **not** the full upstream Java `Scenario` specification—see the **`leek-wars-generator`** tree for reference fixtures.

**Implementation:** `Scenario::from_path` expects **JSON bytes** today; **`.toml`** files are converted to JSON in a **temp file** via `materialize_json_path` (TOML → `serde_json::Value` → JSON). **`load_value`** accepts both extensions.

## File types

| Extension | Loading |
|-----------|---------|
| **`.json`** | Parsed as JSON. |
| **`.toml`** | Parsed as TOML, converted to JSON value. **`null`** cannot round-trip through TOML (conversion errors if present). |

Minimal validation (`validate_value`) requires top-level **`farmers`**, **`teams`**, and **`entities`**, each an **array**.

## Root object (`Scenario`)

| Field | Type | Notes |
|-------|------|--------|
| **`farmers`** | array of **`FarmerInfo`** | Required (validation). |
| **`teams`** | array of **`TeamInfo`** | Required. |
| **`entities`** | array of **arrays** of **`EntityInfo`** | Required. Each inner array is one side’s leeks (typical 1v1: two inner arrays). |
| **`max_turns`** | integer | Default **64** if omitted (`default_max_turns`). |
| **`random_seed`** | integer, optional | **Set explicitly** for reproducible fights. **`run_scenario_path`** parses JSON without `Scenario::from_path`; if the field is absent, the engine uses seed **`1`** (`FightWorld::from_scenario`). **`from_path`** may inject a **time-derived** seed when missing—see [correctness-and-parity.md](../architecture/correctness-and-parity.md). |
| **`draw_check_life`** | boolean | Default false. Tie-break behavior when no unique surviving team. |
| **Other keys** | any | Stored in **`Scenario::extra`** (e.g. **`map`**, **`max_operations_per_entity`**) for parity and tooling. |

### `FarmerInfo`

`id` (i32), `name` (string), `country` (string).

### `TeamInfo`

`id` (i32), `name` (string).

### `EntityInfo` (selected fields)

Combat and AI-relevant fields include: **`id`**, **`name`**, **`type`**, **`level`**, **`life`**, **`tp`**, **`mp`**, stats (**`strength`**, **`agility`**, **`wisdom`**, **`resistance`**, **`science`**, **`magic`**, **`frequency`**), **`cores`**, **`ram`**, **`farmer`**, **`team`**, equipment (**`weapons`**, **`chips`**, **`components`** — ids or `{ "id", "template" }` shapes per deserializers), **`ai`** (string path to **`.leek`** relative to generator root), **`ai_path`**, **`ai_folder`**, **`ai_version`**, **`ai_strict`**, **`cell`**, optional **`total_*`** stats (API-shaped exports), **`hat`** (integer or `{ "template": … }`), etc. See **`EntityInfo`** in `scenario.rs` for the full list and defaults.

## `map` (in `extra`)

Official scenarios often include **`map`** with **`width`**, **`height`**, **`type`**, **`obstacles`**. The Rust **`Scenario::map_size()`** reads width/height from `extra["map"]`, defaulting to **(17, 17)**. **`engine_map_size_java_main()`** documents a **fixed (18, 18)** grid used when mirroring certain **`Main`** jar paths. **`map_obstacles()`** expects **`map.obstacles`** as a **JSON object** mapping cell id strings to values; other obstacle shapes may appear in raw JSON without being interpreted.

## AI paths

**`ai`** strings (e.g. `test/ai/basic.leek`) are resolved relative to the **generator root** (`LEEK_GENERATOR_CWD`, `[generator].generator_root`, or CLI). See [leek-toml.md](leek-toml.md) and [environment.md](../guides/environment.md).

## CLI utilities

```bash
cargo run -p leek_wars_gen --bin leekgen -- scenario validate <path> --recursive
cargo run -p leek_wars_gen --bin leekgen -- scenario convert file.json --to toml
```

## Related

- [Correctness and parity](../architecture/correctness-and-parity.md) (seeds, loaders, replay expectations)
- [generator-and-engine.md](../architecture/generator-and-engine.md)
- [data-and-fixtures.md](../architecture/data-and-fixtures.md)
- Example: `leek-wars-generator/test/scenario/scenario1.json` (submodule)

---

*Revision: operational reference aligned with `scenario.rs` / `scenario_io.rs`; extend when serde shape changes.*
