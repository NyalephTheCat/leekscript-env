//! Rankings and search.

use serde::Serialize;
use serde_json::Value;

use crate::client::LeekWarsClient;
use crate::error::Result;

#[derive(Debug, Serialize)]
pub struct RankingSearchRequest<'a> {
    pub query: &'a str,
    pub search_leeks: bool,
    pub search_farmers: bool,
    pub search_teams: bool,
}

impl LeekWarsClient {
    pub async fn ranking_fun(&self) -> Result<Value> {
        self.get_json("ranking/fun").await
    }

    pub async fn ranking_get_home_ranking(&self) -> Result<Value> {
        self.get_json("ranking/get-home-ranking").await
    }

    /// Paged ranking table (`ranking/{service}/{category}/{order}/{page}/{country}`).
    pub async fn ranking_page(
        &self,
        service: &str,
        category: &str,
        order: &str,
        page: i32,
        country: &str,
    ) -> Result<Value> {
        let path = format!("ranking/{service}/{category}/{order}/{page}/{country}");
        self.get_json(&path).await
    }

    pub async fn ranking_search(&self, req: &RankingSearchRequest<'_>) -> Result<Value> {
        self.post_json("ranking/search", req).await
    }
}
