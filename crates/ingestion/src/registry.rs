//! Ingestion source registry.
//!
//! The first vertical slice wired a fixed set of default sources
//! (Hacker News, Lobsters) directly into the pipeline worker. The
//! documented MVP exposes a configuration endpoint so operators
//! can add, update, and remove sources at runtime. This module
//! holds the persisted registry that backs those endpoints.
//!
//! The registry itself is a thin, well-typed shell:
//!
//! - [`SourceDefinition`] is the on-the-wire shape: a unique name,
//!   the adapter type, the target URL, a cron schedule, and an
//!   `enabled` toggle.
//! - [`SourceRegistry`] is the in-memory store, persisted to a
//!   JSON file under the data root. The store is guarded by a
//!   `tokio::sync::RwLock` because the API gateway reads and
//!   writes it concurrently with the daemon's pipeline worker.
//! - [`SourceRegistry::spawn_adapter`] materialises a registered
//!   source into the actual [`SourceAdapter`] trait object the
//!   pipeline worker polls.
//!
//! The registry deliberately stays small and local: it is a
//! replacement for "edit `app.rs` and restart", not a full
//! workflow engine.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{info, warn};
use utoipa::ToSchema;

use crate::adapters::{
    ArxivSourceAdapter, GitHubReleasesAdapter, GitHubSourceAdapter, HackerNewsSourceAdapter,
    PodcastSourceAdapter, RedditSourceAdapter, RssSourceAdapter, SecEdgarAdapter,
    StaticSourceAdapter, WebSourceAdapter, YouTubeSourceAdapter,
};
use objective_core::traits::SourceAdapter;

/// Errors that can occur when mutating the registry.
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("source not found: {0}")]
    NotFound(String),
    #[error("source already exists: {0}")]
    AlreadyExists(String),
    #[error("unsupported source type: {0}")]
    UnsupportedType(String),
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<RegistryError> for objective_core::ObjectiveError {
    fn from(err: RegistryError) -> Self {
        objective_core::ObjectiveError::Source(err.to_string())
    }
}

/// A registered ingestion source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct SourceDefinition {
    /// Unique name (slug) for this source. Maps to the
    /// `source_id` field on documents produced by the adapter.
    pub name: String,
    /// Adapter type. One of the supported values in
    /// [`SourceType`].
    pub source_type: SourceType,
    /// Target URL or handle. Required for most adapters; ignored
    /// for the always-online adapters (`hackernews`).
    #[serde(default)]
    pub url: Option<String>,
    /// Optional cron expression. If `None`, the source is polled
    /// only on demand (e.g. via the trigger endpoint or the
    /// default scheduler job).
    #[serde(default)]
    pub schedule: Option<String>,
    /// Whether the source is enabled. Disabled sources are kept
    /// on disk but skipped by the pipeline.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// When the source was first registered.
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    /// When the source was last updated.
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

/// Adapter type discriminator. Mirrors the `SourceType` union in
/// `docs/api/internal-api.md` so the dashboard's "Add Source"
/// form maps cleanly onto the API.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Rss,
    Reddit,
    Youtube,
    Hackernews,
    Arxiv,
    Web,
    Podcast,
    Github,
    GithubReleases,
    SecEdgar,
    Static,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceType::Rss => "rss",
            SourceType::Reddit => "reddit",
            SourceType::Youtube => "youtube",
            SourceType::Hackernews => "hackernews",
            SourceType::Arxiv => "arxiv",
            SourceType::Web => "web",
            SourceType::Podcast => "podcast",
            SourceType::Github => "github",
            SourceType::GithubReleases => "github_releases",
            SourceType::SecEdgar => "sec_edgar",
            SourceType::Static => "static",
        }
    }
}

/// In-memory, persisted source registry. Cheap to clone (the
/// store is behind an `Arc<RwLock>`); pass it to the API gateway
/// and the pipeline worker.
#[derive(Clone)]
pub struct SourceRegistry {
    state: Arc<RwLock<RegistryState>>,
    persist_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct RegistryState {
    sources: BTreeMap<String, SourceDefinition>,
}

impl SourceRegistry {
    /// Load (or initialise) the registry from the given directory.
    /// The on-disk file is `<dir>/sources.json`. Missing files are
    /// not an error: the registry starts empty.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("sources.json");
        let state = match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str::<RegistryState>(&content).unwrap_or_else(|e| {
                warn!("failed to parse sources registry: {e}; starting empty");
                RegistryState::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!(
                    "no sources registry file at {}; starting empty",
                    path.display()
                );
                RegistryState::default()
            }
            Err(e) => {
                warn!("failed to read sources registry file: {e}; starting empty");
                RegistryState::default()
            }
        };
        Self {
            state: Arc::new(RwLock::new(state)),
            persist_path: Some(path),
        }
    }

    /// Build a registry that does not persist. Useful for tests.
    pub fn ephemeral() -> Self {
        Self {
            state: Arc::new(RwLock::new(RegistryState::default())),
            persist_path: None,
        }
    }

    /// Persist the registry to disk. Failures are logged but not
    /// propagated, because losing a save to disk should not crash
    /// an in-flight ingestion run.
    async fn persist(&self) {
        let Some(path) = self.persist_path.clone() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let snapshot = {
            let guard = self.state.read().await;
            guard.clone()
        };
        match serde_json::to_string_pretty(&snapshot) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    warn!(error = %e, path = %path.display(), "failed to persist source registry");
                }
            }
            Err(e) => warn!(error = %e, "failed to serialize source registry"),
        }
    }

    /// Add a new source. Returns the stored definition (with
    /// `created_at`/`updated_at` filled in) or an error if the
    /// name is already taken.
    pub async fn add(
        &self,
        mut source: SourceDefinition,
    ) -> Result<SourceDefinition, RegistryError> {
        source.name = source.name.trim().to_string();
        if source.name.is_empty() {
            return Err(RegistryError::Invalid("name must not be empty".to_string()));
        }
        let now = Utc::now();
        source.created_at = now;
        source.updated_at = now;

        {
            let mut state = self.state.write().await;
            if state.sources.contains_key(&source.name) {
                return Err(RegistryError::AlreadyExists(source.name));
            }
            state.sources.insert(source.name.clone(), source.clone());
        }
        self.persist().await;
        info!(name = %source.name, source_type = source.source_type.as_str(), "source registered");
        Ok(source)
    }

    /// Update an existing source. `None` fields are left
    /// untouched. The `updated_at` timestamp is refreshed.
    pub async fn update(
        &self,
        name: &str,
        patch: SourcePatch,
    ) -> Result<SourceDefinition, RegistryError> {
        let mut state = self.state.write().await;
        let entry = state
            .sources
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotFound(name.to_string()))?;
        if let Some(url) = patch.url {
            entry.url = Some(url);
        }
        if let Some(schedule) = patch.schedule {
            entry.schedule = Some(schedule);
        }
        if let Some(enabled) = patch.enabled {
            entry.enabled = enabled;
        }
        entry.updated_at = Utc::now();
        let updated = entry.clone();
        drop(state);
        self.persist().await;
        info!(name = %updated.name, "source updated");
        Ok(updated)
    }

    /// Remove a source by name. Returns the removed definition
    /// so the API can echo the old record back to the caller.
    pub async fn remove(&self, name: &str) -> Result<SourceDefinition, RegistryError> {
        let removed = {
            let mut state = self.state.write().await;
            state
                .sources
                .remove(name)
                .ok_or_else(|| RegistryError::NotFound(name.to_string()))?
        };
        self.persist().await;
        info!(name = %removed.name, "source removed");
        Ok(removed)
    }

    /// Return all registered sources, sorted by name.
    pub async fn list(&self) -> Vec<SourceDefinition> {
        let guard = self.state.read().await;
        guard.sources.values().cloned().collect()
    }

    /// Return the source with the given name, if any.
    pub async fn get(&self, name: &str) -> Option<SourceDefinition> {
        self.state.read().await.sources.get(name).cloned()
    }

    /// Return all enabled sources. Used by the pipeline worker to
    /// build its poll set on each cycle.
    pub async fn enabled(&self) -> Vec<SourceDefinition> {
        let guard = self.state.read().await;
        guard
            .sources
            .values()
            .filter(|source| source.enabled)
            .cloned()
            .collect()
    }

    /// Materialise a registered source into the actual
    /// [`SourceAdapter`] trait object. Returns
    /// `RegistryError::UnsupportedType` for unknown types and
    /// `RegistryError::Invalid` for missing required fields.
    pub fn spawn_adapter(
        source: &SourceDefinition,
    ) -> Result<Box<dyn SourceAdapter>, RegistryError> {
        let name = source.name.clone();
        let url = source.url.clone().unwrap_or_default();
        match source.source_type {
            SourceType::Rss => Ok(Box::new(RssSourceAdapter::new(name, url))),
            SourceType::Reddit => Ok(Box::new(RedditSourceAdapter::new(name, url))),
            SourceType::Youtube => Ok(Box::new(YouTubeSourceAdapter::new(name, url))),
            SourceType::Hackernews => Ok(Box::new(HackerNewsSourceAdapter::new(
                name,
                "https://hn.algolia.com/api/v1",
            ))),
            SourceType::Arxiv => Ok(Box::new(ArxivSourceAdapter::new(name, url))),
            SourceType::Web => Ok(Box::new(WebSourceAdapter::new(name, url))),
            SourceType::Podcast => Ok(Box::new(PodcastSourceAdapter::new(name, url))),
            SourceType::Github => {
                if url.is_empty() {
                    return Err(RegistryError::Invalid(
                        "github source requires a url (e.g. https://api.github.com/orgs/owner/events)"
                            .to_string(),
                    ));
                }
                Ok(Box::new(GitHubSourceAdapter::new(name, url)))
            }
            SourceType::GithubReleases => {
                // The releases adapter is owner/repo-based, so we
                // accept a `https://github.com/owner/repo` URL and
                // strip it down to its components.
                if url.is_empty() {
                    return Err(RegistryError::Invalid(
                        "github_releases source requires a url (e.g. https://github.com/owner/repo)"
                            .to_string(),
                    ));
                }
                let (owner, repo) = parse_github_repo_url(&url).ok_or_else(|| {
                    RegistryError::Invalid(
                        "github_releases url must look like https://github.com/owner/repo"
                            .to_string(),
                    )
                })?;
                Ok(Box::new(GitHubReleasesAdapter::new(name, owner, repo)))
            }
            SourceType::SecEdgar => Ok(Box::new(SecEdgarAdapter::new(name))),
            SourceType::Static => {
                let title = format!("Static source: {name}");
                Ok(Box::new(StaticSourceAdapter::from_plain_text(
                    name, title, url,
                )))
            }
        }
    }
}

/// Partial update payload for [`SourceRegistry::update`]. Only
/// the fields a user is allowed to change are exposed.
#[derive(Debug, Default, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourcePatch {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Parse a `https://github.com/owner/repo` URL into its
/// `(owner, repo)` components. Returns `None` if the URL does
/// not look like a GitHub repo URL.
fn parse_github_repo_url(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim().trim_end_matches('/');
    let tail = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))?;
    let mut parts = tail.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rss(name: &str, url: &str) -> SourceDefinition {
        SourceDefinition {
            name: name.to_string(),
            source_type: SourceType::Rss,
            url: Some(url.to_string()),
            schedule: None,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_add_and_list_round_trip() {
        let registry = SourceRegistry::ephemeral();
        let added = registry
            .add(rss("wired", "https://example.com/rss"))
            .await
            .unwrap();
        assert_eq!(added.name, "wired");
        let list = registry.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].source_type, SourceType::Rss);
    }

    #[tokio::test]
    async fn test_duplicate_add_is_rejected() {
        let registry = SourceRegistry::ephemeral();
        registry
            .add(rss("wired", "https://example.com/rss"))
            .await
            .unwrap();
        let err = registry
            .add(rss("wired", "https://example.com/other"))
            .await
            .unwrap_err();
        assert!(matches!(err, RegistryError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn test_update_preserves_unset_fields() {
        let registry = SourceRegistry::ephemeral();
        registry
            .add(rss("wired", "https://example.com/rss"))
            .await
            .unwrap();
        let updated = registry
            .update(
                "wired",
                SourcePatch {
                    url: Some("https://example.com/new".to_string()),
                    schedule: None,
                    enabled: Some(false),
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.url.as_deref(), Some("https://example.com/new"));
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn test_remove_unknown_source_returns_not_found() {
        let registry = SourceRegistry::ephemeral();
        let err = registry.remove("missing").await.unwrap_err();
        assert!(matches!(err, RegistryError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_enabled_filters_disabled_sources() {
        let registry = SourceRegistry::ephemeral();
        registry
            .add(rss("a", "https://example.com/a"))
            .await
            .unwrap();
        registry
            .add(rss("b", "https://example.com/b"))
            .await
            .unwrap();
        registry
            .update(
                "a",
                SourcePatch {
                    url: None,
                    schedule: None,
                    enabled: Some(false),
                },
            )
            .await
            .unwrap();
        let enabled = registry.enabled().await;
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "b");
    }

    #[tokio::test]
    async fn test_persisted_registry_survives_reload() {
        let dir = tempfile::tempdir().unwrap().keep();
        let registry = SourceRegistry::load(&dir);
        registry
            .add(rss(
                "nyt",
                "https://rss.nytimes.com/services/xml/rss/nyt/HomePage.xml",
            ))
            .await
            .unwrap();
        let reloaded = SourceRegistry::load(&dir);
        let list = reloaded.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "nyt");
    }

    #[test]
    fn test_spawn_adapter_returns_boxed_trait_object() {
        let source = rss("wired", "https://example.com/rss");
        let adapter = SourceRegistry::spawn_adapter(&source).unwrap();
        assert_eq!(adapter.name(), "wired");
        assert_eq!(adapter.source_type(), "rss");
    }

    #[test]
    fn test_spawn_adapter_rejects_invalid_github_url() {
        let source = SourceDefinition {
            name: "gh".to_string(),
            source_type: SourceType::Github,
            url: None,
            schedule: None,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let result = SourceRegistry::spawn_adapter(&source);
        assert!(matches!(result, Err(RegistryError::Invalid(_))));
    }
}
