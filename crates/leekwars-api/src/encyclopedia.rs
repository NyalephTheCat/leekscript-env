//! Encyclopedia pages per locale.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

impl LeekWarsClient {
    /// All encyclopedia pages for a locale (e.g. `fr`, `en`), as a JSON object keyed by page id/slug.
    pub async fn encyclopedia_get_all_locale(&self, locale: &str) -> Result<Value> {
        let path = format!("encyclopedia/get-all-locale/{locale}");
        self.get_json(&path).await
    }

    /// Single page by locale and slug (same slug as in [`encyclopedia_get_all_locale`] keys, e.g. `leek wars`).
    pub async fn encyclopedia_get_page(&self, locale: &str, slug: &str) -> Result<Value> {
        let enc = utf8_percent_encode(slug, NON_ALPHANUMERIC);
        let path = format!("encyclopedia/get/{locale}/{enc}");
        self.get_json(&path).await
    }
}
