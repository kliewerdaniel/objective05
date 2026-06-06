//! Plugin host crate for the Objective platform.
//!
//! The plugin host is the runtime owner of every plugin: it
//! discovers manifests under `.objective/plugins/`, manages plugin
//! lifecycle (start, health, crash, restart with backoff), and
//! routes bus events to subscribed plugins.
//!
//! The v1 host is fully in-process — built-in plugins are Rust
//! `dyn Plugin` objects registered at startup. External plugins
//! will arrive later behind the same trait so the API surface
//! stays stable.

pub mod builtin;
pub mod host;
pub mod manifest;
pub mod state;
pub mod traits;

pub use builtin::{audit_log_plugin, re_emitter_plugin, AuditLogPlugin, ReEmitterPlugin};
pub use host::{HostError, PluginHost, PluginRecord, PluginStatus};
pub use manifest::{
    discover, DiscoveryError, PluginCounts, PluginManifest, PluginSubscription, PluginType,
};
pub use state::{PluginStateFile, PluginStateMap, PluginStateStore, StateError};
pub use traits::{
    builtin_manifest, BuiltinPlugin, HostConfig, NoopPlugin, Plugin, PluginHealth, PluginOutput,
    PluginState,
};
