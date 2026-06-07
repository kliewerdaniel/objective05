use std::sync::Arc;

use objective_core::{
    traits::{DocumentProcessor, ExtractionRepository, MessageBus, SourceAdapter},
    types::{EventEnvelope, FirstClaim},
    Result,
};
use objective_correlation::{store::EventRepository, EventEngine};
use objective_ingestion::IngestionService;
use objective_store::{
    monitoring::MonitoringService, retry_queue::RetryQueue, snapshot::SnapshotService, RuntimeStore,
};
use serde_json::json;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

/// Event-driven pipeline worker that consumes scheduler triggers and drives
/// ingestion, extraction, and event correlation stages.
pub struct PipelineWorker<B: MessageBus, R: EventRepository> {
    ingestion: IngestionService<RuntimeStore, B>,
    processor: Arc<dyn DocumentProcessor>,
    store: Arc<RuntimeStore>,
    bus: Arc<B>,
    sources: Vec<Arc<dyn SourceAdapter>>,
    event_engine: Arc<EventEngine<R>>,
    snapshot_service: Arc<SnapshotService>,
    retry_queue: Arc<RetryQueue>,
    monitoring: Arc<MonitoringService>,
    poll_interval_secs: u64,
}

impl<B: MessageBus, R: EventRepository> PipelineWorker<B, R> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ingestion: IngestionService<RuntimeStore, B>,
        processor: Arc<dyn DocumentProcessor>,
        store: Arc<RuntimeStore>,
        bus: Arc<B>,
        sources: Vec<Arc<dyn SourceAdapter>>,
        event_engine: Arc<EventEngine<R>>,
        snapshot_service: Arc<SnapshotService>,
        retry_queue: Arc<RetryQueue>,
        monitoring: Arc<MonitoringService>,
    ) -> Self {
        Self {
            ingestion,
            processor,
            store,
            bus,
            sources,
            event_engine,
            snapshot_service,
            retry_queue,
            monitoring,
            poll_interval_secs: 5,
        }
    }

    /// Run the worker loop, polling for new bus events and dispatching them.
    pub async fn run(&self) -> Result<()> {
        info!("pipeline worker starting");
        let mut last_index: usize = 0;
        let mut ticker = interval(Duration::from_secs(self.poll_interval_secs));

        loop {
            ticker.tick().await;
            self.monitoring.record_pipeline_cycle();

            // Process retry queue
            if let Err(e) = self.process_retries().await {
                warn!(error = %e, "retry queue processing failed");
                self.monitoring.record_error();
            }

            let events = self.bus.events().await?;
            if events.len() <= last_index {
                continue;
            }

            let new_events: Vec<_> = events[last_index..].to_vec();
            last_index = events.len();

            for (_subject, event) in new_events {
                if let Err(e) = self.dispatch(&event).await {
                    error!(event_type = %event.event_type, error = %e, "pipeline dispatch failed");
                    self.monitoring.record_error();
                    let attempt = 0;
                    if let Err(re) = self.retry_queue.enqueue(
                        &event.event_type,
                        &_subject,
                        &e.to_string(),
                        attempt,
                    ) {
                        error!(error = %re, "failed to enqueue retry job");
                    }
                }
            }
        }
    }

    async fn process_retries(&self) -> Result<()> {
        let ready = self.retry_queue.ready_jobs()?;
        for job in ready {
            info!(job_id = %job.id, event_type = %job.event_type, "processing retry job");
            self.monitoring.record_retry();
            let event = EventEnvelope::new(&job.event_type, &job.source_subject, json!({}));
            match self.dispatch(&event).await {
                Ok(()) => {
                    self.retry_queue.complete(&job.id)?;
                    info!(job_id = %job.id, "retry job completed");
                }
                Err(e) => {
                    warn!(job_id = %job.id, error = %e, "retry job failed again");
                    if let Err(re) = self.retry_queue.re_enqueue(&job) {
                        error!(error = %re, "failed to re-enqueue retry job");
                    }
                    self.monitoring.record_dead_letter();
                }
            }
        }
        Ok(())
    }

    async fn dispatch(&self, event: &EventEnvelope) -> Result<()> {
        match event.event_type.as_str() {
            "ingestion.poll.rss" => {
                info!("handling ingestion.poll.rss");
                self.run_ingestion().await?;
            }
            "extraction.process.pending" => {
                info!("handling extraction.process.pending");
                self.run_extraction().await?;
            }
            "system.maintenance" => {
                info!("handling system.maintenance");
                self.run_maintenance().await?;
            }
            "system.snapshot" => {
                info!("handling system.snapshot");
                self.run_snapshot()?;
            }
            other => {
                info!(event_type = other, "ignoring unhandled pipeline event");
            }
        }
        Ok(())
    }

    async fn run_ingestion(&self) -> Result<()> {
        let mut total = 0;
        for source in &self.sources {
            match self.ingestion.poll_source(source.as_ref()).await {
                Ok(count) => {
                    total += count;
                    for _ in 0..count {
                        self.monitoring.record_document_ingested();
                    }
                    info!(source = source.name(), count, "ingested documents");
                }
                Err(e) => {
                    warn!(source = source.name(), error = %e, "source poll failed");
                }
            }
        }
        info!(total, "ingestion cycle complete");
        Ok(())
    }

    async fn run_extraction(&self) -> Result<()> {
        use objective_core::traits::DocumentRepository;

        let documents = self.store.list_documents().await?;
        let mut processed = 0;

        for document in &documents {
            let extraction = self.processor.process(document).await?;
            self.store.save_extraction(extraction.clone()).await?;
            self.monitoring.record_extraction_completed();

            for (index, claim) in extraction.claims.iter().enumerate() {
                let first_claim = first_claim_from(document, claim, index);
                if let Err(e) = self.event_engine.ingest(first_claim).await {
                    warn!(error = %e, "event_engine.ingest failed");
                } else {
                    self.monitoring.record_event_created();
                }
            }

            let payload = json!({
                "document_id": document.id.to_string(),
                "entity_count": extraction.entities.len(),
                "claim_count": extraction.claims.len(),
            });
            self.bus
                .publish(
                    "extraction.document.processed",
                    EventEnvelope::new("extraction.document.processed", "pipeline.worker", payload),
                )
                .await?;

            processed += 1;
        }

        info!(processed, "extraction cycle complete");
        Ok(())
    }

    async fn run_maintenance(&self) -> Result<()> {
        let updated = self.event_engine.run_maintenance().await?;
        info!(updated, "event maintenance complete");
        Ok(())
    }

    fn run_snapshot(&self) -> Result<()> {
        match self.snapshot_service.create_snapshot() {
            Ok(path) => {
                self.monitoring.record_snapshot();
                info!(path = %path.display(), "snapshot created");
            }
            Err(e) => {
                warn!(error = %e, "snapshot failed");
                self.monitoring.record_error();
            }
        }
        Ok(())
    }
}

/// Convert a raw extracted claim into the `FirstClaim` projection consumed by
/// the event engine.
pub fn first_claim_from(
    document: &objective_core::types::RawDocument,
    claim: &objective_core::types::ExtractedClaim,
    index: usize,
) -> FirstClaim {
    let location = location_from_document(document);
    let claim_id = format!("{}#{index}", document.id);
    FirstClaim {
        claim_id,
        claim_text: claim.claim_text.clone(),
        subject_name: claim.subject_name.clone(),
        object_name: claim.object_name.clone(),
        location,
        published_at: document.published_at,
        confidence: claim.confidence,
        document_id: Some(document.id.to_string()),
    }
}

fn location_from_document(document: &objective_core::types::RawDocument) -> Option<String> {
    let metadata = &document.metadata;
    if let Some(value) = metadata.get("location") {
        if let Some(text) = value.as_str() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(value) = metadata.get("place") {
        if let Some(text) = value.as_str() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(title) = &document.title {
        let trimmed = title.trim();
        if !trimmed.is_empty() && trimmed.len() < 64 {
            return Some(trimmed.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_core::types::{BodyFormat, RawDocument};
    use objective_correlation::InMemoryEventRepository;
    use objective_extraction::HeuristicExtractionService;
    use objective_ingestion::adapters::StaticSourceAdapter;
    use objective_message_bus::InMemoryMessageBus;
    use objective_store::{
        monitoring::MonitoringService, retry_queue::RetryQueue, snapshot::SnapshotService,
        RuntimeStore,
    };
    use std::collections::HashMap;
    use ulid::Ulid;

    fn make_event_engine() -> Arc<EventEngine<InMemoryEventRepository>> {
        let repo = Arc::new(InMemoryEventRepository::new());
        Arc::new(EventEngine::with_defaults(repo))
    }

    fn make_snapshot_service() -> Arc<SnapshotService> {
        let dir = tempfile::tempdir().unwrap().keep();
        Arc::new(SnapshotService::new(
            &dir,
            &dir.join("state"),
            &dir.join("documents"),
        ))
    }

    fn make_retry_queue() -> Arc<RetryQueue> {
        let dir = tempfile::tempdir().unwrap().keep();
        Arc::new(RetryQueue::new(&dir, 3, 0))
    }

    fn make_monitoring() -> Arc<MonitoringService> {
        let dir = tempfile::tempdir().unwrap().keep();
        Arc::new(MonitoringService::new(&dir))
    }

    async fn make_store_with_document() -> Arc<RuntimeStore> {
        use objective_core::traits::DocumentRepository;

        let store = Arc::new(RuntimeStore::new(std::path::PathBuf::from(
            "/tmp/objective-test-pipeline",
        )));
        let doc = RawDocument {
            id: Ulid::new(),
            source_id: "fixture".to_string(),
            source_type: "fixture".to_string(),
            external_id: "1".to_string(),
            url: None,
            title: Some("Test".to_string()),
            body: "Apple Inc announced a 10% manufacturing expansion in Austin.".to_string(),
            body_format: BodyFormat::PlainText,
            author: None,
            published_at: None,
            fetched_at: chrono::Utc::now(),
            language: "en".to_string(),
            content_hash: "test-hash".to_string(),
            metadata: HashMap::new(),
            raw_bytes: None,
        };
        store.save_document(doc).await.unwrap();
        store
    }

    #[tokio::test]
    async fn test_dispatch_ingestion_poll_rss() {
        let store = Arc::new(RuntimeStore::new(std::path::PathBuf::from(
            "/tmp/objective-test-dispatch",
        )));
        let bus = Arc::new(InMemoryMessageBus::new());
        let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));
        let processor: Arc<dyn DocumentProcessor> = Arc::new(HeuristicExtractionService);
        let source: Arc<dyn SourceAdapter> = Arc::new(StaticSourceAdapter::from_plain_text(
            "test_source",
            "Test Article",
            "Apple Inc announced expansion in Austin.",
        ));
        let event_engine = make_event_engine();
        let snapshot_service = make_snapshot_service();
        let retry_queue = make_retry_queue();
        let monitoring = make_monitoring();

        let worker = PipelineWorker::new(
            ingestion,
            processor,
            Arc::clone(&store),
            Arc::clone(&bus),
            vec![source],
            event_engine,
            snapshot_service,
            retry_queue,
            monitoring,
        );

        let event = EventEnvelope::new("ingestion.poll.rss", "scheduler.rss_poll", json!({}));
        worker.dispatch(&event).await.unwrap();

        use objective_core::traits::DocumentRepository;
        let docs = store.list_documents().await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].source_id, "test_source");
    }

    #[tokio::test]
    async fn test_dispatch_extraction_process_pending() {
        let store = make_store_with_document().await;
        let bus = Arc::new(InMemoryMessageBus::new());
        let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));
        let processor: Arc<dyn DocumentProcessor> = Arc::new(HeuristicExtractionService);
        let event_engine = make_event_engine();
        let snapshot_service = make_snapshot_service();
        let retry_queue = make_retry_queue();
        let monitoring = make_monitoring();

        let worker = PipelineWorker::new(
            ingestion,
            processor,
            Arc::clone(&store),
            Arc::clone(&bus),
            vec![],
            Arc::clone(&event_engine),
            snapshot_service,
            retry_queue,
            monitoring,
        );

        let event = EventEnvelope::new(
            "extraction.process.pending",
            "scheduler.extraction_batch",
            json!({}),
        );
        worker.dispatch(&event).await.unwrap();

        use objective_core::traits::ExtractionRepository;
        let extractions = store.list_extractions().await.unwrap();
        assert_eq!(extractions.len(), 1);
        assert!(!extractions[0].entities.is_empty());

        let bus_events = bus.events().await.unwrap();
        assert!(bus_events
            .iter()
            .any(|(_, e)| e.event_type == "extraction.document.processed"));

        // Verify claims were fed into the event engine
        let events = event_engine.list_events().await.unwrap();
        assert!(
            !events.is_empty(),
            "event engine should have derived events"
        );
    }

    #[tokio::test]
    async fn test_extraction_feeds_claims_into_event_engine() {
        let store = make_store_with_document().await;
        let bus = Arc::new(InMemoryMessageBus::new());
        let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));
        let processor: Arc<dyn DocumentProcessor> = Arc::new(HeuristicExtractionService);
        let event_engine = make_event_engine();
        let snapshot_service = make_snapshot_service();
        let retry_queue = make_retry_queue();
        let monitoring = make_monitoring();

        let worker = PipelineWorker::new(
            ingestion,
            processor,
            Arc::clone(&store),
            Arc::clone(&bus),
            vec![],
            Arc::clone(&event_engine),
            snapshot_service,
            retry_queue,
            monitoring,
        );

        let event = EventEnvelope::new(
            "extraction.process.pending",
            "scheduler.extraction_batch",
            json!({}),
        );
        worker.dispatch(&event).await.unwrap();

        let events = event_engine.list_events().await.unwrap();
        assert!(
            !events.is_empty(),
            "event engine should have at least 1 derived event from the seed document"
        );

        // The document mentions "Apple Inc" — the engine should have an event about it
        let has_apple = events.iter().any(|e| {
            e.title.to_lowercase().contains("apple")
                || e.participating_entities
                    .iter()
                    .any(|name| name.to_lowercase().contains("apple"))
        });
        assert!(has_apple, "derived event should mention Apple Inc");
    }

    #[tokio::test]
    async fn test_dispatch_ignores_unknown_events() {
        let store = Arc::new(RuntimeStore::new(std::path::PathBuf::from(
            "/tmp/objective-test-unknown",
        )));
        let bus = Arc::new(InMemoryMessageBus::new());
        let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));
        let processor: Arc<dyn DocumentProcessor> = Arc::new(HeuristicExtractionService);
        let event_engine = make_event_engine();
        let snapshot_service = make_snapshot_service();
        let retry_queue = make_retry_queue();
        let monitoring = make_monitoring();

        let worker = PipelineWorker::new(
            ingestion,
            processor,
            Arc::clone(&store),
            Arc::clone(&bus),
            vec![],
            Arc::clone(&event_engine),
            snapshot_service,
            retry_queue,
            monitoring,
        );

        let event = EventEnvelope::new("unknown.event", "test", json!({}));
        let result = worker.dispatch(&event).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_first_claim_from_includes_document_id() {
        use objective_core::types::ClaimType;

        let doc = RawDocument {
            id: Ulid::new(),
            source_id: "test".to_string(),
            source_type: "test".to_string(),
            external_id: "1".to_string(),
            url: None,
            title: Some("Test".to_string()),
            body: "body".to_string(),
            body_format: BodyFormat::PlainText,
            author: None,
            published_at: None,
            fetched_at: chrono::Utc::now(),
            language: "en".to_string(),
            content_hash: "hash".to_string(),
            metadata: HashMap::new(),
            raw_bytes: None,
        };
        let claim = objective_core::types::ExtractedClaim {
            claim_text: "Apple Inc announced expansion".to_string(),
            subject_name: "Apple Inc".to_string(),
            predicate: "announced".to_string(),
            object_name: None,
            object_value: None,
            claim_type: ClaimType::Relation,
            sentiment: None,
            confidence: 0.6,
            evidence_snippet: "Apple Inc announced expansion".to_string(),
            attributed_to: None,
        };

        let first_claim = first_claim_from(&doc, &claim, 3);
        assert_eq!(first_claim.claim_id, format!("{}#3", doc.id));
        assert_eq!(first_claim.subject_name, "Apple Inc");
        assert_eq!(
            first_claim.document_id.as_deref(),
            Some(doc.id.to_string().as_str())
        );
    }
}
