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

/// Source adapter for GitHub public events API.
///
/// Fetches public events from `https://api.github.com/events` without
/// authentication. Each event is converted to a normalized [`RawDocument`]
/// preserving the event type, actor, and repository metadata.
#[derive(Debug, Clone)]
pub struct GitHubSourceAdapter {
    name: String,
    endpoint: String,
    org: Option<String>,
    repo: Option<String>,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture: Option<String>,
}

impl GitHubSourceAdapter {
    /// Create a new adapter pointing at the GitHub Events API.
    pub fn new(name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: endpoint.into(),
            org: None,
            repo: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: None,
        }
    }

    /// Restrict results to events for a specific organization.
    pub fn with_org(mut self, org: impl Into<String>) -> Self {
        self.org = Some(org.into());
        self
    }

    /// Restrict results to events for a specific repository.
    pub fn with_repo(mut self, repo: impl Into<String>) -> Self {
        self.repo = Some(repo.into());
        self
    }

    /// Build an adapter that returns a pre-canned JSON payload (for tests).
    pub fn from_json(name: impl Into<String>, json_body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: "https://api.github.com".to_string(),
            org: None,
            repo: None,
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: Some(json_body.into()),
        }
    }

    fn build_url(&self) -> String {
        let base = self.endpoint.trim_end_matches('/');
        if let (Some(org), Some(repo)) = (self.org.as_deref(), self.repo.as_deref()) {
            format!("{base}/repos/{org}/{repo}/events")
        } else if let Some(org) = self.org.as_deref() {
            format!("{base}/orgs/{org}/events")
        } else {
            format!("{base}/events")
        }
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
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to fetch GitHub events: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "GitHub API returned HTTP {}",
                response.status()
            )));
        }

        response.text().await.map_err(|error| {
            ObjectiveError::Source(format!("failed to read GitHub events body: {error}"))
        })
    }
}

#[derive(Debug, Deserialize)]
struct Event {
    #[serde(default, alias = "type")]
    event_type: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    actor: Option<Actor>,
    #[serde(default)]
    repo: Option<Repo>,
    #[serde(default)]
    payload: Option<serde_json::Value>,
    #[serde(default, alias = "created_at")]
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Actor {
    #[serde(default)]
    login: Option<String>,
    #[serde(default, alias = "display_login")]
    display_login: Option<String>,
    #[serde(default)]
    _avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Repo {
    #[serde(default)]
    _name: Option<String>,
    #[serde(default)]
    full_name: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

fn build_body(event: &Event) -> String {
    let mut parts = Vec::new();

    if let Some(event_type) = &event.event_type {
        parts.push(format!("Event type: {event_type}."));
    }

    if let Some(actor) = &event.actor {
        let login = actor.login.as_deref().or(actor.display_login.as_deref());
        if let Some(login) = login {
            parts.push(format!("Actor: {login}."));
        }
    }

    if let Some(repo) = &event.repo {
        if let Some(full_name) = &repo.full_name {
            parts.push(format!("Repository: {full_name}."));
        }
    }

    if let Some(payload) = &event.payload {
        if let Some(action) = payload.get("action").and_then(|v| v.as_str()) {
            parts.push(format!("Action: {action}."));
        }
        if let Some(size) = payload.get("size").and_then(|v| v.as_u64()) {
            parts.push(format!("Commits: {size}."));
        }
        if let Some(body) = payload.get("body").and_then(|v| v.as_str()) {
            parts.push(format!("Body: {body}."));
        }
        if let Some(title) = payload.get("title").and_then(|v| v.as_str()) {
            parts.push(format!("Title: {title}."));
        }
        if let Some(count) = payload.get("count").and_then(|v| v.as_u64()) {
            parts.push(format!("Count: {count}."));
        }
    }

    if parts.is_empty() {
        return format!(
            "GitHub event {}",
            event.id.as_deref().unwrap_or("unknown")
        );
    }
    parts.join(" ")
}

fn resolve_external_id(event: &Event) -> String {
    event.id.clone().unwrap_or_else(|| "unknown".to_string())
}

fn resolve_url(event: &Event) -> Option<String> {
    event
        .repo
        .as_ref()
        .and_then(|repo| repo.url.clone())
        .or_else(|| {
            event
                .repo
                .as_ref()
                .and_then(|repo| repo.full_name.as_ref())
                .map(|name| format!("https://github.com/{name}"))
        })
}

fn resolve_title(event: &Event) -> Option<String> {
    let parts: Vec<String> = Vec::new();
    let mut title_parts = parts;

    if let Some(actor) = &event.actor {
        if let Some(login) = actor.login.as_deref().or(actor.display_login.as_deref()) {
            title_parts.push(login.to_string());
        }
    }

    if let Some(repo) = &event.repo {
        if let Some(full_name) = &repo.full_name {
            title_parts.push(full_name.clone());
        }
    }

    if let Some(event_type) = &event.event_type {
        title_parts.push(event_type.replace('_', " "));
    }

    if title_parts.is_empty() {
        return None;
    }
    Some(title_parts.join(" "))
}

fn resolve_published_at(event: &Event) -> Option<DateTime<Utc>> {
    event
        .created_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

#[async_trait]
impl SourceAdapter for GitHubSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "github"
    }

    fn validate(&self) -> Result<()> {
        if self.endpoint.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "GitHub endpoint is required".to_string(),
            ));
        }
        Ok(())
    }

    async fn poll(&self, cursor: Option<String>) -> Result<PollResult> {
        self.validate()?;
        let started = Instant::now();
        let payload = self.fetch_payload().await?;
        let events: Vec<Event> = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse GitHub events JSON: {error}"))
        })?;

        let cursor_ts = cursor
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let mut documents = Vec::new();
        let mut latest: Option<DateTime<Utc>> = None;

        for event in events {
            let published_at = resolve_published_at(&event);
            if let (Some(cursor), Some(published_at)) = (cursor_ts, published_at) {
                if published_at <= cursor {
                    continue;
                }
            }

            let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
            metadata.insert("event_type".to_string(), json!(event.event_type));
            metadata.insert(
                "actor_login".to_string(),
                json!(event
                    .actor
                    .as_ref()
                    .and_then(|a| a.login.as_deref().or(a.display_login.as_deref()))),
            );
            metadata.insert(
                "repo_name".to_string(),
                json!(event
                    .repo
                    .as_ref()
                    .and_then(|r| r.full_name.as_deref())),
            );
            metadata.insert("payload".to_string(), json!(event.payload));

            let input = DocumentInput {
                source_id: self.name.clone(),
                source_type: "github".to_string(),
                external_id: resolve_external_id(&event),
                url: resolve_url(&event),
                title: resolve_title(&event),
                body: build_body(&event),
                body_format: BodyFormat::PlainText,
                author: event
                    .actor
                    .as_ref()
                    .and_then(|a| a.login.clone().or(a.display_login.clone())),
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
        let events: Vec<Event> = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse GitHub events JSON: {error}"))
        })?;

        let event = events
            .into_iter()
            .find(|e| resolve_external_id(e) == external_id)
            .ok_or_else(|| {
                ObjectiveError::Source(format!("GitHub event not found: {external_id}"))
            })?;

        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
        metadata.insert("event_type".to_string(), json!(event.event_type));
        metadata.insert(
            "actor_login".to_string(),
            json!(event
                .actor
                .as_ref()
                .and_then(|a| a.login.as_deref().or(a.display_login.as_deref()))),
        );
        metadata.insert(
            "repo_name".to_string(),
            json!(event
                .repo
                .as_ref()
                .and_then(|r| r.full_name.as_deref())),
        );
        metadata.insert("payload".to_string(), json!(event.payload));

        self.normalizer.normalize(DocumentInput {
            source_id: self.name.clone(),
            source_type: "github".to_string(),
            external_id: resolve_external_id(&event),
            url: resolve_url(&event),
            title: resolve_title(&event),
            body: build_body(&event),
            body_format: BodyFormat::PlainText,
            author: event
                .actor
                .as_ref()
                .and_then(|a| a.login.clone().or(a.display_login.clone())),
            published_at: resolve_published_at(&event),
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
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
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
                ObjectiveError::Source(format!("GitHub health check failed: {error}"))
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

    const FIXTURE: &str = r#"[
        {
            "id": "1",
            "type": "PushEvent",
            "actor": {
                "login": "octocat",
                "display_login": "octocat",
                "avatar_url": "https://example.com/avatar.png"
            },
            "repo": {
                "name": "octocat/hello-world",
                "full_name": "octocat/hello-world",
                "url": "https://api.github.com/repos/octocat/hello-world"
            },
            "payload": {
                "size": 3,
                "commits": [
                    {"message": "Add new feature"}
                ]
            },
            "created_at": "2026-06-02T12:00:00Z"
        },
        {
            "id": "2",
            "type": "IssuesEvent",
            "actor": {
                "login": "alice"
            },
            "repo": {
                "name": "alice/project",
                "full_name": "alice/project",
                "url": "https://api.github.com/repos/alice/project"
            },
            "payload": {
                "action": "opened",
                "title": "Fix memory leak in parser",
                "body": "The parser is leaking memory when processing large inputs."
            },
            "created_at": "2026-06-02T11:00:00Z"
        }
    ]"#;

    #[tokio::test]
    async fn test_poll_parses_github_events_into_documents() {
        let adapter = GitHubSourceAdapter::from_json("gh_fixture", FIXTURE);

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 2);

        let first = &result.documents[0];
        assert_eq!(first.source_type, "github");
        assert_eq!(first.external_id, "1");
        assert!(first.body.contains("PushEvent"));
        assert!(first.body.contains("octocat"));
        assert_eq!(first.metadata["event_type"], json!("PushEvent"));
        assert_eq!(first.metadata["actor_login"], json!("octocat"));

        let second = &result.documents[1];
        assert_eq!(second.external_id, "2");
        assert!(second.body.contains("IssuesEvent"));
        assert!(second.body.contains("alice"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter = GitHubSourceAdapter::from_json("gh_fixture", FIXTURE);

        let result = adapter
            .poll(Some("2026-06-02T12:00:30Z".to_string()))
            .await
            .unwrap();
        assert!(result.documents.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_one_returns_matching_document() {
        let adapter = GitHubSourceAdapter::from_json("gh_fixture", FIXTURE);

        let document = adapter.fetch_one("2").await.unwrap();
        assert_eq!(document.external_id, "2");
        assert!(document.body.contains("IssuesEvent"));
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter = GitHubSourceAdapter::from_json("gh_fixture", FIXTURE);
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }
}
