# leekwars-api

Unofficial Rust HTTP client for the [Leek Wars](https://leekwars.com) JSON API (`/api/…`). It mirrors the official Vue client: JSON bodies, session cookies after login, and optional `Authorization: Bearer`.

**Not affiliated with Leek Wars.** Use responsibly and respect the site’s terms and rate limits.

## Usage

```rust
use leekwars_api::LeekWarsClient;

#[tokio::main]
async fn main() -> Result<(), leekwars_api::Error> {
    let client = LeekWarsClient::new()?;
    let v = client.data_version().await?;
    println!("{}", v.master_version);
    Ok(())
}
```

### Login (session cookies)

```rust
let mut client = LeekWarsClient::new()?;
client
    .farmer_login("user", "password", false)
    .await?;
// Further calls use the same `client` so cookies apply.
```

Custom API base (e.g. local stack):

```rust
use leekwars_api::LeekWarsClient;
use url::Url;

let client = LeekWarsClient::builder()
    .base_url(Url::parse("http://localhost:8500/api/")?)
    .build()?;
```

### Download all AIs to `./ai/`

The library exposes `LeekWarsClient::export_farmer_ais_to_directory` and `ai_leek_relative_path` so on-disk layout matches the editor: `farmer.folders` plus each AI’s `folder` and `name` (same idea as `FileSystem.getAIFullPath` in [leek-wars](https://github.com/leek-wars/leek-wars)).

CLI example from the workspace root (loads `.env` next to `Cargo.toml`):

```bash
cargo run -p leekwars-api --example download_ais
```

Override output directory: `LEEKWARS_AI_DIR=/path/to/out`. Uses `ai/sync`; `ai/download` is only a fallback when an id is missing from sync.

### Integration tests (optional)

Live calls are ignored by default:

With credentials in the workspace root `.env` (`LEEKWARS_LOGIN`, `LEEKWARS_PASSWORD`), integration tests load them via [dotenvy](https://crates.io/crates/dotenvy). Use **one test thread** so parallel requests do not trip `rate_limit`:

```bash
cargo test -p leekwars-api --test integration_live -- --ignored --nocapture --test-threads=1
```

## Endpoints (summary)

Rust methods live on `LeekWarsClient`; source files under `src/` group related routes:

| Module (file) | Examples |
|---------------|----------|
| Auth | `farmer_login`, `farmer_login_token`, `farmer_get_from_token`, `farmer_disconnect` |
| Farmer | `farmer_get`, `farmer_get_login_data`, `farmer_set_language`, `farmer_login_comeback` |
| Data | `data_version`, `data_get_all` |
| Encyclopedia | `encyclopedia_get_all_locale` |
| AI | `ai_sync`, `ai_save`, `ai_download`, `ai_rename`, `ai_new_name`, `ai_delete`, `ai_restore`, `ai_destroy`, folder helpers, `ai_test_scenario`, `ai_set_version`, `ai_set_strict`, `ai_bin_empty` |
| AI export | `export_farmer_ais_to_directory`, `ai_leek_relative_path` ([`ai_export.rs`](crates/leekwars-api/src/ai_export.rs)) |
| Leek | `leek_create`, `leek_set_in_garden`, `leek_set_ai` / `leek_remove_ai`, tournaments, BR, capital, rename, level popup |
| Fight | `fight_get`, `fight_get_logs`, `fight_comment` |
| Team | `team_get` / `team_get_private` / `team_get_connected`, `team_get_recruiting`, `team_rankings`, create/rename/dissolve/quit, compositions, invitations, candidacies, `team_set_emblem` (multipart), turret AI, logs level, … |
| Market | `market_get_item_templates`, `market_buy_habs_quantity` / `market_buy_crystals_quantity` (`item_id` can be e.g. `50` or `"50fights"`), `market_sell_habs`, `market_item_seen`, `market_sound_played` |
| Message | `message_get_messages`, `message_get_conversations`, `message_find_conversation`, send/read/mute/censor/delete, `message_create_conversation`, … |
| Tournament | `tournament_range_compo`, `tournament_range_leek` |
| Ranking | `ranking_fun`, `ranking_get_home_ranking`, `ranking_page`, `ranking_search` |
| Notifications | `notification_get_latest`, `notification_read` |
| Meta | `function_get_all`, `country_get_all`, `changelog_get`, `statistic_get_all`, `talent_farmer`, `talent_leek`, `service_get_all` (auth) |

For anything not wrapped yet, use `get_json` / `post_json` / `put_json` / `delete_json` on the client, or `post_multipart` for other uploads (same auth cookies / Bearer as JSON calls).

## Reference

Endpoint names and payloads follow the [leek-wars](https://github.com/leek-wars/leek-wars) frontend (`src/model/leekwars.ts` and call sites).
