//! Plugin manifest and discovery types.
//!
//! Manifests are JSON files named `plugin.json` placed in
//! `.objective/plugins/<name>/`. They describe plugin metadata, the
//! subscription filter that determines which bus events reach the
//! plugin, and a small declarative configuration block.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The extension point a plugin implements.
///
/// The variants mirror the `plugin_type` strings defined in
/// `docs/api/plugin-api.md` so manifests stay compatible with the
/// gRPC contract when external plugins arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Source,
    Processor,
    Broadcast,
    Notification,
    Embedding,
    Filter,
}

impl PluginType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginType::Source => "source",
            PluginType::Processor => "processor",
            PluginType::Broadcast => "broadcast",
            PluginType::Notification => "notification",
            PluginType::Embedding => "embedding",
            PluginType::Filter => "filter",
        }
    }
}

/// Subscription filter for routing bus events to a plugin.
///
/// A plugin receives an event when *all* of the following match:
/// - The event type is listed in `event_types` (empty means "any").
/// - If `entity_types` is non-empty, the event's `data.entity_type`
///   must be one of the listed values.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginSubscription {
    #[serde(default)]
    pub event_types: Vec<String>,
    #[serde(default)]
    pub entity_types: Vec<String>,
}

/// Manifest for a discoverable plugin (matches `docs/api/plugin-api.md`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub plugin_type: PluginType,
    pub api_version: u32,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub subscriptions: PluginSubscription,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
    #[serde(default = "default_restart_backoff")]
    pub restart_backoff_secs: u64,
    /// Free-form plugin-specific configuration passed to the plugin
    /// on every event.
    #[serde(default)]
    pub config: serde_json::Value,
}

fn default_true() -> bool {
    true
}
fn default_max_restarts() -> u32 {
    5
}
fn default_restart_backoff() -> u64 {
    5
}

/// Discovery errors returned by [`discover`].
#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("plugin directory does not exist: {0}")]
    MissingDirectory(PathBuf),
    #[error("io error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid manifest at {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("manifest validation failed at {path}: {message}")]
    Invalid { path: PathBuf, message: String },
}

/// Discover all valid plugin manifests under `directory`.
///
/// Each immediate subdirectory is treated as a plugin; the loader
/// looks for `plugin.json` inside it. A manifest is valid when:
/// - it parses as JSON,
/// - `name` matches the directory name,
/// - `api_version` is `1`,
/// - `version` is non-empty.
pub fn discover(directory: &Path) -> Result<Vec<PluginManifest>, DiscoveryError> {
    if !directory.exists() {
        return Err(DiscoveryError::MissingDirectory(directory.to_path_buf()));
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(directory).map_err(|e| DiscoveryError::Io {
        path: directory.to_path_buf(),
        source: e,
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("plugin.json");
        if !manifest_path.exists() {
            continue;
        }
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let raw = std::fs::read_to_string(&manifest_path).map_err(|e| DiscoveryError::Io {
            path: manifest_path.clone(),
            source: e,
        })?;
        let manifest: PluginManifest =
            serde_json::from_str(&raw).map_err(|e| DiscoveryError::Parse {
                path: manifest_path.clone(),
                source: e,
            })?;
        if manifest.name != dir_name {
            return Err(DiscoveryError::Invalid {
                path: manifest_path,
                message: format!(
                    "manifest name '{}' does not match directory '{}'",
                    manifest.name, dir_name
                ),
            });
        }
        if manifest.api_version != 1 {
            return Err(DiscoveryError::Invalid {
                path: manifest_path,
                message: format!(
                    "unsupported api_version {} (only 1 is supported)",
                    manifest.api_version
                ),
            });
        }
        if manifest.version.trim().is_empty() {
            return Err(DiscoveryError::Invalid {
                path: manifest_path,
                message: "version must be non-empty".to_string(),
            });
        }
        out.push(manifest);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Summary map of plugin counts by type. Used by the API and the
/// dashboard.
#[derive(Debug, Default, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PluginCounts {
    pub by_type: BTreeMap<String, u32>,
    pub total: u32,
}
