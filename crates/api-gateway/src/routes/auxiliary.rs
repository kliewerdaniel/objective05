//! Auxiliary in-memory stores backing API endpoints that don't yet have a
//! dedicated persistence layer in the core crates.
//!
//! These stores are intentionally simple: a `RwLock<HashMap>` behind an
//! `Arc`. They live inside the API gateway process only — restarting the
//! daemon resets them. That's appropriate for endpoints that model
//! editorial state (narratives, broadcasts) or ephemeral coordination
//! signals (contradictions, manual events) that don't need to survive a
//! restart.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ulid::Ulid;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NarrativeStatus {
    Forming,
    Active,
    Stable,
    Fading,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NarrativeRecord {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: NarrativeStatus,
    pub event_count: usize,
    pub claim_ids: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum BroadcastStatus {
    Draft,
    Ready,
    Published,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BroadcastRecord {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub body_markdown: String,
    pub status: BroadcastStatus,
    pub event_count: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ContradictionStatus {
    Open,
    Investigating,
    Resolved,
    Dismissed,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContradictionRecord {
    pub id: String,
    pub claim_a: String,
    pub claim_b: String,
    pub entity_name: String,
    pub confidence: f32,
    pub severity: f32,
    pub status: ContradictionStatus,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolution_note: Option<String>,
}

/// Bundle of auxiliary stores that the API gateway routes share. Cloned
/// cheaply (everything is behind `Arc`); pass into `ApiState` builders.
#[derive(Clone, Default)]
pub struct AuxiliaryStores {
    pub narratives: Arc<NarrativeStore>,
    pub broadcasts: Arc<BroadcastStore>,
    pub contradictions: Arc<ContradictionStore>,
}

impl AuxiliaryStores {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Default)]
pub struct NarrativeStore {
    inner: RwLock<HashMap<String, NarrativeRecord>>,
}

impl NarrativeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn list(&self) -> Vec<NarrativeRecord> {
        let mut values: Vec<NarrativeRecord> = self.inner.read().await.values().cloned().collect();
        values.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        values
    }

    pub async fn get(&self, id: &str) -> Option<NarrativeRecord> {
        self.inner.read().await.get(id).cloned()
    }

    pub async fn upsert(&self, record: NarrativeRecord) {
        self.inner.write().await.insert(record.id.clone(), record);
    }

    pub async fn delete(&self, id: &str) -> Option<NarrativeRecord> {
        self.inner.write().await.remove(id)
    }
}

#[derive(Debug, Default)]
pub struct BroadcastStore {
    inner: RwLock<HashMap<String, BroadcastRecord>>,
}

impl BroadcastStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn list(&self) -> Vec<BroadcastRecord> {
        let mut values: Vec<BroadcastRecord> = self.inner.read().await.values().cloned().collect();
        values.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        values
    }

    pub async fn latest(&self) -> Option<BroadcastRecord> {
        self.list().await.into_iter().next()
    }

    pub async fn get(&self, id: &str) -> Option<BroadcastRecord> {
        self.inner.read().await.get(id).cloned()
    }

    pub async fn insert(&self, record: BroadcastRecord) {
        self.inner.write().await.insert(record.id.clone(), record);
    }
}

#[derive(Debug, Default)]
pub struct ContradictionStore {
    inner: RwLock<HashMap<String, ContradictionRecord>>,
}

impl ContradictionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn list(&self) -> Vec<ContradictionRecord> {
        let mut values: Vec<ContradictionRecord> =
            self.inner.read().await.values().cloned().collect();
        values.sort_by(|a, b| b.detected_at.cmp(&a.detected_at));
        values
    }

    pub async fn get(&self, id: &str) -> Option<ContradictionRecord> {
        self.inner.read().await.get(id).cloned()
    }

    pub async fn insert(&self, record: ContradictionRecord) {
        self.inner.write().await.insert(record.id.clone(), record);
    }

    pub async fn resolve(
        &self,
        id: &str,
        status: ContradictionStatus,
        note: Option<String>,
    ) -> Option<ContradictionRecord> {
        let mut guard = self.inner.write().await;
        if let Some(record) = guard.get_mut(id) {
            record.status = status;
            record.resolved_at = Some(chrono::Utc::now());
            record.resolution_note = note;
            Some(record.clone())
        } else {
            None
        }
    }
}

pub fn new_id() -> String {
    Ulid::new().to_string()
}
