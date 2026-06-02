use std::sync::Arc;

use axum::{routing::get, Router};
use objective_core::traits::{DocumentRepository, ExtractionRepository};
use objective_message_bus::InMemoryMessageBus;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::routes::{documents, events, extractions, health, stats};

pub trait ApiRepository: DocumentRepository + ExtractionRepository {}

impl<T> ApiRepository for T where T: DocumentRepository + ExtractionRepository {}

#[derive(Clone)]
pub struct ApiState {
    pub store: Arc<dyn ApiRepository>,
    pub bus: Arc<InMemoryMessageBus>,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl ApiState {
    pub fn new(store: Arc<dyn ApiRepository>, bus: Arc<InMemoryMessageBus>) -> Self {
        Self {
            store,
            bus,
            started_at: chrono::Utc::now(),
        }
    }
}

pub fn build_router(state: ApiState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health::get_health))
        .route("/api/v1/stats", get(stats::get_stats))
        .route("/api/v1/documents", get(documents::list_documents))
        .route("/api/v1/extractions", get(extractions::list_extractions))
        .route("/api/v1/events", get(events::list_events))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
