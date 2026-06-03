use std::{collections::HashMap, io::Cursor, time::Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use feed_rs::model::{Entry, Feed};
use objective_core::{
    traits::SourceAdapter,
    types::{BodyFormat, HealthStatus, PollResult, RateLimitConfig, RawDocument},
    ObjectiveError, Result,
};
use serde_json::json;

use crate::normalizer::{DocumentInput, DocumentNormalizer};

/// Source adapter for the arXiv API.
///
/// Fetches academic papers from `http://export.arxiv.org/api/query` using
/// Atom XML. Each paper is converted to a normalized [`RawDocument`]
/// preserving the title, authors, abstract, and category metadata.
#[derive(Debug, Clone)]
pub struct ArxivSourceAdapter {
    name: String,
    endpoint: String,
    search_query: String,
    max_results: u32,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture_xml: Option<String>,
}

impl ArxivSourceAdapter {
    /// Create a new adapter with a search query.
    ///
    /// * `name`          – source identifier used in the ingestion event envelope.
    /// * `search_query`  – arXiv search query (e.g. `"cat:cs.AI"`, `"all:rust"`).
    pub fn new(name: impl Into<String>, search_query: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: "http://export.arxiv.org/api/query".to_string(),
            search_query: search_query.into(),
            max_results: 50,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture_xml: None,
        }
    }

    /// Set the maximum number of results to return.
    pub fn with_max_results(mut self, max_results: u32) -> Self {
        self.max_results = max_results;
        self
    }

    /// Build an adapter that returns a pre-canned XML payload (for tests).
    pub fn from_xml(name: impl Into<String>, search_query: &str, xml: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: "http://export.arxiv.org/api/query".to_string(),
            search_query: search_query.to_string(),
            max_results: 50,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture_xml: Some(xml.into()),
        }
    }

    fn build_url(&self) -> String {
        format!(
            "{}?search_query={}&max_results={}&sortBy=submittedDate&sortOrder=descending",
            self.endpoint.trim_end_matches('/'),
            self.search_query,
            self.max_results
        )
    }

    async fn fetch_feed_bytes(&self) -> Result<Vec<u8>> {
        if let Some(xml) = &self.fixture_xml {
            return Ok(xml.as_bytes().to_vec());
        }

        let response = self
            .client
            .get(self.build_url())
            .header(
                reqwest::header::USER_AGENT,
                "Objective/0.1 (local intelligence system)",
            )
            .header(
                reqwest::header::ACCEPT,
                "application/atom+xml, application/xml, text/xml",
            )
            .send()
            .await
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to fetch arXiv feed: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "arXiv API returned HTTP {}",
                response.status()
            )));
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to read arXiv feed body: {error}"))
            })
    }

    fn parse_documents(&self, bytes: &[u8], cursor: Option<String>) -> Result<Vec<RawDocument>> {
        let feed = feed_rs::parser::parse(Cursor::new(bytes)).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse arXiv Atom feed: {error}"))
        })?;

        let cursor = cursor.and_then(|value| DateTime::parse_from_rfc3339(&value).ok());
        let mut documents = Vec::new();

        for entry in &feed.entries {
            let published_at = entry
                .published
                .or(entry.updated)
                .map(|timestamp| timestamp.with_timezone(&Utc));

            if let (Some(cursor), Some(published_at)) = (cursor, published_at) {
                if published_at <= cursor.with_timezone(&Utc) {
                    continue;
                }
            }

            let input = document_input_from_entry(&self.name, &self.search_query, &feed, entry);
            documents.push(self.normalizer.normalize(input)?);
        }

        Ok(documents)
    }
}

fn document_input_from_entry(
    source_id: &str,
    search_query: &str,
    feed: &Feed,
    entry: &Entry,
) -> DocumentInput {
    let link = entry
        .links
        .iter()
        .find(|link| link.rel == Some("alternate".to_string()))
        .map(|link| link.href.clone())
        .or_else(|| entry.links.first().map(|link| link.href.clone()));

    let summary = entry
        .summary
        .as_ref()
        .map(|summary| summary.content.clone())
        .unwrap_or_else(|| {
            entry
                .title
                .as_ref()
                .map(|title| title.content.clone())
                .unwrap_or_default()
        });

    let authors: Vec<String> = entry
        .authors
        .iter()
        .map(|author| author.name.clone())
        .collect();

    let categories: Vec<String> = entry
        .categories
        .iter()
        .map(|category| category.term.clone())
        .collect();

    let mut metadata = HashMap::new();
    metadata.insert(
        "feed_title".to_string(),
        json!(feed.title.as_ref().map(|title| title.content.clone())),
    );
    metadata.insert("search_query".to_string(), json!(search_query));
    metadata.insert("authors".to_string(), json!(authors));
    metadata.insert("categories".to_string(), json!(categories));

    let content = entry
        .content
        .as_ref()
        .and_then(|content| content.body.clone())
        .unwrap_or_else(|| summary.clone());

    DocumentInput {
        source_id: source_id.to_string(),
        source_type: "arxiv".to_string(),
        external_id: if entry.id.trim().is_empty() {
            link.clone().unwrap_or_else(|| {
                entry
                    .title
                    .as_ref()
                    .map(|title| title.content.clone())
                    .unwrap_or_default()
            })
        } else {
            entry.id.clone()
        },
        url: link,
        title: entry.title.as_ref().map(|title| title.content.clone()),
        body: content,
        body_format: BodyFormat::PlainText,
        author: authors.first().cloned(),
        published_at: entry
            .published
            .or(entry.updated)
            .map(|timestamp| timestamp.with_timezone(&Utc)),
        metadata,
        raw_bytes: None,
    }
}

#[async_trait]
impl SourceAdapter for ArxivSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "arxiv"
    }

    fn validate(&self) -> Result<()> {
        if self.search_query.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "arXiv search query is required".to_string(),
            ));
        }
        Ok(())
    }

    async fn poll(&self, cursor: Option<String>) -> Result<PollResult> {
        self.validate()?;
        let started = Instant::now();
        let bytes = self.fetch_feed_bytes().await?;
        let documents = self.parse_documents(&bytes, cursor)?;
        let new_cursor = documents
            .iter()
            .filter_map(|document| document.published_at)
            .max()
            .map(|timestamp| timestamp.to_rfc3339());

        Ok(PollResult {
            documents,
            new_cursor,
            has_more: false,
            poll_duration: started.elapsed(),
        })
    }

    async fn fetch_one(&self, external_id: &str) -> Result<RawDocument> {
        let bytes = self.fetch_feed_bytes().await?;
        self.parse_documents(&bytes, None)?
            .into_iter()
            .find(|document| {
                document.external_id == external_id || document.url.as_deref() == Some(external_id)
            })
            .ok_or_else(|| {
                ObjectiveError::Source(format!("arXiv paper not found: {external_id}"))
            })
    }

    async fn health(&self) -> Result<HealthStatus> {
        if self.fixture_xml.is_some() {
            return Ok(HealthStatus {
                source_id: self.name.clone(),
                status: "healthy".to_string(),
                latency_ms: 0,
            });
        }

        let started = Instant::now();
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
                ObjectiveError::Source(format!("arXiv health check failed: {error}"))
            })?;

        Ok(HealthStatus {
            source_id: self.name.clone(),
            status: status.to_string(),
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn rate_limit_config(&self) -> RateLimitConfig {
        RateLimitConfig {
            requests_per_minute: 10,
            burst: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARXIV_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>arXiv: cs.AI Recent Submissions</title>
  <link href="http://export.arxiv.org/api/query?search_query=cat:cs.AI"/>
  <id>http://arxiv.org/rss/cs.AI</id>
  <entry>
    <id>http://arxiv.org/abs/2406.12345v1</id>
    <title>Attention Is All You Need: A Survey of Transformer Applications</title>
    <summary>We present a comprehensive survey of transformer architectures applied to natural language processing, computer vision, and beyond. This paper reviews 200+ applications and identifies key design patterns.</summary>
    <author>
      <name>Yann Lecun</name>
    </author>
    <author>
      <name>Ilya Sutskever</name>
    </author>
    <category term="cs.AI" scheme="http://arxiv.org/schemas/atom"/>
    <category term="cs.CL" scheme="http://arxiv.org/schemas/atom"/>
    <link href="https://arxiv.org/abs/2406.12345v1" rel="alternate" type="text/html"/>
    <published>2026-06-02T12:00:00Z</published>
    <updated>2026-06-02T12:00:00Z</updated>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/2406.12346v1</id>
    <title>Rust for Systems Programming: Memory Safety Without Garbage Collection</title>
    <summary>This paper examines Rust's ownership model for systems programming and demonstrates zero-cost abstractions for memory safety in high-performance computing scenarios.</summary>
    <author>
      <name>Andrea Bocelli</name>
    </author>
    <category term="cs.PL" scheme="http://arxiv.org/schemas/atom"/>
    <link href="https://arxiv.org/abs/2406.12346v1" rel="alternate" type="text/html"/>
    <published>2026-06-02T10:00:00Z</published>
    <updated>2026-06-02T10:00:00Z</updated>
  </entry>
</feed>"#;

    #[tokio::test]
    async fn test_poll_parses_arxiv_entries_into_documents() {
        let adapter = ArxivSourceAdapter::from_xml("arxiv_fixture", "cat:cs.AI", ARXIV_FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 2);

        let first = &result.documents[0];
        assert_eq!(first.source_type, "arxiv");
        assert_eq!(first.external_id, "http://arxiv.org/abs/2406.12345v1");
        assert_eq!(
            first.url.as_deref(),
            Some("https://arxiv.org/abs/2406.12345v1")
        );
        assert!(first.body.to_lowercase().contains("transformer"));
        assert_eq!(first.metadata["search_query"], json!("cat:cs.AI"));
        assert_eq!(
            first.metadata["categories"],
            json!(["cs.AI", "cs.CL"])
        );

        let second = &result.documents[1];
        assert_eq!(second.external_id, "http://arxiv.org/abs/2406.12346v1");
        assert!(second.body.to_lowercase().contains("rust"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter = ArxivSourceAdapter::from_xml("arxiv_fixture", "cat:cs.AI", ARXIV_FIXTURE);

        let result = adapter
            .poll(Some("2026-06-02T12:00:30Z".to_string()))
            .await
            .unwrap();
        assert!(result.documents.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_one_returns_matching_document() {
        let adapter = ArxivSourceAdapter::from_xml("arxiv_fixture", "cat:cs.AI", ARXIV_FIXTURE);

        let document = adapter
            .fetch_one("http://arxiv.org/abs/2406.12346v1")
            .await
            .unwrap();
        assert_eq!(document.external_id, "http://arxiv.org/abs/2406.12346v1");
        assert!(document.body.contains("Rust"));
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter = ArxivSourceAdapter::from_xml("arxiv_fixture", "cat:cs.AI", ARXIV_FIXTURE);
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }
}
