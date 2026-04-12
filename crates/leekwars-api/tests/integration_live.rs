//! Live API checks. Loads `.env` from the workspace (walk-up) for `LEEKWARS_LOGIN` / `LEEKWARS_PASSWORD`.
//!
//! Run (serial + spacing avoids `rate_limit`):
//! `cargo test -p leekwars-api --test integration_live -- --ignored --nocapture --test-threads=1`

use std::time::Duration;

use leekwars_api::LeekWarsClient;
use serde_json::Value;

async fn throttle() {
    tokio::time::sleep(Duration::from_millis(450)).await;
}

fn load_env() {
    let _ = dotenvy::dotenv();
}

fn creds() -> Option<(String, String)> {
    load_env();
    match (
        std::env::var("LEEKWARS_LOGIN").ok(),
        std::env::var("LEEKWARS_PASSWORD").ok(),
    ) {
        (Some(u), Some(p)) if !u.is_empty() && !p.is_empty() => Some((u, p)),
        _ => None,
    }
}

#[tokio::test]
#[ignore = "hits production leekwars.com"]
async fn data_version_public() {
    load_env();
    let client = LeekWarsClient::new().expect("client");
    let v = client.data_version().await.expect("data/version");
    assert!(!v.master_version.is_empty());
}

#[tokio::test]
#[ignore = "hits production leekwars.com"]
async fn login_and_get_from_token() {
    let (login, password) =
        creds().expect("set LEEKWARS_LOGIN and LEEKWARS_PASSWORD in .env or env");
    let mut client = LeekWarsClient::new().expect("client");
    client
        .farmer_login(&login, &password, false)
        .await
        .expect("farmer/login");
    let _session = client
        .farmer_get_from_token()
        .await
        .expect("farmer/get-from-token");
}

/// Exercises public + authenticated methods across modules (one login).
#[tokio::test]
#[ignore = "hits production leekwars.com"]
async fn full_client_smoke() {
    let (login, password) =
        creds().expect("set LEEKWARS_LOGIN and LEEKWARS_PASSWORD in .env or env");
    let mut client = LeekWarsClient::new().expect("client");

    let ver = client.data_version().await.expect("data/version");
    assert!(!ver.master_version.is_empty());
    throttle().await;

    client
        .function_get_all()
        .await
        .expect("function/get-all (public)");
    throttle().await;

    client
        .country_get_all()
        .await
        .expect("country/get-all (public)");
    throttle().await;

    client
        .changelog_get("en")
        .await
        .expect("changelog/get (public)");
    throttle().await;

    client
        .encyclopedia_get_all_locale("en")
        .await
        .expect("encyclopedia/get-all-locale (public)");
    throttle().await;

    client
        .farmer_login(&login, &password, false)
        .await
        .expect("farmer/login");
    throttle().await;

    let session: Value = client
        .farmer_get_from_token()
        .await
        .expect("farmer/get-from-token");
    let farmer_id = session["farmer"]["id"]
        .as_i64()
        .expect("response should include farmer.id");
    throttle().await;

    client.farmer_get(farmer_id).await.expect("farmer/get/{id}");
    throttle().await;

    client
        .service_get_all()
        .await
        .expect("service/get-all (auth)");
    throttle().await;

    client.talent_farmer().await.expect("talent/farmer");
    throttle().await;

    client.talent_leek().await.expect("talent/leek");
    throttle().await;

    client.statistic_get_all().await.expect("statistic/get-all");
    throttle().await;

    client
        .notification_get_latest(5)
        .await
        .expect("notification/get-latest");
    throttle().await;

    client
        .market_get_item_templates()
        .await
        .expect("market/get-item-templates");
    throttle().await;

    client.ranking_fun().await.expect("ranking/fun");
    throttle().await;

    client
        .ranking_get_home_ranking()
        .await
        .expect("ranking/get-home-ranking");
    throttle().await;

    if let Some(team_id) = session["farmer"]["team"]["id"].as_i64() {
        client
            .team_get_connected(team_id)
            .await
            .expect("team/get-connected");
    }
}
