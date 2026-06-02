use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use objective_api_gateway::{build_router, ApiState};
use objective_core::traits::{DocumentProcessor, DocumentRepository, ExtractionRepository};
use objective_extraction::HeuristicExtractionService;
use objective_ingestion::{adapters::StaticSourceAdapter, IngestionService};
use objective_message_bus::InMemoryMessageBus;
use objective_store::InMemoryStore;
use serde_json::Value;
use tower::ServiceExt;

async fn seeded_app() -> axum::Router {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));
    let adapter = StaticSourceAdapter::from_plain_text(
        "api_fixture",
        "Austin Manufacturing Update",
        "Apple Inc announced a 10% manufacturing expansion in Austin with public details from local officials.",
    );
    ingestion.poll_source(&adapter).await.unwrap();

    let processor = HeuristicExtractionService;
    for document in store.list_documents().await.unwrap() {
        store
            .save_extraction(processor.process(&document).await.unwrap())
            .await
            .unwrap();
    }

    build_router(ApiState::new(store, bus))
}

async fn get_json(app: axum::Router, uri: &str) -> Value {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_stats_reports_documents_extractions_and_events() {
    let body = get_json(seeded_app().await, "/api/v1/stats").await;

    assert_eq!(body["documents"], 1);
    assert_eq!(body["extractions"], 1);
    assert_eq!(body["events"], 1);
}

#[tokio::test]
async fn test_documents_route_lists_normalized_documents() {
    let body = get_json(seeded_app().await, "/api/v1/documents").await;

    assert_eq!(body["documents"].as_array().unwrap().len(), 1);
    assert_eq!(body["documents"][0]["source_id"], "api_fixture");
}

#[tokio::test]
async fn test_extractions_route_lists_extraction_results() {
    let body = get_json(seeded_app().await, "/api/v1/extractions").await;

    assert_eq!(body["extractions"].as_array().unwrap().len(), 1);
    assert!(body["extractions"][0]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entity| entity["name"] == "Apple Inc"));
}

#[tokio::test]
async fn test_events_route_lists_internal_events() {
    let body = get_json(seeded_app().await, "/api/v1/events").await;

    assert_eq!(body["events"].as_array().unwrap().len(), 1);
    assert_eq!(body["events"][0]["subject"], "ingestion.document.received");
    assert_eq!(
        body["events"][0]["event"]["event_type"],
        "ingestion.document.received"
    );
}
