use std::sync::Arc;

use axum::{routing::get, Router};
use objective_core::traits::{DocumentRepository, ExtractionRepository};
use objective_correlation::EventRepository;
use objective_message_bus::InMemoryMessageBus;
use objective_store::monitoring::MonitoringService;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::routes::{
    broadcasts, claims, contradictions, derived_events, docs, documents, entities, events,
    extractions, health, monitoring, narratives, sources, stats,
};

pub trait ApiRepository: DocumentRepository + ExtractionRepository {}

impl<T> ApiRepository for T where T: DocumentRepository + ExtractionRepository {}

/// Container state shared across every route handler in the API gateway.
///
/// The `event_repository` field is optional so the rest of the gateway can
/// be constructed without the correlation crate (e.g. for tests that only
/// exercise ingestion-visible routes).
#[derive(Clone)]
pub struct ApiState {
    pub store: Arc<dyn ApiRepository>,
    pub bus: Arc<InMemoryMessageBus>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub event_repository: Option<Arc<dyn EventRepository>>,
    pub monitoring: Option<Arc<MonitoringService>>,
}

impl ApiState {
    pub fn new(store: Arc<dyn ApiRepository>, bus: Arc<InMemoryMessageBus>) -> Self {
        Self {
            store,
            bus,
            started_at: chrono::Utc::now(),
            event_repository: None,
            monitoring: None,
        }
    }

    pub fn with_started_at(mut self, started_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.started_at = started_at;
        self
    }

    /// Attach an event repository so routes that surface derived events
    /// (the correlation engine output) can read from it.
    pub fn with_event_repository(mut self, repository: Arc<dyn EventRepository>) -> Self {
        self.event_repository = Some(repository);
        self
    }

    pub fn with_monitoring(mut self, monitoring: Arc<MonitoringService>) -> Self {
        self.monitoring = Some(monitoring);
        self
    }
}

pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health::get_health))
        .route("/api/v1/stats", get(stats::get_stats))
        .route("/api/v1/documents", get(documents::list_documents))
        .route("/api/v1/extractions", get(extractions::list_extractions))
        .route("/api/v1/events", get(events::list_events))
        .route(
            "/api/v1/derived-events",
            get(derived_events::list_derived_events),
        )
        .route(
            "/api/v1/derived-events/top",
            get(derived_events::list_top_derived_events),
        )
        .route(
            "/api/v1/derived-events/:id",
            get(derived_events::get_derived_event),
        )
        .route("/api/v1/entities", get(entities::list_entities))
        .route(
            "/api/v1/entities/summary",
            get(entities::list_entity_summary),
        )
        .route("/api/v1/entities/:name", get(entities::get_entity))
        .route("/api/v1/claims", get(claims::list_claims))
        .route("/api/v1/sources", get(sources::list_sources))
        .route("/api/v1/narratives", get(narratives::list_narratives))
        .route(
            "/api/v1/contradictions",
            get(contradictions::list_contradictions),
        )
        .route("/api/v1/broadcasts", get(broadcasts::list_broadcasts))
        .route("/api/v1/monitoring", get(monitoring::get_metrics))
        .route("/api-docs/openapi.json", get(docs::get_openapi_spec))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
