use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastStatus {
    Draft,
    Ready,
    Published,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastRecord {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub body_markdown: String,
    pub status: BroadcastStatus,
    pub event_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BroadcastRecord {
    pub fn new(title: String, summary: String, body_markdown: String, event_count: usize) -> Self {
        let now = Utc::now();
        Self {
            id: Ulid::new().to_string(),
            title,
            summary,
            body_markdown,
            status: BroadcastStatus::Ready,
            event_count,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BroadcastCollection {
    pub top_events: Vec<ScoredEvent>,
    pub narratives: Vec<ScoredNarrative>,
    pub contradictions: Vec<ScoredContradiction>,
    pub collection_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ScoredEvent {
    pub id: String,
    pub title: String,
    pub description: String,
    pub importance: f32,
    pub confidence: f32,
    pub claim_count: u32,
    pub event_type: String,
    pub entities: Vec<String>,
    pub last_updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ScoredNarrative {
    pub id: String,
    pub title: String,
    pub strength: f32,
    pub event_count: usize,
    pub last_updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ScoredContradiction {
    pub id: String,
    pub description: String,
    pub severity: f32,
    pub entity_name: String,
    pub created_at: DateTime<Utc>,
}

impl BroadcastCollection {
    pub fn is_empty(&self) -> bool {
        self.top_events.is_empty()
            && self.narratives.is_empty()
            && self.contradictions.is_empty()
    }

    pub fn total_count(&self) -> usize {
        self.top_events.len() + self.narratives.len() + self.contradictions.len()
    }
}
