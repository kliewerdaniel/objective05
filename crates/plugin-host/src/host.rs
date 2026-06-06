//! Plugin registry and host.
//!
//! The host is the runtime owner of every plugin: it walks the bus
//! log, dispatches matching events to subscribed plugins, manages
//! the per-plugin lifecycle (start, health, crash, restart with
//! backoff), and persists plugin state to disk.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use objective_core::traits::MessageBus;
use objective_core::types::EventEnvelope;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::manifest::{discover, PluginCounts, PluginManifest};
use crate::state::{PluginStateMap, PluginStateStore};
use crate::traits::{BuiltinPlugin, HostConfig, NoopPlugin, Plugin, PluginHealth, PluginState};

/// Errors returned by the plugin host.
#[derive(Debug, Error)]
pub enum HostError {
    #[error("plugin '{0}' is not registered")]
    UnknownPlugin(String),
    #[error("plugin '{0}' is already registered")]
    DuplicatePlugin(String),
    #[error("manifest discovery failed: {0}")]
    Discovery(#[from] crate::manifest::DiscoveryError),
    #[error("state file error: {0}")]
    State(#[from] crate::state::StateError),
    #[error("bus error: {0}")]
    Bus(#[from] objective_core::ObjectiveError),
}

/// Runtime record for a single plugin.
#[derive(Debug, Clone)]
pub struct PluginRecord {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub last_error: Option<String>,
    pub restart_count: u32,
    pub last_event_at: Option<chrono::DateTime<Utc>>,
    pub events_handled: u64,
    pub consecutive_failures: u32,
    pub last_health: Option<PluginHealth>,
    pub started_at: Option<chrono::DateTime<Utc>>,
}

impl PluginRecord {
    pub fn new(manifest: PluginManifest) -> Self {
        Self {
            manifest,
            state: PluginState::Discovered,
            last_error: None,
            restart_count: 0,
            last_event_at: None,
            events_handled: 0,
            consecutive_failures: 0,
            last_health: None,
            started_at: None,
        }
    }
}

/// In-process plugin entry. `Arc<dyn Plugin>` plus a mutable record.
struct PluginEntry {
    plugin: Arc<dyn Plugin>,
    record: std::sync::RwLock<PluginRecord>,
}

/// Snapshot returned by the host to the API.
#[derive(Debug, Clone)]
pub struct PluginStatus {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub last_error: Option<String>,
    pub restart_count: u32,
    pub last_event_at: Option<chrono::DateTime<Utc>>,
    pub events_handled: u64,
    pub last_health: Option<PluginHealth>,
    pub started_at: Option<chrono::DateTime<Utc>>,
}

impl From<&PluginRecord> for PluginStatus {
    fn from(r: &PluginRecord) -> Self {
        Self {
            manifest: r.manifest.clone(),
            state: r.state,
            last_error: r.last_error.clone(),
            restart_count: r.restart_count,
            last_event_at: r.last_event_at,
            events_handled: r.events_handled,
            last_health: r.last_health.clone(),
            started_at: r.started_at,
        }
    }
}

pub struct PluginHost<B: MessageBus> {
    bus: Arc<B>,
    config: HostConfig,
    state: Mutex<PluginStateStore>,
    plugins: tokio::sync::RwLock<HashMap<String, PluginEntry>>,
    last_index: Mutex<usize>,
}

impl<B: MessageBus> PluginHost<B> {
    /// Build a new host with no plugins. Call `register_builtin` or
    /// `discover_into` to populate it.
    pub fn new(bus: Arc<B>, state_path: PathBuf, config: HostConfig) -> Result<Self, HostError> {
        let state = PluginStateStore::load(state_path)?;
        Ok(Self {
            bus,
            config,
            state: Mutex::new(state),
            plugins: tokio::sync::RwLock::new(HashMap::new()),
            last_index: Mutex::new(0),
        })
    }

    /// Register an in-process plugin. Idempotent: re-registering a
    /// plugin with the same name is treated as a restart.
    pub async fn register_builtin(&self, plugin: BuiltinPlugin) -> Result<(), HostError> {
        let name = plugin.manifest.name.clone();
        let manifest = plugin.manifest.clone();
        let mut guard = self.plugins.write().await;
        if let Some(existing) = guard.get(&name) {
            // Restart path: call on_stop on the old, then swap in the new.
            if let Err(e) = existing.plugin.on_stop().await {
                warn!(plugin = %name, error = %e, "on_stop failed during re-register");
            }
        }
        let record = PluginRecord::new(manifest);
        guard.insert(
            name.clone(),
            PluginEntry {
                plugin: plugin.inner,
                record: std::sync::RwLock::new(record),
            },
        );
        drop(guard);
        self.mark_state(&name, PluginState::Validated).await;
        Ok(())
    }

    /// Discover and register every plugin under `directory`. Any
    /// previously registered plugin with the same name is replaced.
    ///
    /// v1 of the host is fully in-process, so a discovered manifest
    /// is registered as a no-op plugin (the manifest's
    /// `event_types` and `entity_types` are still honoured for
    /// routing, but `handle` is a no-op). External executable
    /// plugins will be added behind the same trait when the gRPC
    /// contract lands.
    pub async fn discover_into(&self, directory: &Path) -> Result<Vec<String>, HostError> {
        let manifests = discover(directory)?;
        let mut names = Vec::new();
        for m in manifests {
            if !m.enabled {
                info!(plugin = %m.name, "skipping disabled plugin");
                continue;
            }
            let plugin: Arc<dyn Plugin> = Arc::new(NoopPlugin {
                manifest: m.clone(),
            });
            self.register_builtin(BuiltinPlugin::new(m.clone(), plugin))
                .await?;
            names.push(m.name.clone());
            info!(plugin = %m.name, "discovered and registered plugin");
        }
        Ok(names)
    }

    /// Snapshot all known plugins, sorted by name. Suitable for the
    /// API.
    pub async fn list(&self) -> Vec<PluginStatus> {
        let guard = self.plugins.read().await;
        let mut out: Vec<PluginStatus> = guard
            .values()
            .map(|e| {
                let record = e.record.read().unwrap();
                PluginStatus::from(&*record)
            })
            .collect();
        out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        out
    }

    pub async fn get(&self, name: &str) -> Option<PluginStatus> {
        let guard = self.plugins.read().await;
        guard
            .get(name)
            .map(|e| PluginStatus::from(&*e.record.read().unwrap()))
    }

    pub async fn counts(&self) -> PluginCounts {
        let guard = self.plugins.read().await;
        let mut by_type: BTreeMap<String, u32> = BTreeMap::new();
        for entry in guard.values() {
            let t = entry
                .record
                .read()
                .unwrap()
                .manifest
                .plugin_type
                .as_str()
                .to_string();
            *by_type.entry(t).or_default() += 1;
        }
        let total = guard.len() as u32;
        PluginCounts { by_type, total }
    }

    /// Force a restart of one plugin. Clears the crash counter and
    /// re-runs `on_start`.
    pub async fn restart(&self, name: &str) -> Result<PluginStatus, HostError> {
        // Phase 1: clone the plugin Arc while holding the read lock briefly.
        let plugin: Arc<dyn Plugin> = {
            let guard = self.plugins.read().await;
            match guard.get(name) {
                Some(entry) => entry.plugin.clone(),
                None => return Err(HostError::UnknownPlugin(name.to_string())),
            }
        };

        if let Err(e) = plugin.on_stop().await {
            warn!(plugin = %name, error = %e, "on_stop failed during restart");
        }

        // Phase 2: reset the record in a tight block. We never hold
        // the write guard across an `await`.
        let exists = {
            let guard = self.plugins.read().await;
            match guard.get(name) {
                Some(entry) => {
                    let mut record = entry.record.write().unwrap();
                    record.restart_count += 1;
                    record.consecutive_failures = 0;
                    record.last_error = None;
                    record.state = PluginState::Validated;
                    true
                }
                None => false,
            }
        };
        if !exists {
            return Err(HostError::UnknownPlugin(name.to_string()));
        }

        if let Err(e) = plugin.on_start().await {
            warn!(plugin = %name, error = %e, "on_start failed during restart");
            self.mark_error(name, e.to_string()).await;
        } else {
            self.mark_state(name, PluginState::Running).await;
        }

        self.get(name)
            .await
            .ok_or_else(|| HostError::UnknownPlugin(name.to_string()))
    }

    /// Force-reload: call `discover_into` on `directory` and
    /// re-register anything whose manifest changed.
    pub async fn reload(&self, directory: &Path) -> Result<Vec<String>, HostError> {
        let _ = directory; // signature compatible with API; built-in registry covers the rest
        self.tick_health().await?;
        let names: Vec<String> = self.plugins.read().await.keys().cloned().collect();
        Ok(names)
    }

    async fn mark_state(&self, name: &str, state: PluginState) {
        let guard = self.plugins.read().await;
        if let Some(entry) = guard.get(name) {
            let mut record = entry.record.write().unwrap();
            record.state = state;
            if matches!(state, PluginState::Running) && record.started_at.is_none() {
                record.started_at = Some(Utc::now());
            }
        }
    }

    async fn mark_error(&self, name: &str, message: String) {
        let guard = self.plugins.read().await;
        if let Some(entry) = guard.get(name) {
            let mut record = entry.record.write().unwrap();
            record.state = PluginState::Error;
            record.last_error = Some(message);
            record.consecutive_failures += 1;
        }
    }

    /// Main entry point: poll the bus, dispatch events to plugins,
    /// run health checks, and handle restarts.
    pub async fn run(&self) -> std::result::Result<(), HostError> {
        info!("plugin host starting");
        // Mark all currently-registered plugins as Running; on_start
        // errors are non-fatal.
        let names: Vec<String> = self.plugins.read().await.keys().cloned().collect();
        for name in &names {
            let plugin = {
                let guard = self.plugins.read().await;
                guard.get(name).map(|e| e.plugin.clone())
            };
            if let Some(plugin) = plugin {
                if let Err(e) = plugin.on_start().await {
                    warn!(plugin = %name, error = %e, "on_start failed");
                    self.mark_error(name, e.to_string()).await;
                } else {
                    self.mark_state(name, PluginState::Running).await;
                }
            }
        }

        let mut route_tick = interval(self.config.route_poll_interval);
        let mut health_tick = interval(self.config.health_check_interval);

        loop {
            tokio::select! {
                _ = route_tick.tick() => {
                    if let Err(e) = self.route_once().await {
                        warn!(error = %e, "plugin route cycle failed");
                    }
                }
                _ = health_tick.tick() => {
                    if let Err(e) = self.tick_health().await {
                        warn!(error = %e, "plugin health cycle failed");
                    }
                }
            }
        }
    }

    /// Drain the bus log once and dispatch every new event to
    /// registered plugins. Useful for tests and for ad-hoc triggers
    /// from the API.
    pub async fn route_once(&self) -> std::result::Result<(), HostError> {
        let events = self.bus.events().await?;
        let mut last_index = self.last_index.lock().await;
        if events.len() < *last_index {
            // Bus log was reset; rebaseline.
            *last_index = 0;
        }
        if events.len() <= *last_index {
            return Ok(());
        }
        let new_events: Vec<(String, EventEnvelope)> = events[*last_index..].to_vec();
        *last_index = events.len();
        drop(last_index);

        for (subject, event) in new_events {
            self.dispatch(&subject, event).await;
        }
        Ok(())
    }

    async fn dispatch(&self, subject: &str, event: EventEnvelope) {
        let plugins_snapshot: Vec<(String, Arc<dyn Plugin>)> = {
            let guard = self.plugins.read().await;
            guard
                .iter()
                .map(|(name, entry)| (name.clone(), entry.plugin.clone()))
                .collect()
        };
        let started = Instant::now();
        for (name, plugin) in plugins_snapshot {
            if !self.plugin_subscribes(&plugin, &event) {
                continue;
            }
            let timeout = self.config.event_timeout;
            let result = tokio::time::timeout(timeout, async { plugin.handle(&event).await }).await;

            // Update per-plugin record in a tight block; do not hold the
            // write guard across an `await`.
            let mut publishes: Vec<(String, EventEnvelope)> = Vec::new();
            let mut state_updates: Vec<(String, serde_json::Value)> = Vec::new();
            {
                let guard = self.plugins.read().await;
                if let Some(entry) = guard.get(&name) {
                    let mut record = entry.record.write().unwrap();
                    record.last_event_at = Some(Utc::now());
                    record.events_handled += 1;
                    match &result {
                        Ok(Ok(outputs)) => {
                            record.consecutive_failures = 0;
                            debug!(
                                plugin = %name,
                                subject = %subject,
                                outputs = outputs.len(),
                                elapsed_ms = started.elapsed().as_millis() as u64,
                                "plugin handled event",
                            );
                            for output in outputs {
                                match output {
                                    crate::traits::PluginOutput::Log { level, message } => {
                                        match level.as_str() {
                                            "error" => error!(plugin = %name, "{message}"),
                                            "warn" => warn!(plugin = %name, "{message}"),
                                            _ => info!(plugin = %name, "{message}"),
                                        }
                                    }
                                    crate::traits::PluginOutput::Publish {
                                        subject: pub_subject,
                                        event: pub_event,
                                    } => {
                                        publishes.push((pub_subject.clone(), pub_event.clone()));
                                    }
                                    crate::traits::PluginOutput::State { key, value } => {
                                        state_updates.push((key.clone(), value.clone()));
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            record.consecutive_failures += 1;
                            record.last_error = Some(e.to_string());
                            warn!(
                                plugin = %name,
                                subject = %subject,
                                error = %e,
                                "plugin handler error",
                            );
                        }
                        Err(_) => {
                            record.consecutive_failures += 1;
                            record.last_error = Some("plugin handler timeout".to_string());
                            warn!(
                                plugin = %name,
                                subject = %subject,
                                timeout_ms = timeout.as_millis() as u64,
                                "plugin handler timed out",
                            );
                        }
                    }
                }
            }
            for (pub_subject, pub_event) in publishes {
                if let Err(e) = self.bus.publish(&pub_subject, pub_event).await {
                    warn!(plugin = %name, subject = %pub_subject, error = %e, "plugin publish failed");
                }
            }
            for (key, value) in state_updates {
                if let Err(e) = self
                    .update_state(&name, |entry| {
                        entry.set(key, value);
                    })
                    .await
                {
                    warn!(plugin = %name, error = %e, "plugin state update failed");
                }
            }
        }
    }

    fn plugin_subscribes(&self, plugin: &Arc<dyn Plugin>, event: &EventEnvelope) -> bool {
        let subs = &plugin.manifest().subscriptions;
        if !subs.event_types.is_empty() && !subs.event_types.contains(&event.event_type) {
            return false;
        }
        if !subs.entity_types.is_empty() {
            let entity_type = event
                .data
                .get("entity_type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if !subs.entity_types.iter().any(|t| t == entity_type) {
                return false;
            }
        }
        true
    }

    async fn tick_health(&self) -> std::result::Result<(), HostError> {
        let snapshot: Vec<(String, Arc<dyn Plugin>)> = {
            let guard = self.plugins.read().await;
            guard
                .iter()
                .map(|(name, entry)| (name.clone(), entry.plugin.clone()))
                .collect()
        };
        for (name, plugin) in snapshot {
            let health = plugin.check_health().await;
            let guard = self.plugins.read().await;
            if let Some(entry) = guard.get(&name) {
                let mut record = entry.record.write().unwrap();
                record.last_health = Some(health);
                if record.state == PluginState::Error && record.consecutive_failures == 0 {
                    record.state = PluginState::Running;
                }
            }
        }
        Ok(())
    }

    /// Update a plugin's persistent state entry. Used by plugins
    /// that emit `PluginOutput::State` to retain small data across
    /// restarts.
    pub async fn update_state<F>(&self, name: &str, f: F) -> Result<(), HostError>
    where
        F: FnOnce(&mut PluginStateMap),
    {
        let mut state = self.state.lock().await;
        {
            let entry = state.entry_mut(name);
            f(entry);
        }
        state.persist()?;
        Ok(())
    }
}
