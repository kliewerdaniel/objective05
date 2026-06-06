//! Built-in plugin helpers.
//!
//! These plugins run in-process and are useful both for the
//! real system (e.g. logging every extraction event) and for tests
//! that need a known-good plugin to exercise the host.

use std::sync::Arc;

use async_trait::async_trait;
use objective_core::{types::EventEnvelope, Result};
use tokio::sync::Mutex;

use crate::manifest::PluginType;
use crate::traits::{builtin_manifest, Plugin, PluginOutput};

/// A plugin that records every event it sees. Useful for tests
/// and as a lightweight audit log in development.
pub struct AuditLogPlugin {
    manifest: crate::manifest::PluginManifest,
    log: Mutex<Vec<EventEnvelope>>,
}

impl AuditLogPlugin {
    pub fn new(event_types: Vec<String>) -> Self {
        Self {
            manifest: builtin_manifest("audit-log", PluginType::Filter, "0.1.0", event_types),
            log: Mutex::new(Vec::new()),
        }
    }

    pub async fn snapshot(&self) -> Vec<EventEnvelope> {
        self.log.lock().await.clone()
    }
}

#[async_trait]
impl Plugin for AuditLogPlugin {
    fn manifest(&self) -> &crate::manifest::PluginManifest {
        &self.manifest
    }

    async fn handle(&self, event: &EventEnvelope) -> Result<Vec<PluginOutput>> {
        self.log.lock().await.push(event.clone());
        Ok(vec![])
    }
}

/// A plugin that re-publishes every event it sees onto a
/// configurable subject. Used to demonstrate the `PluginOutput::Publish`
/// path.
pub struct ReEmitterPlugin {
    manifest: crate::manifest::PluginManifest,
    target_subject: String,
}

impl ReEmitterPlugin {
    pub fn new(event_types: Vec<String>, target_subject: impl Into<String>) -> Self {
        Self {
            manifest: builtin_manifest("re-emitter", PluginType::Processor, "0.1.0", event_types),
            target_subject: target_subject.into(),
        }
    }
}

#[async_trait]
impl Plugin for ReEmitterPlugin {
    fn manifest(&self) -> &crate::manifest::PluginManifest {
        &self.manifest
    }

    async fn handle(&self, event: &EventEnvelope) -> Result<Vec<PluginOutput>> {
        let subject = self.target_subject.clone();
        let new_event = EventEnvelope::new(
            "plugin.re_emitted",
            format!("plugin.{}", self.manifest.name),
            event.data.clone(),
        );
        Ok(vec![PluginOutput::Publish {
            subject,
            event: new_event,
        }])
    }
}

pub fn audit_log_plugin(event_types: Vec<String>) -> Arc<dyn Plugin> {
    Arc::new(AuditLogPlugin::new(event_types))
}

pub fn re_emitter_plugin(event_types: Vec<String>, subject: impl Into<String>) -> Arc<dyn Plugin> {
    Arc::new(ReEmitterPlugin::new(event_types, subject))
}
