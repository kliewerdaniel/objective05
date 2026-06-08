use std::sync::Arc;

use objective_core::types::EventStatus;
use objective_correlation::EventRepository;

use crate::types::{BroadcastCollection, ScoredEvent};

pub struct BroadcastCollector {
    event_repository: Arc<dyn EventRepository>,
}

impl BroadcastCollector {
    pub fn new(event_repository: Arc<dyn EventRepository>) -> Self {
        Self { event_repository }
    }

    pub async fn collect(&self) -> objective_core::Result<BroadcastCollection> {
        let now = chrono::Utc::now();
        let all_events = self.event_repository.list_events().await?;

        let mut top_events: Vec<ScoredEvent> = all_events
            .iter()
            .filter(|e| {
                matches!(
                    e.status,
                    EventStatus::Forming | EventStatus::Active | EventStatus::Evolving
                )
            })
            .map(|e| ScoredEvent {
                id: e.id.to_string(),
                title: e.title.clone(),
                description: e.description.clone(),
                importance: e.importance,
                confidence: e.confidence,
                claim_count: e.claim_count,
                event_type: format!("{:?}", e.event_type),
                entities: e.participating_entities.clone(),
                last_updated_at: e.last_updated_at,
            })
            .collect();

        top_events.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let top_events = top_events.into_iter().take(20).collect();

        Ok(BroadcastCollection {
            top_events,
            narratives: Vec::new(),
            contradictions: Vec::new(),
            collection_timestamp: now,
        })
    }
}
