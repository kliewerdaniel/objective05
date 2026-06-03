use std::{collections::HashMap, io::Cursor, time::Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use feed_rs::model::Entry;
use objective_core::{
    traits::SourceAdapter,
    types::{BodyFormat, HealthStatus, PollResult, RateLimitConfig, RawDocument},
    ObjectiveError, Result,
};
use serde_json::json;

use crate::normalizer::{DocumentInput, DocumentNormalizer};

/// Source adapter for YouTube channel RSS feeds.
///
/// Fetches videos from a channel's Atom feed at
/// `https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}`.
/// Each video is converted to a normalized [`RawDocument`] preserving the
/// title, description, author, and view count metadata.
#[derive(Debug, Clone)]
pub struct YouTubeSourceAdapter {
    name: String,
    channel_id: String,
    endpoint: String,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture_xml: Option<String>,
}

impl YouTubeSourceAdapter {
    /// Create a new adapter for a specific YouTube channel.
    ///
    /// * `name`       – source identifier used in the ingestion event envelope.
    /// * `channel_id` – the YouTube channel ID (e.g. `"UCxxxxxx"`).
    pub fn new(name: impl Into<String>, channel_id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            channel_id: channel_id.into(),
            endpoint: "https://www.youtube.com".to_string(),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture_xml: None,
        }
    }

    /// Build an adapter that returns a pre-canned XML payload (for tests).
    pub fn from_xml(name: impl Into<String>, channel_id: &str, xml: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            channel_id: channel_id.to_string(),
            endpoint: "https://www.youtube.com".to_string(),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture_xml: Some(xml.into()),
        }
    }

    fn build_url(&self) -> String {
        format!(
            "{}/feeds/videos.xml?channel_id={}",
            self.endpoint.trim_end_matches('/'),
            self.channel_id
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
                ObjectiveError::Source(format!("failed to fetch YouTube feed: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "YouTube feed returned HTTP {}",
                response.status()
            )));
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to read YouTube feed body: {error}"))
            })
    }

    fn parse_documents(&self, bytes: &[u8], cursor: Option<String>) -> Result<Vec<RawDocument>> {
        let feed = feed_rs::parser::parse(Cursor::new(bytes)).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse YouTube Atom feed: {error}"))
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

            let input = document_input_from_entry(&self.name, &self.channel_id, entry);
            documents.push(self.normalizer.normalize(input)?);
        }

        Ok(documents)
    }
}

fn document_input_from_entry(source_id: &str, channel_id: &str, entry: &Entry) -> DocumentInput {
    let video_id = entry
        .id
        .rsplit(':')
        .next()
        .unwrap_or("unknown");

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
        .unwrap_or_default();

    let content = entry
        .content
        .as_ref()
        .and_then(|content| content.body.clone())
        .unwrap_or_else(|| summary.clone());

    let media_group = entry.media.first();

    let mut metadata = HashMap::new();
    metadata.insert("channel_id".to_string(), json!(channel_id));
    metadata.insert("video_id".to_string(), json!(video_id));

    if let Some(media) = media_group {
        if let Some(title) = &media.title {
            metadata.insert("media_title".to_string(), json!(title));
        }
        if let Some(description) = &media.description {
            metadata.insert("media_description".to_string(), json!(description));
        }
        if let Some(thumbnail) = media.thumbnails.first() {
            metadata.insert("thumbnail".to_string(), json!(thumbnail.image.uri));
        }
    }

    DocumentInput {
        source_id: source_id.to_string(),
        source_type: "youtube".to_string(),
        external_id: video_id.to_string(),
        url: link,
        title: entry.title.as_ref().map(|title| title.content.clone()),
        body: if content.trim().is_empty() {
            entry
                .title
                .as_ref()
                .map(|title| title.content.clone())
                .unwrap_or_default()
        } else {
            content
        },
        body_format: BodyFormat::PlainText,
        author: entry.authors.first().map(|author| author.name.clone()),
        published_at: entry
            .published
            .or(entry.updated)
            .map(|timestamp| timestamp.with_timezone(&Utc)),
        metadata,
        raw_bytes: None,
    }
}

#[async_trait]
impl SourceAdapter for YouTubeSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "youtube"
    }

    fn validate(&self) -> Result<()> {
        if self.channel_id.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "YouTube channel ID is required".to_string(),
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
                ObjectiveError::Source(format!("YouTube video not found: {external_id}"))
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
                ObjectiveError::Source(format!("YouTube health check failed: {error}"))
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

    const YOUTUBE_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:media="http://search.yahoo.com/mrss/"
      xmlns:yt="http://www.youtube.com/xml/schemas/2015">
  <title>Rust Language</title>
  <link href="https://www.youtube.com/channel/UCzFRDzRn_mYVpPxyrcd_0Gg"/>
  <id>urn:youtube:channel:UCzFRDzRn_mYVpPxyrcd_0Gg</id>
  <entry>
    <id>urn:youtube:video:dQw4w9WgXcQ</id>
    <title>Building a Knowledge Graph in Rust</title>
    <summary>In this tutorial, we build a complete knowledge graph application using Rust, Kuzu, and async/await patterns.</summary>
    <author>
      <name>RustLang</name>
      <uri>https://www.youtube.com/channel/UCzFRDzRn_mYVpPxyrcd_0Gg</uri>
    </author>
    <link href="https://www.youtube.com/watch?v=dQw4w9WgXcQ" rel="alternate" type="text/html"/>
    <published>2026-06-02T12:00:00Z</published>
    <updated>2026-06-02T12:00:00Z</updated>
    <media:group>
      <media:title>Building a Knowledge Graph in Rust</media:title>
      <media:description>We build a complete knowledge graph application using Rust, Kuzu, and async/await patterns.</media:description>
      <media:thumbnail url="https://example.com/thumb.jpg"/>
    </media:group>
  </entry>
  <entry>
    <id>urn:youtube:video:abc123xyz</id>
    <title>Rust Async Runtime Comparison</title>
    <summary>A deep dive comparing Tokio, async-std, and smol for high-performance async applications.</summary>
    <author>
      <name>RustLang</name>
    </author>
    <link href="https://www.youtube.com/watch?v=abc123xyz" rel="alternate" type="text/html"/>
    <published>2026-06-01T10:00:00Z</published>
    <updated>2026-06-01T10:00:00Z</updated>
    <media:group>
      <media:title>Rust Async Runtime Comparison</media:title>
      <media:description>Comparing Tokio, async-std, and smol.</media:description>
    </media:group>
  </entry>
</feed>"#;

    #[tokio::test]
    async fn test_poll_parses_youtube_videos_into_documents() {
        let adapter =
            YouTubeSourceAdapter::from_xml("yt_fixture", "UCzFRDzRn_mYVpPxyrcd_0Gg", YOUTUBE_FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 2);

        let first = &result.documents[0];
        assert_eq!(first.source_type, "youtube");
        assert_eq!(first.external_id, "dQw4w9WgXcQ");
        assert_eq!(
            first.url.as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert!(first.body.to_lowercase().contains("knowledge graph"));
        assert_eq!(
            first.metadata["channel_id"],
            json!("UCzFRDzRn_mYVpPxyrcd_0Gg")
        );
        assert_eq!(first.metadata["video_id"], json!("dQw4w9WgXcQ"));

        let second = &result.documents[1];
        assert_eq!(second.external_id, "abc123xyz");
        assert!(second.body.to_lowercase().contains("async"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter =
            YouTubeSourceAdapter::from_xml("yt_fixture", "UCzFRDzRn_mYVpPxyrcd_0Gg", YOUTUBE_FIXTURE);

        let result = adapter
            .poll(Some("2026-06-02T12:00:30Z".to_string()))
            .await
            .unwrap();
        assert!(result.documents.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_one_returns_matching_document() {
        let adapter =
            YouTubeSourceAdapter::from_xml("yt_fixture", "UCzFRDzRn_mYVpPxyrcd_0Gg", YOUTUBE_FIXTURE);

        let document = adapter.fetch_one("abc123xyz").await.unwrap();
        assert_eq!(document.external_id, "abc123xyz");
        assert!(document.body.to_lowercase().contains("async"));
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter =
            YouTubeSourceAdapter::from_xml("yt_fixture", "UCzFRDzRn_mYVpPxyrcd_0Gg", YOUTUBE_FIXTURE);
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }
}
