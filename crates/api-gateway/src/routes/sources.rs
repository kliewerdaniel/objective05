use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use objective_core::traits::{DocumentRepository, MessageBus, SourceAdapter};
use objective_core::types::EventEnvelope;
use objective_core::Result;
use objective_ingestion::{SourceDefinition, SourcePatch, SourceRegistry};
use objective_message_bus::InMemoryMessageBus;
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct SourceInfo {
    pub source_id: String,
    pub source_type: String,
    pub document_count: usize,
    pub extraction_count: usize,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SourcesResponse {
    pub sources: Vec<SourceInfo>,
    pub total_sources: usize,
}

#[derive(Debug)]
struct SourceAggregate {
    source_type: String,
    document_count: usize,
    extraction_count: usize,
    last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[utoipa::path(
    get,
    path = "/api/v1/sources",
    responses((status = 200, description = "List of configured sources with document and extraction counts", body = SourcesResponse))
)]
pub async fn list_sources(State(state): State<ApiState>) -> Json<SourcesResponse> {
    let documents = state.store.list_documents().await.unwrap_or_default();
    let extractions = state.store.list_extractions().await.unwrap_or_default();

    let mut by_id: BTreeMap<String, SourceAggregate> = BTreeMap::new();

    for doc in &documents {
        let entry = by_id
            .entry(doc.source_id.clone())
            .or_insert_with(|| SourceAggregate {
                source_type: doc.source_type.clone(),
                document_count: 0,
                extraction_count: 0,
                last_fetched_at: None,
            });
        entry.document_count += 1;
        let fetched = doc.fetched_at;
        entry.last_fetched_at = Some(
            entry
                .last_fetched_at
                .map_or(fetched, |current| current.max(fetched)),
        );
    }

    for extraction in &extractions {
        if let Some(entry) = by_id.values_mut().find(|v| {
            documents
                .iter()
                .any(|d| d.id.to_string() == extraction.document_id && d.source_id == v.source_type)
        }) {
            entry.extraction_count += 1;
        }
    }

    let total_sources = by_id.len();
    let sources: Vec<SourceInfo> = by_id
        .into_iter()
        .map(|(source_id, aggregate)| SourceInfo {
            source_id,
            source_type: aggregate.source_type,
            document_count: aggregate.document_count,
            extraction_count: aggregate.extraction_count,
            last_fetched_at: aggregate.last_fetched_at,
        })
        .collect();

    Json(SourcesResponse {
        sources,
        total_sources,
    })
}

// ---- Registry endpoints ------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct SourceRegistryListResponse {
    pub sources: Vec<SourceDefinition>,
    pub total: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SourceRegistryEntryResponse {
    pub source: SourceDefinition,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SourceRegistryError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SourceRegistryTriggerResponse {
    pub source_name: String,
    pub documents_ingested: usize,
    pub message: String,
}

fn registry_error_body(error: objective_ingestion::RegistryError) -> SourceRegistryError {
    use objective_ingestion::RegistryError;
    let code = match &error {
        RegistryError::AlreadyExists(_) => Some("conflict".to_string()),
        RegistryError::NotFound(_) => Some("not_found".to_string()),
        RegistryError::Invalid(_) | RegistryError::UnsupportedType(_) => {
            Some("bad_request".to_string())
        }
        RegistryError::Storage(_) => Some("storage".to_string()),
    };
    SourceRegistryError {
        error: error.to_string(),
        code,
    }
}

fn registry_error_response(error: objective_ingestion::RegistryError) -> Response {
    use objective_ingestion::RegistryError;
    let status = match &error {
        RegistryError::AlreadyExists(_) => StatusCode::CONFLICT,
        RegistryError::NotFound(_) => StatusCode::NOT_FOUND,
        RegistryError::Invalid(_) | RegistryError::UnsupportedType(_) => StatusCode::BAD_REQUEST,
        RegistryError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(registry_error_body(error))).into_response()
}

fn registry_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(SourceRegistryError {
            error: "source registry is not configured on this gateway".to_string(),
            code: Some("registry_unavailable".to_string()),
        }),
    )
        .into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/source-registry",
    responses(
        (status = 200, description = "List of registered ingestion sources", body = SourceRegistryListResponse),
        (status = 503, description = "Source registry not configured", body = SourceRegistryError)
    )
)]
pub async fn list_registered_sources(State(state): State<ApiState>) -> Response {
    let Some(registry) = state.source_registry.clone() else {
        return registry_unavailable();
    };
    let sources = registry.list().await;
    let total = sources.len();
    Json(SourceRegistryListResponse { sources, total }).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/source-registry",
    request_body = SourceDefinition,
    responses(
        (status = 201, description = "Source registered", body = SourceRegistryEntryResponse),
        (status = 400, description = "Invalid source", body = SourceRegistryError),
        (status = 409, description = "Source already exists", body = SourceRegistryError),
        (status = 503, description = "Source registry not configured", body = SourceRegistryError)
    )
)]
pub async fn create_source(
    State(state): State<ApiState>,
    Json(source): Json<SourceDefinition>,
) -> Response {
    let Some(registry) = state.source_registry.clone() else {
        return registry_unavailable();
    };
    match registry.add(source).await {
        Ok(stored) => (
            StatusCode::CREATED,
            Json(SourceRegistryEntryResponse {
                source: stored,
                message: "source registered".to_string(),
            }),
        )
            .into_response(),
        Err(error) => registry_error_response(error),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/source-registry/{name}",
    responses(
        (status = 200, description = "Registered source details", body = SourceRegistryEntryResponse),
        (status = 404, description = "Source not found", body = SourceRegistryError),
        (status = 503, description = "Source registry not configured", body = SourceRegistryError)
    )
)]
pub async fn get_registered_source(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Response {
    let Some(registry) = state.source_registry.clone() else {
        return registry_unavailable();
    };
    match registry.get(&name).await {
        Some(source) => Json(SourceRegistryEntryResponse {
            source,
            message: "ok".to_string(),
        })
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(SourceRegistryError {
                error: format!("source not found: {name}"),
                code: Some("not_found".to_string()),
            }),
        )
            .into_response(),
    }
}

#[utoipa::path(
    put,
    path = "/api/v1/source-registry/{name}",
    request_body = SourcePatch,
    responses(
        (status = 200, description = "Source updated", body = SourceRegistryEntryResponse),
        (status = 404, description = "Source not found", body = SourceRegistryError),
        (status = 503, description = "Source registry not configured", body = SourceRegistryError)
    )
)]
pub async fn update_source(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    Json(patch): Json<SourcePatch>,
) -> Response {
    let Some(registry) = state.source_registry.clone() else {
        return registry_unavailable();
    };
    match registry.update(&name, patch).await {
        Ok(updated) => Json(SourceRegistryEntryResponse {
            source: updated,
            message: "source updated".to_string(),
        })
        .into_response(),
        Err(error) => registry_error_response(error),
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/source-registry/{name}",
    responses(
        (status = 200, description = "Source removed", body = SourceRegistryEntryResponse),
        (status = 404, description = "Source not found", body = SourceRegistryError),
        (status = 503, description = "Source registry not configured", body = SourceRegistryError)
    )
)]
pub async fn delete_source(State(state): State<ApiState>, Path(name): Path<String>) -> Response {
    let Some(registry) = state.source_registry.clone() else {
        return registry_unavailable();
    };
    match registry.remove(&name).await {
        Ok(removed) => Json(SourceRegistryEntryResponse {
            source: removed,
            message: "source removed".to_string(),
        })
        .into_response(),
        Err(error) => registry_error_response(error),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/source-registry/{name}/trigger",
    responses(
        (status = 200, description = "Source poll triggered", body = SourceRegistryTriggerResponse),
        (status = 404, description = "Source not found", body = SourceRegistryError),
        (status = 409, description = "Source disabled", body = SourceRegistryError),
        (status = 503, description = "Source registry not configured", body = SourceRegistryError)
    )
)]
pub async fn trigger_source(State(state): State<ApiState>, Path(name): Path<String>) -> Response {
    let (registry, store, bus) = match (
        state.source_registry.clone(),
        state.store.clone(),
        state.bus.clone(),
    ) {
        (Some(registry), store, bus) => (registry, store, bus),
        _ => return registry_unavailable(),
    };
    let source = match registry.get(&name).await {
        Some(source) => source,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(SourceRegistryError {
                    error: format!("source not found: {name}"),
                    code: Some("not_found".to_string()),
                }),
            )
                .into_response();
        }
    };
    if !source.enabled {
        return (
            StatusCode::CONFLICT,
            Json(SourceRegistryError {
                error: format!("source is disabled: {name}"),
                code: Some("disabled".to_string()),
            }),
        )
            .into_response();
    }
    let adapter = match SourceRegistry::spawn_adapter(&source) {
        Ok(adapter) => adapter,
        Err(error) => return registry_error_response(error),
    };
    let adapter: Arc<dyn SourceAdapter> = Arc::from(adapter);
    let count = match poll_adapter(&adapter, store, bus).await {
        Ok(count) => count,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SourceRegistryError {
                    error: format!("poll failed: {error}"),
                    code: Some("poll_failed".to_string()),
                }),
            )
                .into_response();
        }
    };
    Json(SourceRegistryTriggerResponse {
        source_name: name,
        documents_ingested: count,
        message: "ok".to_string(),
    })
    .into_response()
}

async fn poll_adapter(
    adapter: &Arc<dyn SourceAdapter>,
    store: Arc<dyn DocumentRepository>,
    bus: Arc<InMemoryMessageBus>,
) -> Result<usize> {
    adapter.validate()?;
    let result = adapter.poll(None).await?;
    let count = result.documents.len();
    for document in result.documents {
        let data = json!({
            "document_id": document.id.to_string(),
            "source_id": document.source_id,
            "source_type": document.source_type,
            "url": document.url,
            "title": document.title,
            "published_at": document.published_at,
        });
        store.save_document(document).await?;
        bus.publish(
            "ingestion.document.received",
            EventEnvelope::new("ingestion.document.received", adapter.name(), data),
        )
        .await?;
    }
    Ok(count)
}
