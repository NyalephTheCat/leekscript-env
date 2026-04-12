//! Download all LeekScript AIs into `./ai/` (or `LEEKWARS_AI_DIR`) using
//! [`LeekWarsClient::export_farmer_ais_to_directory`](leekwars_api::LeekWarsClient::export_farmer_ais_to_directory)
//! (same folder layout as the web editor).
//!
//! ```text
//! cargo run -p leekwars-api --example download_ais
//! ```
//!
//! Requires `LEEKWARS_LOGIN` and `LEEKWARS_PASSWORD` (e.g. workspace `.env`).

use std::env;
use std::path::{Path, PathBuf};

use leekwars_api::{AiExportOptions, LeekWarsClient};

#[tokio::main]
async fn main() -> Result<(), String> {
    let _ = dotenvy::from_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env"));
    let _ = dotenvy::dotenv();

    let login =
        env::var("LEEKWARS_LOGIN").map_err(|_| "set LEEKWARS_LOGIN (e.g. in .env)".to_string())?;
    let password =
        env::var("LEEKWARS_PASSWORD").map_err(|_| "set LEEKWARS_PASSWORD".to_string())?;
    let out_root = env::var("LEEKWARS_AI_DIR").unwrap_or_else(|_| "ai".to_string());
    let out_root = PathBuf::from(out_root);

    let mut client = LeekWarsClient::new().map_err(|e| e.to_string())?;
    client
        .farmer_login(&login, &password, false)
        .await
        .map_err(|e| e.to_string())?;

    let session = client
        .farmer_get_from_token()
        .await
        .map_err(|e| e.to_string())?;
    let farmer = session
        .get("farmer")
        .ok_or_else(|| "session response has no `farmer` field".to_string())?;

    eprintln!("Exporting AIs to {} …", out_root.display());
    let report = client
        .export_farmer_ais_to_directory(farmer, &out_root, AiExportOptions::default())
        .await
        .map_err(|e| e.to_string())?;

    for p in &report.paths {
        println!("{}", p.display());
    }
    eprintln!(
        "Wrote {} AI file(s) under {}",
        report.written,
        out_root.display()
    );
    if !report.failures.is_empty() {
        eprintln!("Failures:");
        for (id, msg) in &report.failures {
            eprintln!("  id {id}: {msg}");
        }
        return Err(format!("{} failure(s)", report.failures.len()));
    }

    Ok(())
}
