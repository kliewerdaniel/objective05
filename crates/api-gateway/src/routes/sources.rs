use std::collections::BTreeMap;

use axum::{extract::State, Json};
use serde::Serialize;
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
