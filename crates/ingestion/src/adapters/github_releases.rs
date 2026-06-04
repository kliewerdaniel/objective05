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

/// Source adapter for GitHub Releases API.
///
/// Fetches releases from a GitHub repository using the public releases API.
/// Each release is converted to a normalized [`RawDocument`] preserving the
/// tag, author, and release notes metadata.
#[derive(Debug, Clone)]
pub struct GitHubReleasesAdapter {
    name: String,
    owner: String,
    repo: String,
    endpoint: String,
    normalizer: DocumentNormalizer,
    client: reqwest::Client,
    fixture: Option<String>,
}

impl GitHubReleasesAdapter {
    /// Create a new adapter for a GitHub repository's releases.
    pub fn new(name: impl Into<String>, owner: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            owner: owner.into(),
            repo: repo.into(),
            endpoint: "https://api.github.com".to_string(),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: None,
        }
    }

    /// Build an adapter that returns a pre-canned JSON payload (for tests).
    pub fn from_json(
        name: impl Into<String>,
        owner: &str,
        repo: &str,
        json_body: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            endpoint: "https://api.github.com".to_string(),
            normalizer: DocumentNormalizer,
            client: reqwest::Client::new(),
            fixture: Some(json_body.into()),
        }
    }

    fn build_url(&self) -> String {
        format!(
            "{}/repos/{}/{}/releases",
            self.endpoint.trim_end_matches('/'),
            self.owner,
            self.repo
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
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .send()
            .await
            .map_err(|error| {
                ObjectiveError::Source(format!("failed to fetch GitHub releases: {error}"))
            })?;

        if !response.status().is_success() {
            return Err(ObjectiveError::Source(format!(
                "GitHub API returned HTTP {}",
                response.status()
            )));
        }

        response.text().await.map_err(|error| {
            ObjectiveError::Source(format!("failed to read GitHub releases body: {error}"))
        })
    }
}

#[derive(Debug, Deserialize)]
struct Release {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    tag_name: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: Option<bool>,
    #[serde(default)]
    prerelease: Option<bool>,
    #[serde(default)]
    author: Option<Author>,
    #[serde(default, alias = "created_at")]
    created_at: Option<String>,
    #[serde(default, alias = "published_at")]
    published_at: Option<String>,
    #[serde(default, alias = "html_url")]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Author {
    #[serde(default)]
    login: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Asset {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "browser_download_url")]
    browser_download_url: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

fn build_body(release: &Release) -> String {
    let mut parts = Vec::new();

    if let Some(tag) = &release.tag_name {
        parts.push(format!("Release: {tag}."));
    }

    if let Some(name) = &release.name {
        if name != release.tag_name.as_deref().unwrap_or("") {
            parts.push(format!("Name: {name}."));
        }
    }

    if let Some(author) = &release.author {
        if let Some(login) = &author.login {
            parts.push(format!("Author: {login}."));
        }
    }

    if let Some(draft) = release.draft {
        if draft {
            parts.push("Status: draft.".to_string());
        }
    }

    if let Some(prerelease) = release.prerelease {
        if prerelease {
            parts.push("Status: pre-release.".to_string());
        }
    }

    if let Some(body) = &release.body {
        if !body.trim().is_empty() {
            parts.push(format!("Notes: {}", body.trim()));
        }
    }

    if !release.assets.is_empty() {
        let asset_names: Vec<&str> = release
            .assets
            .iter()
            .filter_map(|a| a.name.as_deref())
            .collect();
        if !asset_names.is_empty() {
            parts.push(format!("Assets: {}.", asset_names.join(", ")));
        }
    }

    if parts.is_empty() {
        return format!(
            "GitHub release {}",
            release
                .id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
    }
    parts.join(" ")
}

fn resolve_external_id(release: &Release) -> String {
    release
        .tag_name
        .clone()
        .or_else(|| release.id.map(|id| id.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn resolve_published_at(release: &Release) -> Option<DateTime<Utc>> {
    release
        .published_at
        .as_deref()
        .or(release.created_at.as_deref())
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

#[async_trait]
impl SourceAdapter for GitHubReleasesAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "github_releases"
    }

    fn validate(&self) -> Result<()> {
        if self.owner.trim().is_empty() || self.repo.trim().is_empty() {
            return Err(ObjectiveError::Validation(
                "GitHub owner and repo are required".to_string(),
            ));
        }
        Ok(())
    }

    async fn poll(&self, cursor: Option<String>) -> Result<PollResult> {
        self.validate()?;
        let started = Instant::now();
        let payload = self.fetch_payload().await?;
        let releases: Vec<Release> = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse GitHub releases JSON: {error}"))
        })?;

        let cursor_ts = cursor
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|dt| dt.with_timezone(&Utc));

        let mut documents = Vec::new();
        let mut latest: Option<DateTime<Utc>> = None;

        for release in releases {
            let published_at = resolve_published_at(&release);
            if let (Some(cursor), Some(published_at)) = (cursor_ts, published_at) {
                if published_at <= cursor {
                    continue;
                }
            }

            let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
            metadata.insert("owner".to_string(), json!(self.owner));
            metadata.insert("repo".to_string(), json!(self.repo));
            metadata.insert("tag_name".to_string(), json!(release.tag_name));
            metadata.insert(
                "author".to_string(),
                json!(release.author.as_ref().and_then(|a| a.login.as_deref())),
            );
            metadata.insert("draft".to_string(), json!(release.draft));
            metadata.insert("prerelease".to_string(), json!(release.prerelease));
            metadata.insert("asset_count".to_string(), json!(release.assets.len()));

            let input = DocumentInput {
                source_id: self.name.clone(),
                source_type: "github_releases".to_string(),
                external_id: resolve_external_id(&release),
                url: release.html_url.clone(),
                title: release.name.clone().or_else(|| release.tag_name.clone()),
                body: build_body(&release),
                body_format: BodyFormat::PlainText,
                author: release.author.as_ref().and_then(|a| a.login.clone()),
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
        let releases: Vec<Release> = serde_json::from_str(&payload).map_err(|error| {
            ObjectiveError::Source(format!("failed to parse GitHub releases JSON: {error}"))
        })?;

        let release = releases
            .into_iter()
            .find(|r| resolve_external_id(r) == external_id)
            .ok_or_else(|| {
                ObjectiveError::Source(format!("GitHub release not found: {external_id}"))
            })?;

        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
        metadata.insert("owner".to_string(), json!(self.owner));
        metadata.insert("repo".to_string(), json!(self.repo));
        metadata.insert("tag_name".to_string(), json!(release.tag_name));
        metadata.insert(
            "author".to_string(),
            json!(release.author.as_ref().and_then(|a| a.login.as_deref())),
        );

        self.normalizer.normalize(DocumentInput {
            source_id: self.name.clone(),
            source_type: "github_releases".to_string(),
            external_id: resolve_external_id(&release),
            url: release.html_url.clone(),
            title: release.name.clone().or_else(|| release.tag_name.clone()),
            body: build_body(&release),
            body_format: BodyFormat::PlainText,
            author: release.author.as_ref().and_then(|a| a.login.clone()),
            published_at: resolve_published_at(&release),
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
            .get(format!(
                "{}/repos/{}/{}/releases?per_page=1",
                self.endpoint.trim_end_matches('/'),
                self.owner,
                self.repo
            ))
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
                ObjectiveError::Source(format!("GitHub releases health check failed: {error}"))
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

    const RELEASES_FIXTURE: &str = r#"[
        {
            "id": 1,
            "tag_name": "v1.2.0",
            "name": "Release 1.2.0",
            "body": "Changes: Added new feature, Fixed bug in parser",
            "draft": false,
            "prerelease": false,
            "author": {"login": "octocat"},
            "created_at": "2026-06-02T12:00:00Z",
            "published_at": "2026-06-02T12:00:00Z",
            "html_url": "https://github.com/octocat/hello-world/releases/tag/v1.2.0",
            "assets": [
                {"name": "hello-world-linux-amd64", "browser_download_url": "https://example.com/download", "size": 1024000},
                {"name": "hello-world-darwin-arm64", "browser_download_url": "https://example.com/download2", "size": 1048576}
            ]
        },
        {
            "id": 2,
            "tag_name": "v1.1.0",
            "name": "Release 1.1.0",
            "body": "Initial stable release",
            "draft": false,
            "prerelease": false,
            "author": {"login": "alice"},
            "created_at": "2026-06-01T10:00:00Z",
            "published_at": "2026-06-01T10:00:00Z",
            "html_url": "https://github.com/octocat/hello-world/releases/tag/v1.1.0",
            "assets": []
        }
    ]"#;

    #[tokio::test]
    async fn test_poll_parses_releases_into_documents() {
        let adapter = GitHubReleasesAdapter::from_json(
            "releases_fixture",
            "octocat",
            "hello-world",
            RELEASES_FIXTURE,
        );

        let result = adapter.poll(None).await.unwrap();
        assert_eq!(result.documents.len(), 2);

        let first = &result.documents[0];
        assert_eq!(first.source_type, "github_releases");
        assert_eq!(first.external_id, "v1.2.0");
        assert!(first.body.contains("v1.2.0"));
        assert!(first.body.contains("Added new feature"));
        assert!(first.body.contains("hello-world-linux-amd64"));
        assert_eq!(first.metadata["owner"], json!("octocat"));
        assert_eq!(first.metadata["repo"], json!("hello-world"));

        let second = &result.documents[1];
        assert_eq!(second.external_id, "v1.1.0");
        assert!(second.body.contains("Initial stable release"));
    }

    #[tokio::test]
    async fn test_poll_honors_rfc3339_cursor() {
        let adapter = GitHubReleasesAdapter::from_json(
            "releases_fixture",
            "octocat",
            "hello-world",
            RELEASES_FIXTURE,
        );

        let result = adapter
            .poll(Some("2026-06-01T12:00:00Z".to_string()))
            .await
            .unwrap();
        assert_eq!(result.documents.len(), 1);
        assert_eq!(result.documents[0].external_id, "v1.2.0");
    }

    #[tokio::test]
    async fn test_fetch_one_returns_matching_document() {
        let adapter = GitHubReleasesAdapter::from_json(
            "releases_fixture",
            "octocat",
            "hello-world",
            RELEASES_FIXTURE,
        );

        let document = adapter.fetch_one("v1.1.0").await.unwrap();
        assert_eq!(document.external_id, "v1.1.0");
        assert!(document.body.contains("Initial stable release"));
    }

    #[tokio::test]
    async fn test_health_reports_healthy_for_fixture() {
        let adapter = GitHubReleasesAdapter::from_json(
            "releases_fixture",
            "octocat",
            "hello-world",
            RELEASES_FIXTURE,
        );
        let health = adapter.health().await.unwrap();
        assert_eq!(health.status, "healthy");
    }
}
