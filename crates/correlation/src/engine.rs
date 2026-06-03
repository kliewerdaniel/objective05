//! In-memory event engine.
//!
//! The [`EventEngine`] deduplicates incoming claims against existing
//! events using a deterministic, weighted similarity score. When the
//! best matching event scores above
//! [`EventEngineConfig::auto_merge_threshold`] the claim is folded into
//! the event, otherwise a new event is created.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use objective_core::{
    types::{Event, EventStatus, FirstClaim},
    Result,
};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::store::EventRepository;
use crate::titles::{generate_event_description, generate_event_title, infer_event_type};

/// Configuration knobs for the event engine.
#[derive(Debug, Clone)]
pub struct EventEngineConfig {
    pub auto_merge_threshold: f32,
    pub min_match_threshold: f32,
    pub top_n_matches: usize,
    pub stability_hours: i64,
    pub min_claim_confidence: f32,
}

impl Default for EventEngineConfig {
    fn default() -> Self {
        Self {
            auto_merge_threshold: 0.7,
            min_match_threshold: 0.35,
            top_n_matches: 3,
            stability_hours: 48,
            min_claim_confidence: 0.3,
        }
    }
}

/// What happened to a claim as a result of ingestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IngestOutcome {
    Merged(String),
    Created(String),
    Dropped,
}

/// The event engine itself.
pub struct EventEngine<R: EventRepository> {
    repository: Arc<R>,
    config: EventEngineConfig,
    cache: RwLock<Vec<Event>>,
    last_refresh: RwLock<Option<DateTime<Utc>>>,
}

impl<R: EventRepository> EventEngine<R> {
    pub fn new(repository: Arc<R>, config: EventEngineConfig) -> Self {
        Self {
            repository,
            config,
            cache: RwLock::new(Vec::new()),
            last_refresh: RwLock::new(None),
        }
    }

    pub fn with_defaults(repository: Arc<R>) -> Self {
        Self::new(repository, EventEngineConfig::default())
    }

    pub async fn ingest(&self, claim: FirstClaim) -> Result<IngestOutcome> {
        if claim.confidence < self.config.min_claim_confidence {
            debug!(
                confidence = claim.confidence,
                "dropping low-confidence claim"
            );
            return Ok(IngestOutcome::Dropped);
        }

        self.refresh_cache().await?;
        let mut cache = self.cache.write().await;
        let now = Utc::now();

        for event in cache.iter_mut() {
            transition_status(event, now, self.config.stability_hours);
        }

        let candidates = score_candidates(&cache, &claim, self.config.top_n_matches);
        let best = candidates.first().cloned();

        if let Some((event_id, score)) = best {
            if score >= self.config.auto_merge_threshold {
                if let Some(event) = cache
                    .iter_mut()
                    .find(|event| event.id.to_string() == event_id)
                {
                    merge_claim(event, &claim, now);
                    let event = event.clone();
                    drop(cache);
                    self.repository.save_event(event.clone()).await?;
                    info!(event_id = %event.id, score, "claim merged into event");
                    return Ok(IngestOutcome::Merged(event.id.to_string()));
                }
            }
        }

        let event_type = infer_event_type(&claim.claim_text);
        let title = generate_event_title(&claim);
        let description = generate_event_description(&claim);
        let event = Event::new(title, description, event_type, &claim, now);
        let id = event.id.to_string();
        cache.push(event.clone());
        self.repository.save_event(event).await?;
        info!(event_id = %id, "new event created");
        Ok(IngestOutcome::Created(id))
    }

    pub async fn ingest_batch(&self, claims: Vec<FirstClaim>) -> Result<Vec<IngestOutcome>> {
        let mut outcomes = Vec::with_capacity(claims.len());
        for claim in claims {
            outcomes.push(self.ingest(claim).await?);
        }
        Ok(outcomes)
    }

    pub async fn list_events(&self) -> Result<Vec<Event>> {
        self.refresh_cache().await?;
        Ok(self.cache.read().await.clone())
    }

    pub async fn recent_events(&self, limit: usize) -> Result<Vec<Event>> {
        let mut events = self.list_events().await?;
        events.sort_by(|a, b| b.last_updated_at.cmp(&a.last_updated_at));
        events.truncate(limit);
        Ok(events)
    }

    pub async fn top_events_by_importance(&self, limit: usize) -> Result<Vec<Event>> {
        let mut events = self.list_events().await?;
        events.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        events.truncate(limit);
        Ok(events)
    }

    pub async fn find_event(&self, id: &str) -> Result<Option<Event>> {
        self.refresh_cache().await?;
        Ok(self
            .cache
            .read()
            .await
            .iter()
            .find(|event| event.id.to_string() == id)
            .cloned())
    }

    pub async fn run_maintenance(&self) -> Result<usize> {
        self.refresh_cache().await?;
        let now = Utc::now();
        let mut cache = self.cache.write().await;
        let mut updated = 0;
        for event in cache.iter_mut() {
            let prior = event.status;
            transition_status(event, now, self.config.stability_hours);
            if event.status != prior {
                updated += 1;
            }
        }
        if updated > 0 {
            let snapshot = cache.clone();
            drop(cache);
            self.repository.replace_all(snapshot).await?;
        }
        Ok(updated)
    }

    pub fn config(&self) -> &EventEngineConfig {
        &self.config
    }

    async fn refresh_cache(&self) -> Result<()> {
        let should_refresh = {
            let last = self.last_refresh.read().await;
            match *last {
                None => true,
                Some(timestamp) => Utc::now() - timestamp > Duration::seconds(1),
            }
        };

        if should_refresh {
            let events = self.repository.list_events().await?;
            *self.cache.write().await = events;
            *self.last_refresh.write().await = Some(Utc::now());
        }
        Ok(())
    }
}

fn score_candidates(events: &[Event], claim: &FirstClaim, top_n: usize) -> Vec<(String, f32)> {
    let mut scored: Vec<(String, f32)> = events
        .iter()
        .filter(|event| event.status.accepts_claims())
        .map(|event| {
            let score = similarity(event, claim);
            (event.id.to_string(), score)
        })
        .filter(|(_, score)| *score > 0.0)
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_n);
    scored
}

fn similarity(event: &Event, claim: &FirstClaim) -> f32 {
    let subject_match = entity_overlap(&event.participating_entities, &claim.subject_name);
    let object_match = claim
        .object_name
        .as_deref()
        .map(|object| entity_overlap(&event.participating_entities, object))
        .unwrap_or(0.0);
    let location_match = match (&event.location, &claim.location) {
        (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => 0.2,
        _ => 0.0,
    };
    let predicate_match = predicate_overlap(&event.title, &event.description, &claim.claim_text);

    // The entity component dominates: two claims about the same
    // subject and object must always merge even when the predicate
    // language differs. Predicate overlap is a tie-breaker when the
    // entity match is partial.
    let entity_score = 0.5 * subject_match + 0.3 * object_match;
    let predicate_score = 0.2 * predicate_match;
    let location_score = location_match;

    (entity_score + predicate_score + location_score).clamp(0.0, 1.0)
}

fn entity_overlap(entities: &[String], candidate: &str) -> f32 {
    if entities.is_empty() || candidate.is_empty() {
        return 0.0;
    }
    let candidate_lower = candidate.to_lowercase();
    if entities
        .iter()
        .any(|entity| entity.to_lowercase() == candidate_lower)
    {
        return 1.0;
    }
    if entities
        .iter()
        .any(|entity| candidate_lower.contains(&entity.to_lowercase()))
    {
        return 0.6;
    }
    0.0
}

fn predicate_overlap(title: &str, description: &str, claim_text: &str) -> f32 {
    let claim_words: Vec<String> = claim_text
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| word.len() > 3)
        .collect();
    if claim_words.is_empty() {
        return 0.0;
    }
    let haystack = format!("{title} {description}").to_lowercase();
    let matches = claim_words
        .iter()
        .filter(|word| haystack.contains(word.as_str()))
        .count();
    matches as f32 / claim_words.len() as f32
}

fn merge_claim(event: &mut Event, claim: &FirstClaim, now: DateTime<Utc>) {
    let total_confidence = event.confidence * event.claim_count as f32 + claim.confidence;
    event.claim_count += 1;
    event.confidence = total_confidence / event.claim_count as f32;

    if !event
        .participating_entities
        .iter()
        .any(|name| name == &claim.subject_name)
    {
        event
            .participating_entities
            .push(claim.subject_name.clone());
    }
    if let Some(object) = &claim.object_name {
        if !event
            .participating_entities
            .iter()
            .any(|name| name == object)
        {
            event.participating_entities.push(object.clone());
        }
    }
    if event.location.is_none() {
        event.location = claim.location.clone();
    }

    if !event.claim_ids.iter().any(|id| id == &claim.claim_id) {
        event.claim_ids.push(claim.claim_id.clone());
    }
    if let Some(document_id) = &claim.document_id {
        if !event.document_ids.iter().any(|id| id == document_id) {
            event.document_ids.push(document_id.clone());
        }
    }

    if let Some(published_at) = claim.published_at {
        if published_at < event.first_observed_at {
            event.first_observed_at = published_at;
        }
    }

    event.last_claim_at = now;
    event.last_updated_at = now;
    event.importance = compute_importance(event);
    if event.claim_count >= 3 {
        event.status = EventStatus::Active;
    } else {
        event.status = EventStatus::Forming;
    }
}

fn compute_importance(event: &Event) -> f32 {
    let evidence_volume = (event.claim_count as f32 / 50.0).min(1.0);
    let recency_hours = (Utc::now() - event.last_claim_at).num_hours().max(0) as f32;
    let recency = (1.0 - recency_hours / 168.0).max(0.0);
    (0.45 * evidence_volume + 0.35 * event.source_diversity + 0.2 * recency).clamp(0.0, 1.0)
}

fn transition_status(event: &mut Event, now: DateTime<Utc>, stability_hours: i64) {
    if matches!(
        event.status,
        EventStatus::Resolved | EventStatus::Archived | EventStatus::Merged
    ) {
        return;
    }
    if event.claim_count >= 3 {
        event.status = EventStatus::Active;
    }
    let hours_since_claim = (now - event.last_claim_at).num_hours();
    if hours_since_claim >= stability_hours {
        event.status = EventStatus::Stable;
    }
    event.importance = compute_importance(event);
    event.source_diversity =
        (1.0 + 0.1 * (event.claim_count.saturating_sub(1) as f32).min(3.0)).min(1.3) / 1.3;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryEventRepository;
    use std::sync::Arc;

    fn make_claim(
        id: &str,
        subject: &str,
        object: Option<&str>,
        text: &str,
        location: Option<&str>,
        confidence: f32,
    ) -> FirstClaim {
        FirstClaim {
            claim_id: id.to_string(),
            claim_text: text.to_string(),
            subject_name: subject.to_string(),
            object_name: object.map(|s| s.to_string()),
            location: location.map(|s| s.to_string()),
            published_at: Some(Utc::now()),
            confidence,
            document_id: Some(format!("doc-{id}")),
        }
    }

    #[tokio::test]
    async fn test_first_claim_creates_new_event() {
        let repository = Arc::new(InMemoryEventRepository::new());
        let engine = EventEngine::with_defaults(Arc::clone(&repository));

        let claim = make_claim(
            "1",
            "Apple Inc",
            Some("Austin"),
            "Apple Inc announced a 10% expansion in Austin.",
            Some("Austin"),
            0.8,
        );
        let outcome = engine.ingest(claim).await.unwrap();
        assert!(matches!(outcome, IngestOutcome::Created(_)));

        let events = engine.list_events().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].claim_count, 1);
        assert!(events[0]
            .participating_entities
            .iter()
            .any(|entity| entity == "Apple Inc"));
    }

    #[tokio::test]
    async fn test_similar_claims_merge_into_single_event() {
        let repository = Arc::new(InMemoryEventRepository::new());
        let engine = EventEngine::with_defaults(Arc::clone(&repository));

        let first = make_claim(
            "1",
            "Apple Inc",
            Some("Austin"),
            "Apple Inc announced a 10% expansion in Austin.",
            Some("Austin"),
            0.8,
        );
        let second = make_claim(
            "2",
            "Apple Inc",
            Some("Austin"),
            "Apple Inc announced manufacturing expansion in Austin.",
            Some("Austin"),
            0.7,
        );

        let outcome_first = engine.ingest(first).await.unwrap();
        let outcome_second = engine.ingest(second).await.unwrap();
        assert!(matches!(outcome_first, IngestOutcome::Created(_)));
        assert!(matches!(outcome_second, IngestOutcome::Merged(_)));

        let events = engine.list_events().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].claim_count, 2);
    }

    #[tokio::test]
    async fn test_low_confidence_claim_is_dropped() {
        let repository = Arc::new(InMemoryEventRepository::new());
        let engine = EventEngine::with_defaults(Arc::clone(&repository));

        let claim = make_claim(
            "1",
            "Apple Inc",
            Some("Austin"),
            "Apple Inc announced expansion.",
            Some("Austin"),
            0.1,
        );
        let outcome = engine.ingest(claim).await.unwrap();
        assert_eq!(outcome, IngestOutcome::Dropped);
        assert!(engine.list_events().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_top_events_by_importance_orders_correctly() {
        let repository = Arc::new(InMemoryEventRepository::new());
        let engine = EventEngine::with_defaults(Arc::clone(&repository));

        // Force merge by feeding the same event cluster three times.
        for index in 0..3 {
            let claim = make_claim(
                &format!("{index}"),
                "Apple Inc",
                Some("Austin"),
                "Apple Inc announced a 10% manufacturing expansion in Austin.",
                Some("Austin"),
                0.8,
            );
            engine.ingest(claim).await.unwrap();
        }

        let top = engine.top_events_by_importance(5).await.unwrap();
        assert!(!top.is_empty());
        // Higher importance should be first.
        for window in top.windows(2) {
            assert!(window[0].importance >= window[1].importance);
        }
    }

    #[tokio::test]
    async fn test_maintenance_transitions_status_for_stale_events() {
        let repository = Arc::new(InMemoryEventRepository::new());
        let config = EventEngineConfig {
            stability_hours: 0,
            ..EventEngineConfig::default()
        };
        let engine = EventEngine::new(Arc::clone(&repository), config);

        let claim = make_claim(
            "1",
            "Apple Inc",
            Some("Austin"),
            "Apple Inc announced expansion in Austin.",
            Some("Austin"),
            0.8,
        );
        engine.ingest(claim).await.unwrap();
        let updated = engine.run_maintenance().await.unwrap();
        assert!(updated >= 1);
        let event = engine.list_events().await.unwrap().pop().unwrap();
        assert_eq!(event.status, EventStatus::Stable);
    }
}
