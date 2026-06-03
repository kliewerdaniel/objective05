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

/// Source adapter for Hacker News backed by the public Algolia search API.
///
/// Polling the front page or any other tag, the adapter converts each story
/// into a normalized [`RawDocument`] preserving the link, author, and score
/// metadata. The HTTP client is only used when the adapter was constructed
/// from a URL; the `from_json` constructor is intended for offline tests.
#[derive(Debug, Clone)]
pub struct HackerNewsSourceAdapter {
    name: String,
    endpoint: String,
    tags: Option<String>,
    query: Option<String>,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture: Option<String>,
}

impl HackerNewsSourceAdapter {
    /// Create a new adapter pointing at a Hacker News Algolia endpoint.
    ///
    /// * `name`     – source identifier used in the ingestion event envelope.
    /// * `endpoint` – base URL, e.g. `https://hn.algolia.com/api/v1`.
    pub fn new(name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: endpoint.into(),
            tags: None,
            query: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: None,
        }
    }

    /// Restrict results to a single Algolia tag (e.g. `story`, `front_page`).
    pub fn with_tags(mut self, tags: impl Into<String>) -> Self {
        self.tags = Some(tags.into());
        self
    }

    /// Restrict results to free-text query.
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Build an adapter that returns a pre-canned JSON payload (for tests).
    pub fn from_json(name: impl Into<String>, json_body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: "https://hn.algolia.com/api/v1".to_string(),
            tags: None,
            query: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: Some(json_body.into()),
        }
    }

    fn build_url(&self) -> String {
        let path = match (self.tags.as_deref(), self.query.as_deref()) {
            (Some(tags), Some(query)) => {
                format!("/search_by_date?tags={tags}&query={query}")
            }
            (Some(tags), None) => format!("/search_by_date?tags={tags}"),
            (None, Some(query)) => format!("/search?query={query}"),
            (None, None) => "/search?tags=front_page".to_string(),
        };
        format!("{}{}", self.endpoint.trim_end_matches('/'), path)
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
                ObjectiveError::Source(format!("failed to fetch Hacker News feed: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "Hacker News feed returned HTTP {}",
                response.status()
            )));
        }

        response.text().await.map_err(|error| {
            ObjectiveError::Source(format!("failed to read Hacker News body: {error}"))
        })
    }
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    hits: Vec<Hit>,
}

#[derive(Debug, Deserialize)]
struct Hit {
    #[serde(default, alias = "objectID")]
    object_id: Option<String>,
    #[serde(default)]
    story_id: Option<i64>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    story_title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    story_url: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default, alias = "created_at_i")]
    created_at_unix: Option<i64>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    points: Option<i64>,
    #[serde(default)]
    num_comments: Option<i64>,
    #[serde(default)]
    story_text: Option<String>,
    #[serde(default)]
    comment_text: Option<String>,
}

fn build_body(hit: &Hit) -> String {
    let mut parts = Vec::new();
    if let Some(title) = hit.story_title.as_ref().or(hit.title.as_ref()) {
        parts.push(title.clone());
    }
    if let Some(text) = hit.story_text.as_ref().or(hit.comment_text.as_ref()) {
        parts.push(text.clone());
    }
    if let Some(points) = hit.points {
        parts.push(format!("Score: {points} points."));
    }
    if let Some(comments) = hit.num_comments {
        parts.push(format!("Comments: {comments}."));
    }
    if parts.is_empty() {
        return format!(
            "Hacker News item {} by {}",
            hit.story_id
                .map(|id| id.to_string())
                .or_else(|| hit.object_id.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            hit.author
                .clone()
                .unwrap_or_else(|| "anonymous".to_string())
        );
    }
    parts.join(" ")
}

fn resolve_external_id(hit: &Hit) -> String {
    if let Some(object_id) = hit.object_id.as_ref() {
        return object_id.clone();
    }
    if let Some(story_id) = hit.story_id {
        return story_id.to_string();
    }
    "unknown".to_string()
}

fn resolve_url(hit: &Hit) -> Option<String> {
    hit.story_url
        .clone()
        .or_else(|| hit.url.clone())
        .or_else(|| {
            hit.story_id
                .map(|id| format!("https://news.ycombinator.com/item?id={id}"))
        })
        .or_else(|| {
            hit.object_id
                .clone()
                .map(|id| format!("https://news.ycombinator.com/item?id={id}"))
        })
}

fn resolve_title(hit: &Hit) -> Option<String> {
    hit.story_title.clone().or_else(|| hit.title.clone())
}

fn resolve_published_at(hit: &Hit) -> Option<DateTime<Utc>> {
    if let Some(created_at) = hit.created_at.as_deref() {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(created_at) {
            return Some(parsed.with_timezone(&Utc));
        }
    }
    hit.created_at_unix
        .and_then(|seconds| chrono::DateTime::<Utc>::from_timestamp(seconds, 0))
}

#[async_trait]
impl SourceAdapter for HackerNewsSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "hacker_news"
    }

    fn validate(&self) -> Result<()> {
        if self.endpoint.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "Hacker News endpoint is required".to_string(),
            ));
        }
        Ok(())
    }

    async fn poll(&self, cursor: Option<String>) -> Result<PollResult> {
        self.validate()?;
        let started = Instant::now();
        let payload = self.fetch_payload().await?;
        let parsed: SearchResponse = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse Hacker News JSON: {error}"))
        })?;

        let cursor_ts = cursor
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let mut documents = Vec::new();
        let mut latest: Option<DateTime<Utc>> = None;

        for hit in parsed.hits {
            let published_at = resolve_published_at(&hit);
            if let (Some(cursor), Some(published_at)) = (cursor_ts, published_at) {
                if published_at <= cursor {
                    continue;
                }
            }

            let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
            metadata.insert("endpoint".to_string(), json!(self.endpoint));
            metadata.insert("tags".to_string(), json!(self.tags));
            metadata.insert("query".to_string(), json!(self.query));
            metadata.insert(
                "object_id".to_string(),
                json!(hit.object_id.clone().unwrap_or_default()),
            );
            metadata.insert("story_id".to_string(), json!(hit.story_id));
            metadata.insert("points".to_string(), json!(hit.points));
            metadata.insert("num_comments".to_string(), json!(hit.num_comments));
            metadata.insert("author".to_string(), json!(hit.author.clone()));

            let input = DocumentInput {
                source_id: self.name.clone(),
                source_type: "hacker_news".to_string(),
                external_id: resolve_external_id(&hit),
                url: resolve_url(&hit),
                title: resolve_title(&hit),
                body: build_body(&hit),
                body_format: BodyFormat::PlainText,
                author: hit.author.clone(),
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
        let parsed: SearchResponse = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse Hacker News JSON: {error}"))
        })?;

        let hit = parsed
            .hits
            .into_iter()
            .find(|hit| {
                resolve_external_id(hit) == external_id
                    || hit.story_id.map(|id| id.to_string()) == Some(external_id.to_string())
            })
            .ok_or_else(|| {
                ObjectiveError::Source(format!("Hacker News story not found: {external_id}"))
            })?;

        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
        metadata.insert("endpoint".to_string(), json!(self.endpoint));
        metadata.insert(
            "object_id".to_string(),
            json!(hit.object_id.clone().unwrap_or_default()),
        );
        metadata.insert("story_id".to_string(), json!(hit.story_id));
        metadata.insert("points".to_string(), json!(hit.points));
        metadata.insert("num_comments".to_string(), json!(hit.num_comments));
        metadata.insert("author".to_string(), json!(hit.author.clone()));

        self.normalizer.normalize(DocumentInput {
            source_id: self.name.clone(),
            source_type: "hacker_news".to_string(),
            external_id: resolve_external_id(&hit),
            url: resolve_url(&hit),
            title: resolve_title(&hit),
            body: build_body(&hit),
            body_format: BodyFormat::PlainText,
            author: hit.author.clone(),
            published_at: resolve_published_at(&hit),
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
                ObjectiveError::Source(format!("Hacker News health check failed: {error}"))
            })?;

        Ok(HealthStatus {
            source_id: self.name.clone(),
            status: status.to_string(),
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn rate_limit_config(&self) -> RateLimitConfig {
        RateLimitConfig {
            requests_per_minute: 60,
            burst: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "hits": [
            {
                "objectID": "1",
                "story_id": 40000111,
                "title": "Apple expands chip manufacturing in Austin",
                "url": "https://example.com/apple-chips",
                "author": "alice",
                "created_at": "Tue, 02 Jun 2026 12:00:00 GMT",
                "created_at_i": 1748865600,
                "points": 312,
                "num_comments": 87,
                "story_text": "Apple announced a new chip fab in Austin, Texas with manufacturing capacity details."
            },
            {
                "objectID": "2",
                "story_id": 40000112,
                "title": "Show HN: Local-first knowledge graph",
                "url": "https://example.com/show-hn",
                "author": "bob",
                "created_at": "Tue, 02 Jun 2026 11:00:00 GMT",
                "created_at_i": 1748862000,
                "points": 198,
                "num_comments": 33,
                "story_text": "We open-sourced a Rust-based local knowledge graph and document store."
            }
        ]
    }"#;

    #[tokio::test]
    async fn test_poll_parses_hacker_news_hits_into_documents() {
        let adapter = HackerNewsSourceAdapter::from_json("hn_fixture", FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 2);

        let first = &result.documents[0];
        assert_eq!(first.source_type, "hacker_news");
        assert_eq!(first.external_id, "1");
        assert_eq!(
            first.url.as_deref(),
            Some("https://example.com/apple-chips")
        );
        assert_eq!(first.metadata["points"], json!(312));
        assert!(first.body.contains("Apple"));

        let second = &result.documents[1];
        assert_eq!(second.external_id, "2");
        assert!(second.body.contains("Local-first"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter = HackerNewsSourceAdapter::from_json("hn_fixture", FIXTURE);

        // Cursor is 1 second after the latest item, so all hits are filtered out.
        let result = adapter
            .poll(Some("2026-06-02T12:00:30Z".to_string()))
            .await
            .unwrap();
        assert!(result.documents.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_one_returns_matching_document() {
        let adapter = HackerNewsSourceAdapter::from_json("hn_fixture", FIXTURE);

        let document = adapter.fetch_one("2").await.unwrap();
        assert_eq!(document.external_id, "2");
        assert_eq!(
            document.title.as_deref(),
            Some("Show HN: Local-first knowledge graph")
        );
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter = HackerNewsSourceAdapter::from_json("hn_fixture", FIXTURE);
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }
}
