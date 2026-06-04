use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use objective_api_gateway::{
    build_router,
    routes::auxiliary::{
        BroadcastRecord, BroadcastStatus, ContradictionRecord, ContradictionStatus,
        NarrativeRecord, NarrativeStatus,
    },
    ApiState,
};
use objective_core::traits::{DocumentProcessor, DocumentRepository, ExtractionRepository};
use objective_core::types::FirstClaim;
use objective_correlation::{EventEngine, InMemoryEventRepository};
use objective_extraction::HeuristicExtractionService;
use objective_ingestion::{adapters::StaticSourceAdapter, IngestionService, SourceRegistry};
use objective_message_bus::InMemoryMessageBus;
use objective_store::{
    monitoring::MonitoringService,
    recovery::{RecoveryConfig, RecoveryService},
    InMemoryStore,
};
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
    assert!(!body["claims"].as_array().unwrap().is_empty());
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

fn build_recovery_app() -> axum::Router {
    let dir = tempfile::tempdir().unwrap().keep();
    let monitoring = Arc::new(MonitoringService::new(&dir));
    let bus = Arc::new(InMemoryMessageBus::new());
    let config = RecoveryConfig::default();
    let recovery = Arc::new(RecoveryService::new(config, monitoring, bus.clone()));
    let store = Arc::new(InMemoryStore::new());
    build_router(ApiState::new(store, bus).with_recovery(recovery))
}

#[tokio::test]
async fn test_recovery_route_returns_state_when_configured() {
    let body = get_json(build_recovery_app(), "/api/v1/recovery").await;
    assert_eq!(body["message"], "ok");
    let state = &body["state"];
    assert_eq!(state["service_name"], "pipeline");
    assert_eq!(state["current_status"], "healthy");
    assert_eq!(state["checks_performed"].as_u64().unwrap(), 0);
    assert!(state["history"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_recovery_check_route_runs_health_check() {
    let app = build_recovery_app();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/recovery/check")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["check"]["status"], "healthy");
    assert!(body["check"]["published_heartbeat"].as_bool().unwrap());
}

#[tokio::test]
async fn test_websocket_returns_503_when_hub_not_configured() {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let app = build_router(ApiState::new(store, bus));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/ws")
                .header("connection", "upgrade")
                .header("upgrade", "websocket")
                .header("sec-websocket-version", "13")
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_websocket_upgrades_and_streams_events() {
    use futures::StreamExt;
    use objective_api_gateway::WebSocketHub;
    use objective_core::traits::MessageBus;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let hub = WebSocketHub::spawn(Arc::clone(&bus));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_router(ApiState::new(store, Arc::clone(&bus)).with_websocket_hub(hub));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("ws://{addr}/ws");
    let (mut client, _response) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    // First frame is the welcome message.
    let welcome = client
        .next()
        .await
        .expect("welcome frame")
        .expect("welcome frame ok");
    if let WsMessage::Text(text) = welcome {
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["type"], "welcome");
    } else {
        panic!("expected text frame for welcome, got {welcome:?}");
    }

    // Publish an event on the bus; the hub should push it back.
    use objective_core::types::EventEnvelope;
    use serde_json::json;
    let _ = bus.clone();
    bus.publish(
        "ingestion.document.received",
        EventEnvelope::new(
            "ingestion.document.received",
            "test.source",
            json!({"document_id": "ws-test-1"}),
        ),
    )
    .await
    .unwrap();

    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), client.next())
        .await
        .expect("event frame in time")
        .expect("event frame")
        .expect("event frame ok");
    if let WsMessage::Text(text) = frame {
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["type"], "event");
        assert_eq!(value["data"]["event_type"], "ingestion.document.received");
    } else {
        panic!("expected text frame for event, got {frame:?}");
    }

    let _ = client.close(None).await;
    server.abort();
}

// ---- Source registry routes ---------------------------------------------

async fn seeded_app_with_registry() -> (axum::Router, Arc<SourceRegistry>) {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let registry = Arc::new(SourceRegistry::ephemeral());
    let router =
        build_router(ApiState::new(store, bus).with_source_registry(Arc::clone(&registry)));
    (router, registry)
}

#[tokio::test]
async fn test_source_registry_returns_503_when_unconfigured() {
    let app = build_router(ApiState::new(
        Arc::new(InMemoryStore::new()),
        Arc::new(InMemoryMessageBus::new()),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/source-registry")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_source_registry_crud_round_trip() {
    let (app, registry) = seeded_app_with_registry().await;

    // POST creates a source.
    let body = serde_json::json!({
        "name": "hackernews_front",
        "source_type": "rss",
        "url": "https://hnrss.org/frontpage",
        "enabled": true,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/source-registry")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["source"]["name"], "hackernews_front");

    // GET list returns the source.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/source-registry")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["total"], 1);
    assert_eq!(payload["sources"][0]["name"], "hackernews_front");

    // GET one returns the source.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/source-registry/hackernews_front")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // PUT patches the URL.
    let patch = serde_json::json!({
        "url": "https://hnrss.org/best",
        "enabled": false,
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/source-registry/hackernews_front")
                .header("content-type", "application/json")
                .body(Body::from(patch.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["source"]["url"], "https://hnrss.org/best");
    assert_eq!(payload["source"]["enabled"], false);

    // Trigger while disabled should 409.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/source-registry/hackernews_front/trigger")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // DELETE removes the source.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/source-registry/hackernews_front")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // GET 404 after delete.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/source-registry/hackernews_front")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    assert_eq!(registry.list().await.len(), 0);
}

#[tokio::test]
async fn test_source_registry_create_rejects_duplicate() {
    let (app, _registry) = seeded_app_with_registry().await;
    let body = serde_json::json!({
        "name": "dup",
        "source_type": "rss",
        "url": "https://example.com/rss",
        "enabled": true,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
    });
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/source-registry")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/source-registry")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_source_registry_unknown_source_returns_404() {
    let (app, _registry) = seeded_app_with_registry().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/source-registry/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---- Narrative / broadcast / contradiction routes --------------------

fn make_narrative(id: &str) -> NarrativeRecord {
    let now = chrono::Utc::now();
    NarrativeRecord {
        id: id.to_string(),
        title: format!("Test narrative {id}"),
        description: "A test narrative".to_string(),
        status: NarrativeStatus::Active,
        event_count: 1,
        claim_ids: vec!["c1".to_string()],
        created_at: now,
        updated_at: now,
    }
}

#[allow(dead_code)]
fn make_broadcast(id: &str) -> BroadcastRecord {
    let now = chrono::Utc::now();
    BroadcastRecord {
        id: id.to_string(),
        title: format!("Test broadcast {id}"),
        summary: "Summary".to_string(),
        body_markdown: "# body".to_string(),
        status: BroadcastStatus::Draft,
        event_count: 0,
        created_at: now,
        updated_at: now,
    }
}

fn make_contradiction(id: &str) -> ContradictionRecord {
    let now = chrono::Utc::now();
    ContradictionRecord {
        id: id.to_string(),
        claim_a: "Apple announced Austin expansion".to_string(),
        claim_b: "Apple cancelled Austin expansion".to_string(),
        entity_name: "Apple Inc".to_string(),
        confidence: 0.7,
        severity: 0.4,
        status: ContradictionStatus::Open,
        detected_at: now,
        resolved_at: None,
        resolution_note: None,
    }
}

#[tokio::test]
async fn test_narrative_get_returns_404_when_missing() {
    let app = build_router(ApiState::new(
        Arc::new(InMemoryStore::new()),
        Arc::new(InMemoryMessageBus::new()),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/narratives/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_narrative_get_returns_record_when_seeded() {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let aux = objective_api_gateway::routes::auxiliary::AuxiliaryStores::new();
    aux.narratives.upsert(make_narrative("N-1")).await;
    let app = build_router(ApiState::new(store, bus).with_auxiliary(aux));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/narratives/N-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["narrative"]["id"], "N-1");
    assert_eq!(payload["narrative"]["status"], "active");
}

#[tokio::test]
async fn test_broadcasts_generate_then_latest() {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let app = build_router(ApiState::new(store, bus));

    // Latest when empty returns 200 with null broadcast.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/broadcasts/latest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(payload["broadcast"].is_null());

    // Generate a broadcast.
    let body = serde_json::json!({"title": "Test Pulse", "focus": "Austin"});
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/broadcasts/generate")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["broadcast"]["title"], "Test Pulse");

    // Latest now returns the broadcast.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/broadcasts/latest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["broadcast"]["title"], "Test Pulse");
}

#[tokio::test]
async fn test_contradiction_resolve_marks_resolved() {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let aux = objective_api_gateway::routes::auxiliary::AuxiliaryStores::new();
    aux.contradictions.insert(make_contradiction("K-1")).await;
    let app = build_router(ApiState::new(store, bus).with_auxiliary(aux));

    // Resolve.
    let body = serde_json::json!({"note": "Same source, different phrasing"});
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/contradictions/K-1/resolve")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["contradiction"]["status"], "resolved");
    assert_eq!(
        payload["contradiction"]["resolution_note"],
        "Same source, different phrasing"
    );

    // GET reflects the resolved state.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/contradictions/K-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["contradiction"]["status"], "resolved");
}

#[tokio::test]
async fn test_contradiction_resolve_unknown_returns_404() {
    let app = build_router(ApiState::new(
        Arc::new(InMemoryStore::new()),
        Arc::new(InMemoryMessageBus::new()),
    ));
    let body = serde_json::json!({});
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/contradictions/missing/resolve")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_entity_merge_combines_claims_and_drops_source() {
    use objective_core::types::{
        ClaimType, ExtractedClaim, ExtractedEntity, ExtractedRelationship, ExtractionResult,
    };
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());

    // Seed an extraction with both source and target entities plus claims
    // and relationships that reference the source name.
    let extraction = ExtractionResult {
        document_id: "doc-1".to_string(),
        entities: vec![
            ExtractedEntity {
                name: "Apple Inc".to_string(),
                entity_type: objective_core::types::EntityType::Organization,
                aliases: vec![],
                description: None,
                metadata: Default::default(),
                confidence: 0.9,
                evidence_snippet: "Apple Inc".to_string(),
            },
            ExtractedEntity {
                name: "Apple Computer".to_string(),
                entity_type: objective_core::types::EntityType::Organization,
                aliases: vec![],
                description: None,
                metadata: Default::default(),
                confidence: 0.8,
                evidence_snippet: "Apple Computer".to_string(),
            },
        ],
        claims: vec![
            ExtractedClaim {
                claim_text: "Apple Inc announced Austin expansion".to_string(),
                subject_name: "Apple Inc".to_string(),
                predicate: "announced".to_string(),
                object_name: Some("Austin".to_string()),
                object_value: None,
                claim_type: ClaimType::Relation,
                sentiment: None,
                confidence: 0.9,
                evidence_snippet: "Apple Inc announced Austin expansion".to_string(),
                attributed_to: None,
            },
            ExtractedClaim {
                claim_text: "Apple Inc builds in Austin".to_string(),
                subject_name: "Apple Computer".to_string(),
                predicate: "builds".to_string(),
                object_name: Some("Apple Inc".to_string()),
                object_value: None,
                claim_type: ClaimType::Relation,
                sentiment: None,
                confidence: 0.8,
                evidence_snippet: "Apple Inc builds in Austin".to_string(),
                attributed_to: None,
            },
        ],
        relationships: vec![ExtractedRelationship {
            from_entity_name: "Apple Inc".to_string(),
            to_entity_name: "Austin".to_string(),
            relationship_type: "located_in".to_string(),
            confidence: 0.7,
            evidence_snippet: "Apple Inc in Austin".to_string(),
        }],
    };
    store.save_extraction(extraction).await.unwrap();

    let app = build_router(ApiState::new(store.clone(), bus));
    let body = serde_json::json!({"source": "Apple Inc", "target": "Apple Computer"});
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/entities/merge")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["entities_merged"], 1);
    assert_eq!(payload["claims_rewritten"], 2);
    assert_eq!(payload["relationships_rewritten"], 1);

    // Verify the source entity is gone and the target is preserved.
    let extractions = store.list_extractions().await.unwrap();
    let mut found_source = 0usize;
    let mut found_target = 0usize;
    for extraction in &extractions {
        for entity in &extraction.entities {
            if entity.name == "Apple Inc" {
                found_source += 1;
            }
            if entity.name == "Apple Computer" {
                found_target += 1;
            }
        }
    }
    assert_eq!(found_source, 0);
    assert_eq!(found_target, 1);
}

#[tokio::test]
async fn test_entity_merge_unknown_source_returns_404() {
    let app = build_router(ApiState::new(
        Arc::new(InMemoryStore::new()),
        Arc::new(InMemoryMessageBus::new()),
    ));
    let body = serde_json::json!({"source": "Nope", "target": "Apple Computer"});
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/entities/merge")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_entity_merge_same_source_target_returns_400() {
    let app = build_router(ApiState::new(
        Arc::new(InMemoryStore::new()),
        Arc::new(InMemoryMessageBus::new()),
    ));
    let body = serde_json::json!({"source": "Apple", "target": "Apple"});
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/entities/merge")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_export_returns_documents_and_entities() {
    let app = seeded_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 32768).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(payload["document_count"].as_u64().unwrap() >= 1);
    assert!(payload["entity_count"].as_u64().unwrap() >= 1);
    assert!(payload["claim_count"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn test_search_finds_matching_document() {
    let app = seeded_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/search?q=Austin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 16384).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["query"], "Austin");
    assert!(payload["total_hits"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn test_config_returns_503_when_unset() {
    let app = build_router(ApiState::new(
        Arc::new(InMemoryStore::new()),
        Arc::new(InMemoryMessageBus::new()),
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_config_returns_payload_when_set() {
    use objective_core::ObjectiveConfig;
    let app = build_router(
        ApiState::new(
            Arc::new(InMemoryStore::new()),
            Arc::new(InMemoryMessageBus::new()),
        )
        .with_config(ObjectiveConfig::default()),
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let payload: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(payload["rest_port"], 8080);
    assert_eq!(payload["log_level"], "info");
}

#[tokio::test]
async fn test_event_resolve_returns_503_when_repository_unset() {
    let app = build_router(ApiState::new(
        Arc::new(InMemoryStore::new()),
        Arc::new(InMemoryMessageBus::new()),
    ));
    let body = serde_json::json!({});
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/events/01HQ1Y2Z3A4B5C6D7E8F9G0H1J/resolve")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
