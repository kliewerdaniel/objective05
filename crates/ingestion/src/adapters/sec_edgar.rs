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

/// Source adapter for SEC EDGAR full-text search API.
///
/// Fetches recent financial filings from the SEC EDGAR system using the
/// full-text search API. Each filing is converted to a normalized
/// [`RawDocument`] preserving the company, filing type, and date metadata.
#[derive(Debug, Clone)]
pub struct SecEdgarAdapter {
    name: String,
    endpoint: String,
    query: Option<String>,
    form_types: Option<String>,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture: Option<String>,
}

impl SecEdgarAdapter {
    /// Create a new adapter for SEC EDGAR filings.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: "https://efts.sec.gov/LATEST".to_string(),
            query: None,
            form_types: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: None,
        }
    }

    /// Set the search query (e.g., company name or keyword).
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Filter by form types (e.g., "10-K,10-Q,8-K").
    pub fn with_form_types(mut self, form_types: impl Into<String>) -> Self {
        self.form_types = Some(form_types.into());
        self
    }

    /// Build an adapter that returns a pre-canned JSON payload (for tests).
    pub fn from_json(name: impl Into<String>, json_body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: "https://efts.sec.gov/LATEST".to_string(),
            query: None,
            form_types: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: Some(json_body.into()),
        }
    }

    fn build_url(&self) -> String {
        let mut params = Vec::new();
        if let Some(query) = &self.query {
            params.push(format!("q={query}"));
        }
        if let Some(form_types) = &self.form_types {
            params.push(format!("forms={form_types}"));
        }
        params.push("dateRange=custom".to_string());
        params.push("startdt=2024-01-01".to_string());

        let query_string = params.join("&");
        format!(
            "{}/search-index?{query_string}",
            self.endpoint.trim_end_matches('/')
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
                ObjectiveError::Source(format!("failed to fetch SEC EDGAR: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "SEC EDGAR returned HTTP {}",
                response.status()
            )));
        }

        response.text().await.map_err(|error| {
            ObjectiveError::Source(format!("failed to read SEC EDGAR body: {error}"))
        })
    }
}

#[derive(Debug, Deserialize)]
struct EdgarResponse {
    #[serde(default)]
    hits: Option<EdgarHits>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EdgarHits {
    #[serde(default)]
    total: Option<TotalHits>,
    #[serde(default)]
    hits: Vec<EdgarHit>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TotalHits {
    #[serde(default)]
    value: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct EdgarHit {
    #[serde(default)]
    _id: Option<String>,
    #[serde(default, alias = "_source")]
    source: Option<HitSource>,
}

#[derive(Debug, Deserialize)]
struct HitSource {
    #[serde(default, alias = "file_date")]
    file_date: Option<String>,
    #[serde(default, alias = "form_type")]
    form_type: Option<String>,
    #[serde(default, alias = "display_names")]
    display_names: Option<Vec<String>>,
    #[serde(default, alias = "entity_name")]
    entity_name: Option<String>,
    #[serde(default)]
    period_of_report: Option<String>,
    #[serde(default)]
    items: Option<String>,
}

fn build_body(hit: &EdgarHit) -> String {
    let mut parts = Vec::new();

    if let Some(source) = &hit.source {
        if let Some(form_type) = &source.form_type {
            parts.push(format!("Form type: {form_type}."));
        }
        if let Some(names) = &source.display_names {
            if let Some(name) = names.first() {
                parts.push(format!("Company: {name}."));
            }
        } else if let Some(name) = &source.entity_name {
            parts.push(format!("Company: {name}."));
        }
        if let Some(date) = &source.file_date {
            parts.push(format!("Filed: {date}."));
        }
        if let Some(period) = &source.period_of_report {
            parts.push(format!("Period: {period}."));
        }
        if let Some(items) = &source.items {
            parts.push(format!("Items: {items}."));
        }
    }

    if parts.is_empty() {
        return format!(
            "SEC EDGAR filing {}",
            hit._id.as_deref().unwrap_or("unknown")
        );
    }
    parts.join(" ")
}

fn resolve_external_id(hit: &EdgarHit) -> String {
    hit._id.clone().unwrap_or_else(|| "unknown".to_string())
}

fn resolve_company_name(hit: &EdgarHit) -> Option<String> {
    hit.source.as_ref().and_then(|s| {
        s.display_names
            .as_ref()
            .and_then(|names| names.first().cloned())
            .or_else(|| s.entity_name.clone())
    })
}

fn resolve_form_type(hit: &EdgarHit) -> Option<String> {
    hit.source
        .as_ref()
        .and_then(|s| s.form_type.clone())
}

fn resolve_published_at(hit: &EdgarHit) -> Option<DateTime<Utc>> {
    hit.source
        .as_ref()
        .and_then(|s| s.file_date.as_deref())
        .and_then(|date_str| {
            chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|datetime| datetime.and_utc())
        })
}

#[async_trait]
impl SourceAdapter for SecEdgarAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "sec_edgar"
    }

    fn validate(&self) -> Result<()> {
        Ok(())
    }

    async fn poll(&self, cursor: Option<String>) -> Result<PollResult> {
        self.validate()?;
        let started = Instant::now();
        let payload = self.fetch_payload().await?;
        let response: EdgarResponse = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse SEC EDGAR JSON: {error}"))
        })?;

        let cursor_ts = cursor
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let hits = response.hits.map(|h| h.hits).unwrap_or_default();
        let mut documents = Vec::new();
        let mut latest: Option<DateTime<Utc>> = None;

        for hit in hits {
            let published_at = resolve_published_at(&hit);
            if let (Some(cursor), Some(published_at)) = (cursor_ts, published_at) {
                if published_at <= cursor {
                    continue;
                }
            }

            let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
            metadata.insert(
                "company_name".to_string(),
                json!(resolve_company_name(&hit)),
            );
            metadata.insert(
                "form_type".to_string(),
                json!(resolve_form_type(&hit)),
            );

            let input = DocumentInput {
                source_id: self.name.clone(),
                source_type: "sec_edgar".to_string(),
                external_id: resolve_external_id(&hit),
                url: Some(format!(
                    "https://www.sec.gov/cgi-bin/browse-edgar?action=getcompany&type={}&dateb=&owner=include&count=40",
                    resolve_form_type(&hit).unwrap_or_default()
                )),
                title: Some(format!(
                    "{} - {}",
                    resolve_company_name(&hit).unwrap_or_else(|| "Unknown".to_string()),
                    resolve_form_type(&hit).unwrap_or_else(|| "Unknown".to_string())
                )),
                body: build_body(&hit),
                body_format: BodyFormat::PlainText,
                author: resolve_company_name(&hit),
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
        let response: EdgarResponse = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse SEC EDGAR JSON: {error}"))
        })?;

        let hits = response.hits.map(|h| h.hits).unwrap_or_default();
        let hit = hits
            .into_iter()
            .find(|h| resolve_external_id(h) == external_id)
            .ok_or_else(|| {
                ObjectiveError::Source(format!("SEC EDGAR filing not found: {external_id}"))
            })?;

        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
        metadata.insert(
            "company_name".to_string(),
            json!(resolve_company_name(&hit)),
        );
        metadata.insert(
            "form_type".to_string(),
            json!(resolve_form_type(&hit)),
        );

        self.normalizer.normalize(DocumentInput {
            source_id: self.name.clone(),
            source_type: "sec_edgar".to_string(),
            external_id: resolve_external_id(&hit),
            url: Some(format!(
                "https://www.sec.gov/cgi-bin/browse-edgar?action=getcompany&type={}",
                resolve_form_type(&hit).unwrap_or_default()
            )),
            title: Some(format!(
                "{} - {}",
                resolve_company_name(&hit).unwrap_or_else(|| "Unknown".to_string()),
                resolve_form_type(&hit).unwrap_or_else(|| "Unknown".to_string())
            )),
            body: build_body(&hit),
            body_format: BodyFormat::PlainText,
            author: resolve_company_name(&hit),
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
            .get("https://efts.sec.gov/LATEST/search-index?q=苹果&dateRange=custom&startdt=2024-01-01&forms=10-K")
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
                ObjectiveError::Source(format!("SEC EDGAR health check failed: {error}"))
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
            burst: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDGAR_FIXTURE: &str = r#"{
        "hits": {
            "total": {"value": 2},
            "hits": [
                {
                    "_id": "0001234567-26-000001",
                    "_source": {
                        "file_date": "2026-06-01",
                        "form_type": "10-K",
                        "display_names": ["Apple Inc"],
                        "period_of_report": "2026-03-31",
                        "items": "1,1A,2,3,4,5,6,7,7A,8,9,9A,10,11,12,13,14,15"
                    }
                },
                {
                    "_id": "0001234567-26-000002",
                    "_source": {
                        "file_date": "2026-06-03",
                        "form_type": "8-K",
                        "display_names": ["Tesla, Inc."],
                        "period_of_report": "2026-06-01",
                        "items": "2.01,5.07,9.01"
                    }
                }
            ]
        }
    }"#;

    #[tokio::test]
    async fn test_poll_parses_edgar_filings_into_documents() {
        let adapter = SecEdgarAdapter::from_json("sec_fixture", EDGAR_FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 2);

        let first = &result.documents[0];
        assert_eq!(first.source_type, "sec_edgar");
        assert_eq!(first.external_id, "0001234567-26-000001");
        assert!(first.body.contains("10-K"));
        assert!(first.body.contains("Apple Inc"));
        assert_eq!(first.metadata["form_type"], json!("10-K"));

        let second = &result.documents[1];
        assert_eq!(second.external_id, "0001234567-26-000002");
        assert!(second.body.contains("8-K"));
        assert!(second.body.contains("Tesla"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter = SecEdgarAdapter::from_json("sec_fixture", EDGAR_FIXTURE);

        let result = adapter
            .poll(Some("2026-06-01T12:00:00Z".to_string()))
            .await
            .unwrap();
        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].external_id, "0001234567-26-000002");
    }

    #[tokio::test]
    async fn test_fetch_one_returns_matching_document() {
        let adapter = SecEdgarAdapter::from_json("sec_fixture", EDGAR_FIXTURE);

        let document = adapter
            .fetch_one("0001234567-26-000002")
            .await
            .unwrap();
        assert_eq!(document.external_id, "0001234567-26-000002");
        assert!(document.body.contains("Tesla"));
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter = SecEdgarAdapter::from_json("sec_fixture", EDGAR_FIXTURE);
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }
}
