use std::{collections::HashMap, time::Instant};

use async_trait::async_trait;
use chrono::Utc;
use objective_core::{
    traits::SourceAdapter,
    types::{BodyFormat, HealthStatus, PollResult, RateLimitConfig, RawDocument},
    ObjectiveError, Result,
};
use regex::Regex;
use serde_json::json;

use crate::normalizer::{DocumentInput, DocumentNormalizer};

/// Generic source adapter for fetching and extracting text from web pages.
///
/// Fetches a URL, extracts the text content from the HTML, and normalizes
/// it into a [`RawDocument`]. This adapter is useful for monitoring
/// specific web pages that don't have dedicated API or feed support.
#[derive(Debug, Clone)]
pub struct WebSourceAdapter {
    name: String,
    url: String,
    selector: Option<String>,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture_html: Option<String>,
}

impl WebSourceAdapter {
    /// Create a new adapter for a specific URL.
    ///
    /// * `name` – source identifier used in the ingestion event envelope.
    /// * `url`  – the web page URL to fetch.
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            selector: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture_html: None,
        }
    }

    /// Set a CSS selector to extract specific content (for future use).
    pub fn with_selector(mut self, selector: impl Into<String>) -> Self {
        self.selector = Some(selector.into());
        self
    }

    /// Build an adapter that returns a pre-canned HTML payload (for tests).
    pub fn from_html(name: impl Into<String>, url: &str, html: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.to_string(),
            selector: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture_html: Some(html.into()),
        }
    }

    async fn fetch_html(&self) -> Result<String> {
        if let Some(html) = &self.fixture_html {
            return Ok(html.clone());
        }

        let response = self
            .client
            .get(&self.url)
            .header(
                reqwest::header::USER_AGENT,
                "Objective/0.1 (local intelligence system)",
            )
            .header(reqwest::header::ACCEPT, "text/html, text/plain")
            .send()
            .await
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to fetch web page: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "web page returned HTTP {}",
                response.status()
            )));
        }

        response.text().await.map_err(|error| {
            ObjectiveError::Source(format!("failed to read web page body: {error}"))
        })
    }

    fn extract_title(html: &str) -> Option<String> {
        let re = Regex::new(r"(?i)<title[^>]*>([^<]+)</title>").ok()?;
        let caps = re.captures(html)?;
        Some(caps.get(1)?.as_str().trim().to_string())
    }

    fn extract_text(html: &str) -> String {
        let tags = Regex::new(r"(?i)<script[^>]*>[\s\S]*?</script>").expect("valid script regex");
        let html = tags.replace_all(html, " ");
        let tags = Regex::new(r"(?i)<style[^>]*>[\s\S]*?</style>").expect("valid style regex");
        let html = tags.replace_all(&html, " ");
        let tags = Regex::new(r"(?i)<[^>]+>").expect("valid html tag regex");
        let text = tags.replace_all(&html, " ");
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn extract_meta_description(html: &str) -> Option<String> {
        let re = Regex::new(r#"(?i)<meta\s+name=["']description["']\s+content=["']([^"']+)["']"#)
            .ok()?;
        let caps = re.captures(html)?;
        Some(caps.get(1)?.as_str().trim().to_string())
    }

    fn extract_metadata(html: &str) -> HashMap<String, serde_json::Value> {
        let mut metadata = HashMap::new();

        if let Some(description) = Self::extract_meta_description(html) {
            metadata.insert("meta_description".to_string(), json!(description));
        }

        let re = Regex::new(r#"(?i)<meta\s+name=["']author["']\s+content=["']([^"']+)["']"#);
        if let Ok(re) = re {
            if let Some(caps) = re.captures(html) {
                if let Some(author) = caps.get(1) {
                    metadata.insert("meta_author".to_string(), json!(author.as_str().trim()));
                }
            }
        }

        metadata
    }
}

#[async_trait]
impl SourceAdapter for WebSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "web"
    }

    fn validate(&self) -> Result<()> {
        if self.url.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "web page URL is required".to_string(),
            ));
        }
        Ok(())
    }

    async fn poll(&self, _cursor: Option<String>) -> Result<PollResult> {
        self.validate()?;
        let started = Instant::now();
        let html = self.fetch_html().await?;

        let title = Self::extract_title(&html);
        let text = Self::extract_text(&html);

        if text.len() < 40 {
            return Err(ObjectiveError::Source(
                "web page content is too short".to_string(),
            ));
        }

        let mut metadata = Self::extract_metadata(&html);
        metadata.insert("url".to_string(), json!(self.url));
        metadata.insert("source_type".to_string(), json!("web"));
        if let Some(selector) = &self.selector {
            metadata.insert("selector".to_string(), json!(selector));
        }

        let input = DocumentInput {
            source_id: self.name.clone(),
            source_type: "web".to_string(),
            external_id: self.url.clone(),
            url: Some(self.url.clone()),
            title,
            body: text,
            body_format: BodyFormat::Html,
            author: metadata
                .get("meta_author")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            published_at: Some(Utc::now()),
            metadata,
            raw_bytes: None,
        };

        let document = self.normalizer.normalize(input)?;

        Ok(PollResult {
            documents: vec![document],
            new_cursor: Some(Utc::now().to_rfc3339()),
            has_more: false,
            poll_duration: started.elapsed(),
        })
    }

    async fn fetch_one(&self, _external_id: &str) -> Result<RawDocument> {
        let result = self.poll(None).await?;
        result
            .documents
            .into_iter()
            .next()
            .ok_or_else(|| ObjectiveError::Source("web page produced no documents".to_string()))
    }

    async fn health(&self) -> Result<HealthStatus> {
        let started = Instant::now();
        if self.fixture_html.is_some() {
            return Ok(HealthStatus {
                source_id: self.name.clone(),
                status: "healthy".to_string(),
                latency_ms: 0,
            });
        }

        let status = self
            .client
            .get(&self.url)
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
            .map_err(|error| ObjectiveError::Source(format!("web health check failed: {error}")))?;

        Ok(HealthStatus {
            source_id: self.name.clone(),
            status: status.to_string(),
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn rate_limit_config(&self) -> RateLimitConfig {
        RateLimitConfig {
            requests_per_minute: 10,
            burst: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML_FIXTURE: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>Example News Article - Breaking Technology News</title>
    <meta name="description" content="Latest technology news and analysis from around the world."/>
    <meta name="author" content="Jane Smith"/>
</head>
<body>
    <header>
        <h1>Tech Industry Update</h1>
    </header>
    <main>
        <article>
            <h2>AI Models Reach New Milestone in Reasoning Capabilities</h2>
            <p>Researchers at major AI labs have announced a breakthrough in reasoning capabilities, with new models demonstrating unprecedented performance on complex mathematical and logical reasoning tasks.</p>
            <p>The advancement comes from improved training techniques that focus on chain-of-thought reasoning and self-verification methods.</p>
            <p>Industry experts suggest this could accelerate the adoption of AI in scientific research and engineering applications.</p>
        </article>
    </main>
    <script>
        // This should be stripped
        console.log('analytics tracking');
    </script>
</body>
</html>"#;

    #[tokio::test]
    async fn test_poll_extracts_text_from_html() {
        let adapter =
            WebSourceAdapter::from_html("web_fixture", "https://example.com/news", HTML_FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 1);

        let document = &result.documents[0];
        assert_eq!(document.source_type, "web");
        assert_eq!(document.external_id, "https://example.com/news");
        assert_eq!(document.url.as_deref(), Some("https://example.com/news"));
        assert_eq!(
            document.title.as_deref(),
            Some("Example News Article - Breaking Technology News")
        );
        assert!(document.body.contains("AI Models Reach New Milestone"));
        assert!(!document.body.contains("console.log"));
        assert!(document.body.contains("chain-of-thought reasoning"));
        assert_eq!(document.metadata["url"], json!("https://example.com/news"));
    }

    #[tokio::test]
    async fn test_poll_extracts_metadata() {
        let adapter =
            WebSourceAdapter::from_html("web_fixture", "https://example.com/news", HTML_FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        let document = &result.documents[0];
        assert_eq!(
            document
                .metadata
                .get("meta_author")
                .and_then(|v| v.as_str()),
            Some("Jane Smith")
        );
        assert_eq!(
            document
                .metadata
                .get("meta_description")
                .and_then(|v| v.as_str()),
            Some("Latest technology news and analysis from around the world.")
        );
    }

    #[tokio::test]
    async fn test_fetch_one_returns_document() {
        let adapter =
            WebSourceAdapter::from_html("web_fixture", "https://example.com/news", HTML_FIXTURE);

        let document = adapter.fetch_one("https://example.com/news").await.unwrap();
        assert!(document.body.contains("AI Models"));
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter =
            WebSourceAdapter::from_html("web_fixture", "https://example.com/news", HTML_FIXTURE);
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }

    #[test]
    fn test_extract_title() {
        let html = "<html><head><title>Test Title</title></head></html>";
        assert_eq!(
            WebSourceAdapter::extract_title(html),
            Some("Test Title".to_string())
        );
    }

    #[test]
    fn test_extract_text_strips_tags() {
        let html = "<html><body><p>Hello <strong>world</strong></p></body></html>";
        let text = WebSourceAdapter::extract_text(html);
        assert_eq!(text, "Hello world");
    }
}
