//! Plugin trait and lifecycle types.
//!
//! Plugins are objects that receive bus events they have subscribed
//! to and may emit new events back onto the bus. The host manages
//! their lifecycle and restart behaviour; plugins do not have to be
//! aware of process management.

use std::time::Duration;

use async_trait::async_trait;
use objective_core::{types::EventEnvelope, Result};
use serde::{Deserialize, Serialize};

use crate::manifest::{PluginManifest, PluginType};

/// Plugin lifecycle state.
///
/// Mirrors the diagram in `docs/api/plugin-api.md`. In-process
/// plugins skip the "Started" and "Ready" subprocess states and
/// transition directly into `Running`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    Discovered,
    Validated,
    Started,
    Ready,
    Running,
    Crashed,
    Stopped,
    Error,
}

impl PluginState {
    pub fn is_healthy(&self) -> bool {
        matches!(self, PluginState::Running | PluginState::Ready)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PluginState::Discovered => "discovered",
            PluginState::Validated => "validated",
            PluginState::Started => "started",
            PluginState::Ready => "ready",
            PluginState::Running => "running",
            PluginState::Crashed => "crashed",
            PluginState::Stopped => "stopped",
            PluginState::Error => "error",
        }
    }
}

/// Health snapshot returned by [`Plugin::check_health`].
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PluginHealth {
    pub healthy: bool,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub events_handled: u64,
}

/// Output a plugin may produce in response to an event.
///
/// `Publish` re-enters the bus; `Log` is surfaced through tracing;
/// `State` is persisted to the plugin's persistent state file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginOutput {
    Publish {
        subject: String,
        event: EventEnvelope,
    },
    Log {
        level: String,
        message: String,
    },
    State {
        key: String,
        value: serde_json::Value,
    },
}

/// The plugin contract. Built-in plugins implement this directly;
/// the host will route any bus event whose `event_type` matches the
/// plugin's subscription.
#[async_trait]
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;

    /// Called once when the plugin transitions into `Running`.
    /// The host ignores errors here; they only feed the health
    /// check.
    async fn on_start(&self) -> Result<()> {
        Ok(())
    }

    /// Called when the host wants to take the plugin offline.
    async fn on_stop(&self) -> Result<()> {
        Ok(())
    }

    /// Periodic health probe. Defaults to reporting healthy.
    async fn check_health(&self) -> PluginHealth {
        PluginHealth {
            healthy: true,
            message: "ok".to_string(),
            last_event_at: None,
            events_handled: 0,
        }
    }

    /// Handle a single event. Returning `Ok(vec![])` is the
    /// common case. Returning `Err` is treated as a transient
    /// failure and counted toward the crash threshold.
    async fn handle(&self, event: &EventEnvelope) -> Result<Vec<PluginOutput>>;
}

/// In-process plugin: a typed concrete `Plugin` plus a name. Used
/// to register built-in plugins at startup.
pub struct BuiltinPlugin {
    pub manifest: PluginManifest,
    pub inner: std::sync::Arc<dyn Plugin>,
}

impl BuiltinPlugin {
    pub fn new(manifest: PluginManifest, inner: std::sync::Arc<dyn Plugin>) -> Self {
        Self { manifest, inner }
    }
}

/// Helper: build a `PluginManifest` for a built-in plugin with a
/// minimal-but-valid manifest block.
pub fn builtin_manifest(
    name: &str,
    plugin_type: PluginType,
    version: &str,
    event_types: Vec<String>,
) -> PluginManifest {
    PluginManifest {
        name: name.to_string(),
        version: version.to_string(),
        description: format!("Built-in {plugin_type:?} plugin"),
        author: "objective".to_string(),
        plugin_type,
        api_version: 1,
        capabilities: vec![],
        subscriptions: crate::manifest::PluginSubscription {
            event_types,
            entity_types: vec![],
        },
        enabled: true,
        max_restarts: 5,
        restart_backoff_secs: 5,
        config: serde_json::Value::Null,
    }
}

/// Public config the host uses to run its background loops.
#[derive(Debug, Clone)]
pub struct HostConfig {
    pub health_check_interval: Duration,
    pub route_poll_interval: Duration,
    pub event_timeout: Duration,
    pub max_event_backlog: usize,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(15),
            route_poll_interval: Duration::from_millis(500),
            event_timeout: Duration::from_secs(10),
            max_event_backlog: 1024,
        }
    }
}

/// No-op plugin used as the v1 default for discovered manifests.
///
/// Real external plugins (gRPC, executable) will replace this; the
/// in-process `NoopPlugin` exists so a freshly discovered manifest
/// still shows up in the API and counts toward the totals.
pub struct NoopPlugin {
    pub manifest: PluginManifest,
}

#[async_trait]
impl Plugin for NoopPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn handle(&self, _event: &EventEnvelope) -> Result<Vec<PluginOutput>> {
        Ok(vec![])
    }
}
