//! Farmer profile and preferences (beyond login).

use serde::Serialize;
use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

#[derive(Debug, Serialize)]
pub struct FarmerSetLanguageRequest<'a> {
    pub language: &'a str,
}

#[derive(Debug, Serialize)]
pub struct FarmerLoginComebackRequest<'a> {
    pub token: &'a str,
}

impl LeekWarsClient {
    /// Public farmer profile (`farmer/get/{id}`).
    pub async fn farmer_get(&self, farmer_id: i64) -> Result<Value> {
        self.get_json(&format!("farmer/get/{farmer_id}")).await
    }

    /// Full login payload for the current session (teams, etc.).
    pub async fn farmer_get_login_data(&self) -> Result<Value> {
        self.get_json("farmer/get-login-data").await
    }

    /// Change UI / account language.
    pub async fn farmer_set_language(&self, language: &str) -> Result<Value> {
        let body = FarmerSetLanguageRequest { language };
        self.put_json("farmer/set-language", &body).await
    }

    /// Resume session via emailed link token (same as login page with `:token` route).
    pub async fn farmer_login_comeback(&mut self, token: &str) -> Result<Value> {
        let body = FarmerLoginComebackRequest { token };
        let v: Value = self.post_json("farmer/login-comeback", &body).await?;
        if let Some(t) = v.get("token").and_then(|x| x.as_str()) {
            self.set_bearer(Some(t.to_string()));
        } else {
            self.set_bearer(Some("$".to_string()));
        }
        Ok(v)
    }
}
