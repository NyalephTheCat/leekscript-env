//! Session: login, restore session, disconnect.

use serde::Serialize;
use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

/// Login form (matches [leek-wars/src/component/login/login.vue](https://github.com/leek-wars/leek-wars)).
#[derive(Debug, Serialize)]
pub struct LoginRequest<'a> {
    pub login: &'a str,
    pub password: &'a str,
    pub keep_connected: bool,
}

impl LeekWarsClient {
    /// Production login (`farmer/login`). Sets session cookies on the underlying client.
    pub async fn farmer_login(
        &mut self,
        login: &str,
        password: &str,
        keep_connected: bool,
    ) -> Result<Value> {
        let body = LoginRequest {
            login,
            password,
            keep_connected,
        };
        let v: Value = self.post_json("farmer/login", &body).await?;
        if let Some(token) = v.get("token").and_then(|t| t.as_str()) {
            self.set_bearer(Some(token.to_string()));
        } else {
            // Production web uses cookie session; Bearer is often the placeholder `$`.
            self.set_bearer(Some("$".to_string()));
        }
        Ok(v)
    }

    /// Dev-style login that returns a JWT (`farmer/login-token`). Use when the server exposes it.
    pub async fn farmer_login_token(
        &mut self,
        login: &str,
        password: &str,
        keep_connected: bool,
    ) -> Result<Value> {
        let body = LoginRequest {
            login,
            password,
            keep_connected,
        };
        let v: Value = self.post_json("farmer/login-token", &body).await?;
        if let Some(token) = v.get("token").and_then(|t| t.as_str()) {
            self.set_bearer(Some(token.to_string()));
        }
        Ok(v)
    }

    /// Restore farmer from existing session (cookies and/or Bearer, same as page load).
    pub async fn farmer_get_from_token(&self) -> Result<Value> {
        self.get_json("farmer/get-from-token").await
    }

    /// Log out (invalidates server session; clears cookies on next response).
    pub async fn farmer_disconnect(&self) -> Result<()> {
        self.post_json_discard("farmer/disconnect", &serde_json::json!({}))
            .await
    }
}
