use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct ExportResponse {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub document_count: usize,
    pub extraction_count: usize,
    pub entity_count: usize,
    pub claim_count: usize,
    pub relationship_count: usize,
    pub documents: Vec<serde_json::Value>,
    pub entities: Vec<ExtractedEntitySummary>,
    pub claims: Vec<ExtractedClaimSummary>,
    pub relationships: Vec<ExtractedRelationshipSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtractedEntitySummary {
    pub name: String,
    pub entity_type: String,
    pub document_count: usize,
    pub confidence: f32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtractedClaimSummary {
    pub document_id: String,
    pub subject_name: String,
    pub predicate: String,
    pub object_name: Option<String>,
    pub claim_text: String,
    pub confidence: f32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtractedRelationshipSummary {
    pub document_id: String,
    pub from_entity_name: String,
    pub to_entity_name: String,
    pub relationship_type: String,
    pub confidence: f32,
}

#[utoipa::path(
    get,
    path = "/api/v1/export",
    responses((status = 200, description = "Full export of the running dataset as JSON", body = ExportResponse))
)]
pub async fn export_data(State(state): State<ApiState>) -> Response {
    let documents = state.store.list_documents().await.unwrap_or_default();
    let extractions = state.store.list_extractions().await.unwrap_or_default();

    let document_count = documents.len();
    let extraction_count = extractions.len();

    let mut entity_doc_counts: BTreeMap<String, (String, f32, usize)> = BTreeMap::new();
    let mut claim_count = 0usize;
    let mut relationship_count = 0usize;
    let mut claims: Vec<ExtractedClaimSummary> = Vec::new();
    let mut relationships: Vec<ExtractedRelationshipSummary> = Vec::new();

    for extraction in &extractions {
        for entity in &extraction.entities {
            let entry = entity_doc_counts
                .entry(entity.name.clone())
                .or_insert_with(|| (format!("{:?}", entity.entity_type), 0.0, 0));
            entry.1 += entity.confidence;
            entry.2 += 1;
        }
        for claim in &extraction.claims {
            claim_count += 1;
            claims.push(ExtractedClaimSummary {
                document_id: extraction.document_id.clone(),
                subject_name: claim.subject_name.clone(),
                predicate: claim.predicate.clone(),
                object_name: claim.object_name.clone(),
                claim_text: claim.claim_text.clone(),
                confidence: claim.confidence,
            });
        }
        for rel in &extraction.relationships {
            relationship_count += 1;
            relationships.push(ExtractedRelationshipSummary {
                document_id: extraction.document_id.clone(),
                from_entity_name: rel.from_entity_name.clone(),
                to_entity_name: rel.to_entity_name.clone(),
                relationship_type: rel.relationship_type.clone(),
                confidence: rel.confidence,
            });
        }
    }

    let entity_count = entity_doc_counts.len();
    let entities: Vec<ExtractedEntitySummary> = entity_doc_counts
        .into_iter()
        .map(|(name, (entity_type, sum, count))| ExtractedEntitySummary {
            name,
            entity_type,
            document_count: count,
            confidence: if count > 0 { sum / count as f32 } else { 0.0 },
        })
        .collect();

    let body = ExportResponse {
        generated_at: chrono::Utc::now(),
        document_count,
        extraction_count,
        entity_count,
        claim_count,
        relationship_count,
        documents: documents
            .into_iter()
            .map(|d| serde_json::to_value(d).unwrap_or_default())
            .collect(),
        entities,
        claims,
        relationships,
    };

    let payload = Json(body);
    let mut response = (StatusCode::OK, payload).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/json; charset=utf-8".parse().unwrap(),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=objective-export.json"
            .parse()
            .unwrap(),
    );
    response
}

use std::collections::BTreeMap;
