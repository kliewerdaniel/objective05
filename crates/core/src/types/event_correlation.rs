use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ulid::Ulid;
use utoipa::ToSchema;

/// Lifecycle status of a derived event.
///
/// Events flow through a series of well-defined states from creation to
/// archival. The status drives downstream decisions such as broadcast
/// priority and whether the engine should still consider merging new
/// claims into the event.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    /// Less than three supporting claims or the event was created in
    /// the last hour and is still accumulating evidence.
    Forming,
    /// The event is being actively updated with new claims.
    Active,
    /// The event's scope or significance is changing rapidly.
    Evolving,
    /// No new claims have arrived in the last 48 hours.
    Stable,
    /// The event has a clear conclusion and is no longer expected to
    /// gain new claims.
    Resolved,
    /// The event has been moved to cold storage; it is still queryable
    /// for historical analysis but is not surfaced in the default
    /// feeds.
    Archived,
    /// The event was merged into another event. The original id is
    /// preserved on the merged record for provenance.
    Merged,
}

impl EventStatus {
    /// Returns `true` if the event should still accept new claims.
    pub fn accepts_claims(&self) -> bool {
        matches!(
            self,
            EventStatus::Forming
                | EventStatus::Active
                | EventStatus::Evolving
                | EventStatus::Stable
        )
    }
}

/// High-level classification of an event, loosely modelled on journalism
/// beats. The categorization drives both UI grouping and broadcast topic
/// selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Business,
    Politics,
    Technology,
    Science,
    Health,
    World,
    Sports,
    Entertainment,
    Other,
}

/// A concrete real-world event formed from one or more extracted claims.
///
/// Events are the atomic unit of intelligence produced by the Event
/// Engine. They are deduplicated against existing events, scored for
/// importance, and stored as nodes in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct Event {
    pub id: Ulid,
    pub title: String,
    pub description: String,
    pub event_type: EventType,
    pub status: EventStatus,
    pub importance: f32,
    pub confidence: f32,
    pub source_diversity: f32,
    pub claim_count: u32,
    pub participating_entities: Vec<String>,
    pub location: Option<String>,
    pub first_observed_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
    pub last_claim_at: DateTime<Utc>,
    pub claim_ids: Vec<String>,
    pub document_ids: Vec<String>,
    pub metadata: HashMap<String, Value>,
}

impl Event {
    /// Construct a new event from the very first claim that produced
    /// it. The event enters the `Forming` state and is assigned a fresh
    /// identifier.
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        event_type: EventType,
        first_claim: &FirstClaim,
        now: DateTime<Utc>,
    ) -> Self {
        let mut participating_entities = Vec::new();
        if !first_claim.subject_name.is_empty() {
            participating_entities.push(first_claim.subject_name.clone());
        }
        if let Some(object) = &first_claim.object_name {
            if !participating_entities.iter().any(|name| name == object) {
                participating_entities.push(object.clone());
            }
        }

        Self {
            id: Ulid::new(),
            title: title.into(),
            description: description.into(),
            event_type,
            status: EventStatus::Forming,
            importance: first_claim.confidence,
            confidence: first_claim.confidence,
            source_diversity: 1.0,
            claim_count: 1,
            participating_entities,
            location: first_claim.location.clone(),
            first_observed_at: first_claim.published_at.unwrap_or(now),
            last_updated_at: now,
            last_claim_at: now,
            claim_ids: vec![first_claim.claim_id.clone()],
            document_ids: first_claim.document_id.iter().cloned().collect(),
            metadata: HashMap::new(),
        }
    }
}

/// A condensed projection of a claim used by the Event Engine to
/// decide whether a new claim should be folded into an existing event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FirstClaim {
    pub claim_id: String,
    pub claim_text: String,
    pub subject_name: String,
    pub object_name: Option<String>,
    pub location: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub confidence: f32,
    pub document_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_status_accepts_claims_for_active_states() {
        assert!(EventStatus::Forming.accepts_claims());
        assert!(EventStatus::Active.accepts_claims());
        assert!(EventStatus::Evolving.accepts_claims());
        assert!(EventStatus::Stable.accepts_claims());
        assert!(!EventStatus::Resolved.accepts_claims());
        assert!(!EventStatus::Archived.accepts_claims());
        assert!(!EventStatus::Merged.accepts_claims());
    }

    #[test]
    fn test_event_new_seeds_participating_entities() {
        let first = FirstClaim {
            claim_id: "claim-1".to_string(),
            claim_text: "Apple announced a new factory".to_string(),
            subject_name: "Apple Inc".to_string(),
            object_name: Some("Austin".to_string()),
            location: Some("Austin".to_string()),
            published_at: None,
            confidence: 0.8,
            document_id: Some("doc-1".to_string()),
        };

        let event = Event::new(
            "Apple opens Austin factory",
            "Apple announced manufacturing expansion.",
            EventType::Business,
            &first,
            Utc::now(),
        );

        assert_eq!(event.status, EventStatus::Forming);
        assert_eq!(event.claim_count, 1);
        assert!(event
            .participating_entities
            .contains(&"Apple Inc".to_string()));
        assert!(event.participating_entities.contains(&"Austin".to_string()));
    }
}
