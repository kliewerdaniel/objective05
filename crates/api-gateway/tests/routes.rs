use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use objective_api_gateway::{build_router, ApiState};
use objective_core::traits::{DocumentProcessor, DocumentRepository, ExtractionRepository};
use objective_core::types::FirstClaim;
use objective_correlation::{EventEngine, InMemoryEventRepository};
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

async fn seeded_app_with_event_engine() -> (axum::Router, Arc<InMemoryEventRepository>) {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));
    let adapter = StaticSourceAdapter::from_plain_text(
        "api_fixture",
        "Austin Manufacturing Update",
        "Apple Inc announced a 10% manufacturing expansion in Austin with public details from local officials. Analysts reported Apple Inc hired workers for the new facility.",
    );
    ingestion.poll_source(&adapter).await.unwrap();

    let processor = HeuristicExtractionService;
    let mut documents = store.list_documents().await.unwrap();
    let document = documents.pop().expect("seeded document");
    let extraction = processor.process(&document).await.unwrap();
    store.save_extraction(extraction.clone()).await.unwrap();

    let event_repository = Arc::new(InMemoryEventRepository::new());
    let engine = EventEngine::with_defaults(Arc::clone(&event_repository));
    for (index, claim) in extraction.claims.iter().enumerate() {
        let first_claim = FirstClaim {
            claim_id: format!("{}#{index}", document.id),
            claim_text: claim.claim_text.clone(),
            subject_name: claim.subject_name.clone(),
            object_name: claim.object_name.clone(),
            location: Some("Austin".to_string()),
            published_at: document.published_at,
            confidence: claim.confidence,
            document_id: Some(document.id.to_string()),
        };
        engine.ingest(first_claim).await.unwrap();
    }

    let router =
        build_router(ApiState::new(store, bus).with_event_repository(event_repository.clone()));
    (router, event_repository)
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

async fn get_response_status(app: axum::Router, uri: &str) -> StatusCode {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    response.status()
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

#[tokio::test]
async fn test_entities_route_dedupes_across_extractions() {
    let body = get_json(seeded_app().await, "/api/v1/entities").await;

    let entities = body["entities"].as_array().unwrap();
    assert!(!entities.is_empty());
    assert!(entities.iter().any(|entity| entity["name"] == "Apple Inc"));
    // A name should only appear once even if the extraction was processed once.
    let apple = entities
        .iter()
        .filter(|entity| entity["name"] == "Apple Inc")
        .count();
    assert_eq!(apple, 1);
}

#[tokio::test]
async fn test_entity_summary_aggregates_by_name() {
    let body = get_json(seeded_app().await, "/api/v1/entities/summary").await;

    let entries = body["entities"].as_array().unwrap();
    assert!(!entries.is_empty());
    let apple = entries
        .iter()
        .find(|entry| entry["name"] == "Apple Inc")
        .expect("Apple Inc summary");
    assert_eq!(apple["document_count"], 1);
    assert!(apple["confidence"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_claims_route_returns_extracted_claims() {
    let body = get_json(seeded_app().await, "/api/v1/claims").await;

    let claims = body["claims"].as_array().unwrap();
    assert!(!claims.is_empty());
    assert!(claims
        .iter()
        .any(|claim| claim["subject_name"] == "Apple Inc"));
}

#[tokio::test]
async fn test_openapi_spec_includes_all_documented_paths() {
    let body = get_json(seeded_app().await, "/api-docs/openapi.json").await;

    let paths = body["paths"].as_object().expect("paths object");
    for path in [
        "/api/v1/health",
        "/api/v1/stats",
        "/api/v1/documents",
        "/api/v1/extractions",
        "/api/v1/events",
        "/api/v1/derived-events",
        "/api/v1/derived-events/top",
        "/api/v1/entities",
        "/api/v1/entities/summary",
        "/api/v1/claims",
    ] {
        assert!(
            paths.contains_key(path),
            "missing path {path} in OpenAPI spec"
        );
    }

    let schemas = body["components"]["schemas"].as_object().expect("schemas");
    for schema in [
        "HealthResponse",
        "StatsResponse",
        "DocumentsResponse",
        "ExtractionsResponse",
        "EventsResponse",
        "DerivedEventsResponse",
        "DerivedEventsQuery",
        "DerivedEventError",
        "EntitiesResponse",
        "EntitySummary",
        "EntitySummaryResponse",
        "ClaimsResponse",
        "RawDocument",
        "ExtractionResult",
        "Event",
        "EventStatus",
        "EventType",
    ] {
        assert!(
            schemas.contains_key(schema),
            "missing schema {schema} in OpenAPI spec"
        );
    }
}

#[tokio::test]
async fn test_derived_events_route_returns_seeded_events() {
    let (app, _repository) = seeded_app_with_event_engine().await;
    let body = get_json(app, "/api/v1/derived-events").await;

    let events = body["events"].as_array().expect("events array");
    assert!(!events.is_empty(), "expected at least one derived event");
    let first = &events[0];
    assert!(first["title"].is_string());
    assert!(first["event_type"].is_string());
    assert!(first["claim_count"].as_u64().unwrap() >= 1);
    assert!(first["participating_entities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entity| entity == "Apple Inc"));
    assert_eq!(body["count"].as_u64().unwrap(), events.len() as u64);
}

#[tokio::test]
async fn test_top_derived_events_route_orders_by_importance() {
    let (app, _repository) = seeded_app_with_event_engine().await;
    let body = get_json(app, "/api/v1/derived-events/top").await;
    let events = body["events"].as_array().expect("events array");
    for window in events.windows(2) {
        assert!(
            window[0]["importance"].as_f64().unwrap() >= window[1]["importance"].as_f64().unwrap(),
            "events should be sorted by importance descending"
        );
    }
}

#[tokio::test]
async fn test_derived_events_route_respects_limit_query() {
    let (app, _repository) = seeded_app_with_event_engine().await;
    let body = get_json(app, "/api/v1/derived-events?limit=1").await;
    assert_eq!(body["events"].as_array().unwrap().len(), 1);
    assert_eq!(body["count"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn test_derived_events_route_returns_503_without_repository() {
    let status = get_response_status(seeded_app().await, "/api/v1/derived-events").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_sources_route_lists_sources() {
    let body = get_json(seeded_app().await, "/api/v1/sources").await;

    let sources = body["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["source_id"], "api_fixture");
    assert_eq!(sources[0]["document_count"], 1);
    assert_eq!(body["total_sources"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn test_derived_events_by_id_returns_event() {
    let (app, _repository) = seeded_app_with_event_engine().await;
    let list_body = get_json(app.clone(), "/api/v1/derived-events").await;
    let event_id = list_body["events"][0]["id"].as_str().unwrap();

    let body = get_json(app, &format!("/api/v1/derived-events/{event_id}")).await;
    assert!(body["event"]["id"].is_string());
    assert_eq!(body["event"]["id"].as_str().unwrap(), event_id);
}

#[tokio::test]
async fn test_derived_events_by_id_returns_404_for_unknown() {
    let (app, _repository) = seeded_app_with_event_engine().await;
    let status = get_response_status(app, "/api/v1/derived-events/nonexistent").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_entity_by_name_returns_detail() {
    let body = get_json(seeded_app().await, "/api/v1/entities/Apple%20Inc").await;

    assert_eq!(body["entity"]["name"], "Apple Inc");
    assert!(body["claims"].as_array().unwrap().len() >= 1);
    assert_eq!(body["document_count"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn test_entity_by_name_returns_404_for_unknown() {
    let status = get_response_status(seeded_app().await, "/api/v1/entities/UnknownEntity").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_narratives_route_returns_empty() {
    let body = get_json(seeded_app().await, "/api/v1/narratives").await;
    assert_eq!(body["total"].as_u64().unwrap(), 0);
    assert!(body["narratives"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_contradictions_route_returns_empty() {
    let body = get_json(seeded_app().await, "/api/v1/contradictions").await;
    assert_eq!(body["total"].as_u64().unwrap(), 0);
    assert!(body["contradictions"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_broadcasts_route_returns_empty() {
    let body = get_json(seeded_app().await, "/api/v1/broadcasts").await;
    assert_eq!(body["total"].as_u64().unwrap(), 0);
    assert!(body["broadcasts"].as_array().unwrap().is_empty());
}
