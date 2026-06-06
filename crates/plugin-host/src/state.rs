//! Persistent plugin state.
//!
//! Each plugin may stash small key/value pairs that survive
//! restarts. The state is stored as a single JSON file under
//! `.objective/state/plugins.json` so it stays local-first and
//! inspectable from the filesystem.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// State for a single plugin: arbitrary JSON values keyed by string.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PluginStateMap {
    pub values: BTreeMap<String, serde_json::Value>,
}

impl PluginStateMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: String, value: serde_json::Value) {
        self.values.insert(key, value);
    }
}

/// Top-level state file. One file, one map of plugin-name -> state.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PluginStateFile {
    pub plugins: BTreeMap<String, PluginStateMap>,
}

#[derive(Debug, Error)]
pub enum StateError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn load_or_default(path: &Path) -> Result<PluginStateFile, StateError> {
    if !path.exists() {
        return Ok(PluginStateFile::default());
    }
    let raw = std::fs::read_to_string(path)?;
    let file: PluginStateFile = serde_json::from_str(&raw)?;
    Ok(file)
}

pub fn save(path: &Path, state: &PluginStateFile) -> Result<(), StateError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(state)?;
    std::fs::write(path, raw)?;
    Ok(())
}

/// Convenience wrapper that owns the path and serializes access.
pub struct PluginStateStore {
    path: PathBuf,
    state: PluginStateFile,
}

impl PluginStateStore {
    pub fn load(path: PathBuf) -> Result<Self, StateError> {
        let state = load_or_default(&path)?;
        Ok(Self { path, state })
    }

    pub fn snapshot(&self) -> &PluginStateFile {
        &self.state
    }

    pub fn entry_mut(&mut self, name: &str) -> &mut PluginStateMap {
        self.state.plugins.entry(name.to_string()).or_default()
    }

    pub fn persist(&self) -> Result<(), StateError> {
        save(&self.path, &self.state)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
