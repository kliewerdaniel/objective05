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

/// Source adapter for podcast RSS feeds.
///
/// Extends the standard RSS adapter to extract audio-specific metadata
/// such as duration, enclosure URL, and episode number. Each episode
/// is converted to a normalized [`RawDocument`].
#[derive(Debug, Clone)]
pub struct PodcastSourceAdapter {
    name: String,
    feed_url: String,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture_xml: Option<String>,
}

impl PodcastSourceAdapter {
    /// Create a new adapter for a podcast RSS feed.
    pub fn new(name: impl Into<String>, feed_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            feed_url: feed_url.into(),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture_xml: None,
        }
    }

    /// Build an adapter that returns a pre-canned XML payload (for tests).
    pub fn from_xml(name: impl Into<String>, feed_url: &str, xml: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            feed_url: feed_url.to_string(),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture_xml: Some(xml.into()),
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
                ObjectiveError::Source(format!("failed to fetch podcast feed: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "podcast feed returned HTTP {}",
                response.status()
            )));
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to read podcast feed body: {error}"))
            })
    }

    fn parse_documents(&self, bytes: &[u8], cursor: Option<String>) -> Result<Vec<RawDocument>> {
        let feed = feed_rs::parser::parse(Cursor::new(bytes)).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse podcast feed: {error}"))
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

            let input = document_input_from_entry(&self.name, &feed.title.as_ref().map(|t| t.content.clone()).unwrap_or_default(), entry);
            documents.push(self.normalizer.normalize(input)?);
        }

        Ok(documents)
    }
}

fn document_input_from_entry(source_id: &str, podcast_title: &str, entry: &Entry) -> DocumentInput {
    let link = entry.links.first().map(|link| link.href.clone());

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

    // Extract podcast-specific metadata from iTunes extensions
    let mut duration = None;
    let mut enclosure_url = None;
    let episode_number: Option<u32> = None;
    let season_number: Option<u32> = None;
    let episode_type: Option<String> = None;

    // Check for enclosure in links
    for link in &entry.links {
        if link.media_type.as_deref() == Some("audio/mpeg")
            || link.media_type.as_deref() == Some("audio/x-m4a")
            || link.media_type.as_deref() == Some("audio/ogg")
        {
            enclosure_url = Some(link.href.clone());
            break;
        }
    }

    // Extract from media group if available
    for media in &entry.media {
        if let Some(content) = media.content.first() {
            if let Some(url) = &content.url {
                enclosure_url = Some(url.to_string());
            }
        }
        if let Some(duration_val) = media.duration {
            duration = Some(format_duration(duration_val));
        }
    }

    let mut metadata = HashMap::new();
    metadata.insert("podcast_title".to_string(), json!(podcast_title));
    metadata.insert("feed_url".to_string(), json!(""));
    if let Some(url) = &enclosure_url {
        metadata.insert("enclosure_url".to_string(), json!(url));
    }
    if let Some(dur) = &duration {
        metadata.insert("duration".to_string(), json!(dur));
    }
    if let Some(ep) = episode_number {
        metadata.insert("episode_number".to_string(), json!(ep));
    }
    if let Some(season) = season_number {
        metadata.insert("season_number".to_string(), json!(season));
    }
    if let Some(ep_type) = &episode_type {
        metadata.insert("episode_type".to_string(), json!(ep_type));
    }

    let title = entry.title.as_ref().map(|title| {
        format!("{} - {}", podcast_title, title.content)
    });

    DocumentInput {
        source_id: source_id.to_string(),
        source_type: "podcast".to_string(),
        external_id: entry.id.clone(),
        url: link,
        title,
        body: if content.trim().is_empty() {
            summary
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

fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[async_trait]
impl SourceAdapter for PodcastSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "podcast"
    }

    fn validate(&self) -> Result<()> {
        if self.feed_url.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "podcast feed URL is required".to_string(),
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
            .find(|document| document.external_id == external_id)
            .ok_or_else(|| {
                ObjectiveError::Source(format!("podcast episode not found: {external_id}"))
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
            .map_err(|error| {
                ObjectiveError::Source(format!("podcast health check failed: {error}"))
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

    const PODCAST_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
  <channel>
    <title>Tech Daily Podcast</title>
    <link>https://example.com/podcast</link>
    <description>Daily technology news and analysis</description>
    <item>
      <title>Episode 42: The Future of Rust in Systems Programming</title>
      <link>https://example.com/podcast/42</link>
      <guid>podcast-42</guid>
      <pubDate>Tue, 02 Jun 2026 12:00:00 GMT</pubDate>
      <description><![CDATA[In this episode, we discuss the growing adoption of Rust in operating systems, web browsers, and embedded devices.]]></description>
      <enclosure url="https://example.com/episodes/42.mp3" length="52428800" type="audio/mpeg"/>
      <itunes:duration>45:30</itunes:duration>
      <itunes:episode>42</itunes:episode>
      <itunes:season>3</itunes:season>
      <itunes:episodeType>full</itunes:episodeType>
    </item>
    <item>
      <title>Episode 41: AI Agents and Autonomous Systems</title>
      <link>https://example.com/podcast/41</link>
      <guid>podcast-41</guid>
      <pubDate>Mon, 01 Jun 2026 12:00:00 GMT</pubDate>
      <description><![CDATA[Exploring the latest developments in AI agents and how they're being deployed in production systems.]]></description>
      <enclosure url="https://example.com/episodes/41.mp3" length="48150528" type="audio/mpeg"/>
      <itunes:duration>38:15</itunes:duration>
      <itunes:episode>41</itunes:episode>
      <itunes:season>3</itunes:season>
      <itunes:episodeType>full</itunes:episodeType>
    </item>
  </channel>
</rss>"#;

    #[tokio::test]
    async fn test_poll_parses_podcast_episodes_into_documents() {
        let adapter =
            PodcastSourceAdapter::from_xml("podcast_fixture", "https://example.com/feed.xml", PODCAST_FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 2);

        let first = &result.documents[0];
        assert_eq!(first.source_type, "podcast");
        assert_eq!(first.external_id, "podcast-42");
        assert!(first.title.as_deref().unwrap().contains("Future of Rust"));
        assert!(first.body.contains("Rust in operating systems"));
        assert_eq!(first.metadata["podcast_title"], json!("Tech Daily Podcast"));

        let second = &result.documents[1];
        assert_eq!(second.external_id, "podcast-41");
        assert!(second.body.contains("AI agents"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter =
            PodcastSourceAdapter::from_xml("podcast_fixture", "https://example.com/feed.xml", PODCAST_FIXTURE);

        let result = adapter
            .poll(Some("2026-06-02T12:00:30Z".to_string()))
            .await
            .unwrap();
        assert!(result.documents.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_one_returns_matching_document() {
        let adapter =
            PodcastSourceAdapter::from_xml("podcast_fixture", "https://example.com/feed.xml", PODCAST_FIXTURE);

        let document = adapter.fetch_one("podcast-41").await.unwrap();
        assert_eq!(document.external_id, "podcast-41");
        assert!(document.body.contains("AI agents"));
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter =
            PodcastSourceAdapter::from_xml("podcast_fixture", "https://example.com/feed.xml", PODCAST_FIXTURE);
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(
            format_duration(std::time::Duration::from_secs(2730)),
            "45:30"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_secs(3661)),
            "1:01:01"
        );
        assert_eq!(
            format_duration(std::time::Duration::from_secs(59)),
            "0:59"
        );
    }
}
