//! Low-level HTTP client (JSON + cookies + optional Bearer).

use std::sync::Arc;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::error::{ApiErrorBody, Error, Result};

const DEFAULT_API_BASE: &str = "https://leekwars.com/api/";

/// Client for `https://leekwars.com/api/` (or a custom base), matching the official web app transport.
pub struct LeekWarsClient {
    http: reqwest::Client,
    base: Url,
    bearer: Option<String>,
}

impl LeekWarsClient {
    /// New client with default production API base and an in-memory cookie jar (session after login).
    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    pub fn builder() -> LeekWarsClientBuilder {
        LeekWarsClientBuilder {
            base: DEFAULT_API_BASE.parse().expect("valid default API URL"),
            bearer: None,
        }
    }

    /// Full URL for a path segment (no leading slash), e.g. `farmer/login`.
    pub fn endpoint_url(&self, path: &str) -> Result<Url> {
        Ok(self.base.join(path)?)
    }

    pub fn base_url(&self) -> &Url {
        &self.base
    }

    /// Set or clear Bearer token (used in dev with `farmer/login-token`; production web uses `'$'` with cookies).
    pub fn set_bearer(&mut self, token: Option<String>) {
        self.bearer = token;
    }

    pub fn bearer(&self) -> Option<&str> {
        self.bearer.as_deref()
    }

    fn headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=UTF-8"),
        );
        if let Some(t) = &self.bearer {
            let value = format!("Bearer {t}");
            if let Ok(v) = HeaderValue::from_str(&value) {
                h.insert(AUTHORIZATION, v);
            }
        }
        h
    }

    /// `GET base/path` → JSON body.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = self.endpoint_url(path)?;
        let resp = self.http.get(url).headers(self.headers()).send().await?;
        self.json_response(resp).await
    }

    /// `POST base/path` with JSON body.
    pub async fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.endpoint_url(path)?;
        let resp = self
            .http
            .post(url)
            .headers(self.headers())
            .json(body)
            .send()
            .await?;
        self.json_response(resp).await
    }

    /// `POST base/path` with JSON body; ignore JSON response (empty or non-JSON OK for some endpoints).
    pub async fn post_json_discard<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<()> {
        let url = self.endpoint_url(path)?;
        let resp = self
            .http
            .post(url)
            .headers(self.headers())
            .json(body)
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Self::err_from_response(resp).await
        }
    }

    /// `PUT base/path` with JSON body.
    pub async fn put_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.endpoint_url(path)?;
        let resp = self
            .http
            .put(url)
            .headers(self.headers())
            .json(body)
            .send()
            .await?;
        self.json_response(resp).await
    }

    /// `DELETE base/path` with JSON body (same as the Vue XHR client).
    pub async fn delete_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = self.endpoint_url(path)?;
        let resp = self
            .http
            .delete(url)
            .headers(self.headers())
            .json(body)
            .send()
            .await?;
        self.json_response(resp).await
    }

    /// `POST multipart/form-data` (e.g. team emblem). Does not send JSON `Content-Type`.
    pub async fn post_multipart(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<Value> {
        let url = self.endpoint_url(path)?;
        let mut req = self.http.post(url).multipart(form);
        if let Some(t) = &self.bearer {
            let value = format!("Bearer {t}");
            if let Ok(v) = HeaderValue::from_str(&value) {
                req = req.header(AUTHORIZATION, v);
            }
        }
        let resp = req.send().await?;
        self.json_response(resp).await
    }

    /// Raw GET (e.g. downloading AI source); returns bytes on success.
    pub async fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let url = self.endpoint_url(path)?;
        let mut req = self.http.get(url);
        if let Some(t) = &self.bearer {
            req = req.header(AUTHORIZATION, format!("Bearer {t}"));
        }
        let resp = req.send().await?;
        if resp.status().is_success() {
            Ok(resp.bytes().await?.to_vec())
        } else {
            Self::err_from_response(resp).await
        }
    }

    async fn json_response<T: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        if resp.status().is_success() {
            let text = resp.text().await?;
            Ok(serde_json::from_str(&text)?)
        } else {
            Self::err_from_response(resp).await
        }
    }

    async fn err_from_response<T>(resp: reqwest::Response) -> Result<T> {
        let status = resp.status().as_u16();
        let retry_after_secs = parse_retry_after_secs(resp.headers());
        let body = resp.text().await.unwrap_or_default();
        if let Ok(api) = serde_json::from_str::<ApiErrorBody>(&body) {
            if let Some(msg) = api.message() {
                return Err(Error::Api(msg.to_string()));
            }
        }
        Err(Error::Http {
            status,
            body,
            retry_after_secs,
        })
    }
}

/// Delta-seconds form of `Retry-After` (common for 429). HTTP-date form is not parsed yet.
fn parse_retry_after_secs(headers: &HeaderMap) -> Option<u64> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?;
    raw.trim().parse::<u64>().ok()
}

pub struct LeekWarsClientBuilder {
    base: Url,
    bearer: Option<String>,
}

impl LeekWarsClientBuilder {
    pub fn base_url(mut self, base: Url) -> Self {
        self.base = base;
        self
    }

    pub fn bearer(mut self, token: impl Into<String>) -> Self {
        self.bearer = Some(token.into());
        self
    }

    pub fn build(self) -> Result<LeekWarsClient> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let http = reqwest::Client::builder().cookie_provider(jar).build()?;
        let mut base = self.base;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Ok(LeekWarsClient {
            http,
            base,
            bearer: self.bearer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_join_farmer_login() {
        let c = LeekWarsClient::new().unwrap();
        let u = c.endpoint_url("farmer/login").unwrap();
        assert_eq!(u.as_str(), "https://leekwars.com/api/farmer/login");
    }

    #[test]
    fn builder_normalizes_trailing_slash() {
        let c = LeekWarsClientBuilder {
            base: "https://example.com/api".parse().unwrap(),
            bearer: None,
        }
        .build()
        .unwrap();
        assert!(c.base_url().as_str().ends_with("/api/"));
    }

    #[test]
    fn parse_api_error_body() {
        let j = r#"{"error":"wrong_password"}"#;
        let b: ApiErrorBody = serde_json::from_str(j).unwrap();
        assert_eq!(b.message(), Some("wrong_password"));
    }
}
