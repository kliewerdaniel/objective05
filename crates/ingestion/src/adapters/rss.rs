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

#[derive(Debug, Clone)]
pub struct RssSourceAdapter {
    name: String,
    feed_url: String,
    fixture_xml: Option<String>,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
}

impl RssSourceAdapter {
    pub fn new(name: impl Into<String>, feed_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            feed_url: feed_url.into(),
            fixture_xml: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
        }
    }

    pub fn from_xml(
        name: impl Into<String>,
        feed_url: impl Into<String>,
        xml: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            feed_url: feed_url.into(),
            fixture_xml: Some(xml.into()),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
        }
    }

    async fn fetch_feed_bytes(&self) -> Result<Vec<u8>> {
        if let Some(xml) = &self.fixture_xml {
            return Ok(xml.as_bytes().to_vec());
        }

        let response = self
            .client
            .get(&self.feed_url)
            .header(
                reqwest::header::USER_AGENT,
                "Objective/0.1 (local intelligence system)",
            )
            .header(
                reqwest::header::ACCEPT,
                "application/rss+xml, application/atom+xml, application/xml, text/xml",
            )
            .send()
            .await
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to fetch RSS feed: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "RSS feed returned HTTP {}",
                response.status()
            )));
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to read RSS feed body: {error}"))
            })
    }

    fn parse_documents(&self, bytes: &[u8], cursor: Option<String>) -> Result<Vec<RawDocument>> {
        let feed = feed_rs::parser::parse(Cursor::new(bytes)).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse RSS/Atom feed: {error}"))
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

            let input = document_input_from_entry(&self.name, &self.feed_url, &feed, entry);
            documents.push(self.normalizer.normalize(input)?);
        }

        Ok(documents)
    }
}

#[async_trait]
impl SourceAdapter for RssSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "rss"
    }

    fn validate(&self) -> Result<()> {
        if self.feed_url.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "RSS feed URL is required".to_string(),
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
            .ok_or_else(|| ObjectiveError::Source(format!("RSS entry not found: {external_id}")))
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
            .get(&self.feed_url)
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
            .map_err(|error| ObjectiveError::Source(format!("RSS health check failed: {error}")))?;

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

fn document_input_from_entry(
    source_id: &str,
    feed_url: &str,
    feed: &Feed,
    entry: &Entry,
) -> DocumentInput {
    let link = entry.links.first().map(|link| link.href.clone());
    let summary = entry
        .summary
        .as_ref()
        .map(|summary| summary.content.clone());
    let content = entry
        .content
        .as_ref()
        .and_then(|content| content.body.clone())
        .or(summary)
        .unwrap_or_else(|| {
            entry
                .title
                .as_ref()
                .map(|title| title.content.clone())
                .unwrap_or_default()
        });
    let categories = entry
        .categories
        .iter()
        .map(|category| category.term.clone())
        .collect::<Vec<_>>();
    let mut metadata = HashMap::new();
    metadata.insert(
        "feed_title".to_string(),
        json!(feed.title.as_ref().map(|title| title.content.clone())),
    );
    metadata.insert("feed_url".to_string(), json!(feed_url));
    metadata.insert("categories".to_string(), json!(categories));
    metadata.insert(
        "description".to_string(),
        json!(entry
            .summary
            .as_ref()
            .map(|summary| summary.content.clone())),
    );

    DocumentInput {
        source_id: source_id.to_string(),
        source_type: "rss".to_string(),
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
        body_format: BodyFormat::Html,
        author: entry.authors.first().map(|author| author.name.clone()),
        published_at: entry
            .published
            .or(entry.updated)
            .map(|timestamp| timestamp.with_timezone(&Utc)),
        metadata,
        raw_bytes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <title>Fixture Feed</title>
    <link>https://example.com</link>
    <description>Fixture feed</description>
    <item>
      <title>Apple expands Austin manufacturing</title>
      <link>https://example.com/apple-austin</link>
      <guid>apple-austin</guid>
      <pubDate>Tue, 02 Jun 2026 12:00:00 GMT</pubDate>
      <description><![CDATA[Apple Inc announced a 10% manufacturing expansion in Austin with details from local officials.]]></description>
      <category>Business</category>
    </item>
  </channel>
</rss>"#;

    #[tokio::test]
    async fn test_poll_parses_rss_entries_into_documents() {
        let adapter =
            RssSourceAdapter::from_xml("fixture_rss", "https://example.com/rss.xml", RSS_FIXTURE);

        let result = adapter.poll(None).await.unwrap();

        assert_eq!(result.documents.len(), 1);
        let document = &result.documents[0];
        assert_eq!(document.source_type, "rss");
        assert_eq!(document.external_id, "apple-austin");
        assert_eq!(
            document.url.as_deref(),
            Some("https://example.com/apple-austin")
        );
        assert_eq!(document.metadata["feed_title"], json!("Fixture Feed"));
        assert!(document.body.contains("Apple Inc announced"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter =
            RssSourceAdapter::from_xml("fixture_rss", "https://example.com/rss.xml", RSS_FIXTURE);

        let result = adapter
            .poll(Some("2026-06-03T00:00:00Z".to_string()))
            .await
            .unwrap();

        assert!(result.documents.is_empty());
    }
}
