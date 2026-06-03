use std::{collections::HashMap, time::Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use objective_core::{
    traits::SourceAdapter,
    types::{BodyFormat, HealthStatus, PollResult, RateLimitConfig, RawDocument},
    ObjectiveError, Result,
};
use serde::Deserialize;
use serde_json::json;

use crate::normalizer::{DocumentInput, DocumentNormalizer};

/// Source adapter for Reddit's public JSON API.
///
/// Fetches posts from a subreddit using `https://www.reddit.com/r/{subreddit}/new.json`
/// without authentication. Each post is converted to a normalized [`RawDocument`]
/// preserving the title, author, score, and comment metadata.
#[derive(Debug, Clone)]
pub struct RedditSourceAdapter {
    name: String,
    subreddit: String,
    endpoint: String,
    sort: String,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture: Option<String>,
}

impl RedditSourceAdapter {
    /// Create a new adapter for a specific subreddit.
    ///
    /// * `name`      – source identifier used in the ingestion event envelope.
    /// * `subreddit` – the subreddit name (without r/ prefix).
    pub fn new(name: impl Into<String>, subreddit: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            subreddit: subreddit.into(),
            endpoint: "https://www.reddit.com".to_string(),
            sort: "new".to_string(),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: None,
        }
    }

    /// Set the sort order (new, hot, top, rising).
    pub fn with_sort(mut self, sort: impl Into<String>) -> Self {
        self.sort = sort.into();
        self
    }

    /// Build an adapter that returns a pre-canned JSON payload (for tests).
    pub fn from_json(name: impl Into<String>, subreddit: &str, json_body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            subreddit: subreddit.to_string(),
            endpoint: "https://www.reddit.com".to_string(),
            sort: "new".to_string(),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: Some(json_body.into()),
        }
    }

    fn build_url(&self) -> String {
        format!(
            "{}/r/{}/{}.json",
            self.endpoint.trim_end_matches('/'),
            self.subreddit,
            self.sort
        )
    }

    async fn fetch_payload(&self) -> Result<String> {
        if let Some(fixture) = &self.fixture {
            return Ok(fixture.clone());
        }

        let response = self
            .client
            .get(self.build_url())
            .header(
                reqwest::header::USER_AGENT,
                "Objective/0.1 (local intelligence system)",
            )
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to fetch Reddit feed: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "Reddit API returned HTTP {}",
                response.status()
            )));
        }

        response.text().await.map_err(|error| {
            ObjectiveError::Source(format!("failed to read Reddit body: {error}"))
        })
    }
}

#[derive(Debug, Deserialize)]
struct ListingResponse {
    #[serde(default)]
    data: Option<ListingData>,
}

#[derive(Debug, Deserialize)]
struct ListingData {
    #[serde(default)]
    children: Vec<PostChild>,
}

#[derive(Debug, Deserialize)]
struct PostChild {
    #[serde(default)]
    data: Option<Post>,
}

#[derive(Debug, Deserialize)]
struct Post {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, alias = "title")]
    title: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    subreddit: Option<String>,
    #[serde(default)]
    selftext: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default, alias = "permalink")]
    permalink: Option<String>,
    #[serde(default)]
    score: Option<i64>,
    #[serde(default, alias = "num_comments")]
    num_comments: Option<i64>,
    #[serde(default)]
    over_18: Option<bool>,
    #[serde(default, alias = "created_utc")]
    created_utc: Option<f64>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    link_flair_text: Option<String>,
}

fn build_body(post: &Post) -> String {
    let mut parts = Vec::new();

    if let Some(title) = &post.title {
        parts.push(title.clone());
    }

    if let Some(selftext) = &post.selftext {
        if !selftext.trim().is_empty() {
            parts.push(selftext.clone());
        }
    }

    if let Some(score) = post.score {
        parts.push(format!("Score: {score}."));
    }

    if let Some(comments) = post.num_comments {
        parts.push(format!("Comments: {comments}."));
    }

    if let Some(flair) = &post.link_flair_text {
        parts.push(format!("Flair: {flair}."));
    }

    if parts.is_empty() {
        return format!(
            "Reddit post {} by {}",
            post.id.as_deref().unwrap_or("unknown"),
            post.author.as_deref().unwrap_or("anonymous")
        );
    }
    parts.join(" ")
}

fn resolve_external_id(post: &Post) -> String {
    post.id.clone().unwrap_or_else(|| "unknown".to_string())
}

fn resolve_url(post: &Post) -> Option<String> {
    if let Some(permalink) = &post.permalink {
        return Some(format!("https://www.reddit.com{permalink}"));
    }
    post.url.clone()
}

fn resolve_published_at(post: &Post) -> Option<DateTime<Utc>> {
    post.created_utc
        .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds as i64, 0))
}

#[async_trait]
impl SourceAdapter for RedditSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "reddit"
    }

    fn validate(&self) -> Result<()> {
        if self.subreddit.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "Reddit subreddit is required".to_string(),
            ));
        }
        Ok(())
    }

    async fn poll(&self, cursor: Option<String>) -> Result<PollResult> {
        self.validate()?;
        let started = Instant::now();
        let payload = self.fetch_payload().await?;
        let listing: ListingResponse = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse Reddit JSON: {error}"))
        })?;

        let cursor_ts = cursor
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let mut documents = Vec::new();
        let mut latest: Option<DateTime<Utc>> = None;

        for child in listing.data.map(|d| d.children).unwrap_or_default() {
            let post = match child.data {
                Some(post) => post,
                None => continue,
            };

            let published_at = resolve_published_at(&post);
            if let (Some(cursor), Some(published_at)) = (cursor_ts, published_at) {
                if published_at <= cursor {
                    continue;
                }
            }

            let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
            metadata.insert("subreddit".to_string(), json!(post.subreddit));
            metadata.insert("score".to_string(), json!(post.score));
            metadata.insert("num_comments".to_string(), json!(post.num_comments));
            metadata.insert("over_18".to_string(), json!(post.over_18));
            metadata.insert("domain".to_string(), json!(post.domain));
            metadata.insert("link_flair_text".to_string(), json!(post.link_flair_text));

            let input = DocumentInput {
                source_id: self.name.clone(),
                source_type: "reddit".to_string(),
                external_id: resolve_external_id(&post),
                url: resolve_url(&post),
                title: post.title.clone(),
                body: build_body(&post),
                body_format: BodyFormat::PlainText,
                author: post.author.clone(),
                published_at,
                metadata,
                raw_bytes: None,
            };

            let document = self.normalizer.normalize(input)?;
            if let Some(published_at) = document.published_at {
                latest = Some(latest.map_or(published_at, |current| current.max(published_at)));
            }
            documents.push(document);
        }

        Ok(PollResult {
            documents,
            new_cursor: latest.map(|timestamp| timestamp.to_rfc3339()),
            has_more: false,
            poll_duration: started.elapsed(),
        })
    }

    async fn fetch_one(&self, external_id: &str) -> Result<RawDocument> {
        let payload = self.fetch_payload().await?;
        let listing: ListingResponse = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse Reddit JSON: {error}"))
        })?;

        let post = listing
            .data
            .and_then(|d| {
                d.children
                    .into_iter()
                    .find(|c| c.data.as_ref().map(|p| p.id.as_deref()) == Some(Some(external_id)))
                    .and_then(|c| c.data)
            })
            .ok_or_else(|| ObjectiveError::Source(format!("Reddit post not found: {external_id}")))?;

        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
        metadata.insert("subreddit".to_string(), json!(post.subreddit));
        metadata.insert("score".to_string(), json!(post.score));
        metadata.insert("num_comments".to_string(), json!(post.num_comments));

        self.normalizer.normalize(DocumentInput {
            source_id: self.name.clone(),
            source_type: "reddit".to_string(),
            external_id: resolve_external_id(&post),
            url: resolve_url(&post),
            title: post.title.clone(),
            body: build_body(&post),
            body_format: BodyFormat::PlainText,
            author: post.author.clone(),
            published_at: resolve_published_at(&post),
            metadata,
            raw_bytes: None,
        })
    }

    async fn health(&self) -> Result<HealthStatus> {
        let started = Instant::now();
        if self.fixture.is_some() {
            return Ok(HealthStatus {
                source_id: self.name.clone(),
                status: "healthy".to_string(),
                latency_ms: 0,
            });
        }

        let status = self
            .client
            .get(self.build_url())
            .header(
                reqwest::header::USER_AGENT,
                "Objective/0.1 (local intelligence system)",
            )
            .send()
            .await
            .map(|response| {
                if response.status().is_success() {
                    "healthy"
                } else {
                    "degraded"
                }
            })
            .map_err(|error| {
                ObjectiveError::Source(format!("Reddit health check failed: {error}"))
            })?;

        Ok(HealthStatus {
            source_id: self.name.clone(),
            status: status.to_string(),
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn rate_limit_config(&self) -> RateLimitConfig {
        RateLimitConfig {
            requests_per_minute: 30,
            burst: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "data": {
            "children": [
                {
                    "data": {
                        "id": "abc123",
                        "title": "Rust 2024 Edition Released",
                        "author": "rustacean42",
                        "subreddit": "rust",
                        "selftext": "The Rust 2024 edition has been released with exciting new features including async closures and improved trait upcasting.",
                        "url": "https://blog.rust-lang.org/2024-edition",
                        "permalink": "/r/rust/comments/abc123/rust_2024_edition_released/",
                        "score": 1250,
                        "num_comments": 342,
                        "over_18": false,
                        "created_utc": 1748865600.0,
                        "domain": "blog.rust-lang.org",
                        "link_flair_text": "News"
                    }
                },
                {
                    "data": {
                        "id": "def456",
                        "title": "What's the best way to learn async Rust?",
                        "author": "newbie_dev",
                        "subreddit": "rust",
                        "selftext": "I've been learning Rust for a few months and want to dive into async. Any recommendations?",
                        "url": "https://www.reddit.com/r/rust/comments/def456",
                        "permalink": "/r/rust/comments/def456/whats_the_best_way_to_learn/",
                        "score": 89,
                        "num_comments": 67,
                        "over_18": false,
                        "created_utc": 1748862000.0,
                        "domain": "self.rust",
                        "link_flair_text": "Question"
                    }
                }
            ]
        }
    }"#;

    #[tokio::test]
    async fn test_poll_parses_reddit_posts_into_documents() {
        let adapter = RedditSourceAdapter::from_json("reddit_fixture", "rust", FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 2);

        let first = &result.documents[0];
        assert_eq!(first.source_type, "reddit");
        assert_eq!(first.external_id, "abc123");
        assert!(first.body.contains("Rust 2024 Edition"));
        assert_eq!(first.metadata["score"], json!(1250));
        assert_eq!(first.metadata["subreddit"], json!("rust"));

        let second = &result.documents[1];
        assert_eq!(second.external_id, "def456");
        assert!(second.body.contains("async Rust"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter = RedditSourceAdapter::from_json("reddit_fixture", "rust", FIXTURE);

        let result = adapter
            .poll(Some("2026-06-02T12:00:30Z".to_string()))
            .await
            .unwrap();
        assert!(result.documents.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_one_returns_matching_document() {
        let adapter = RedditSourceAdapter::from_json("reddit_fixture", "rust", FIXTURE);

        let document = adapter.fetch_one("def456").await.unwrap();
        assert_eq!(document.external_id, "def456");
        assert!(document.body.contains("async Rust"));
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter = RedditSourceAdapter::from_json("reddit_fixture", "rust", FIXTURE);
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }
}
