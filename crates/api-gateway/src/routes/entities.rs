use std::collections::BTreeMap;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use objective_core::types::{EntityType, ExtractedClaim, ExtractedEntity};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct EntitiesResponse {
    pub entities: Vec<ExtractedEntity>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EntitySummary {
    pub name: String,
    pub entity_type: EntityType,
    pub document_count: usize,
    pub confidence: f32,
    pub evidence_snippet: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EntitySummaryResponse {
    pub entities: Vec<EntitySummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EntityDetailResponse {
    pub entity: ExtractedEntity,
    pub claims: Vec<ExtractedClaim>,
    pub document_count: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EntityError {
    pub error: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/entities",
    responses((status = 200, description = "Entities aggregated across all extractions", body = EntitiesResponse))
)]
pub async fn list_entities(State(state): State<ApiState>) -> Json<EntitiesResponse> {
    let extractions = state.store.list_extractions().await.unwrap_or_default();
    let mut dedup: BTreeMap<String, ExtractedEntity> = BTreeMap::new();
    for extraction in extractions {
        for entity in extraction.entities {
            dedup.entry(entity.name.clone()).or_insert(entity);
        }
    }

    Json(EntitiesResponse {
        entities: dedup.into_values().collect(),
    })
}

#[utoipa::path(
    get,
    path = "/api/v1/entities/summary",
    responses((status = 200, description = "Entities aggregated with document counts and average confidence", body = EntitySummaryResponse))
)]
pub async fn list_entity_summary(State(state): State<ApiState>) -> Json<EntitySummaryResponse> {
    let extractions = state.store.list_extractions().await.unwrap_or_default();
    let mut by_name: BTreeMap<String, EntityAggregate> = BTreeMap::new();

    for extraction in extractions {
        for entity in extraction.entities {
            let entry = by_name
                .entry(entity.name.clone())
                .or_insert_with(|| EntityAggregate {
                    entity_type: entity.entity_type.clone(),
                    confidence_sum: 0.0,
                    confidence_count: 0,
                    document_count: 0,
                    evidence_snippet: entity.evidence_snippet.clone(),
                });
            entry.confidence_sum += entity.confidence;
            entry.confidence_count += 1;
            entry.document_count += 1;
            if entity.evidence_snippet.len() > entry.evidence_snippet.len() {
                entry.evidence_snippet = entity.evidence_snippet.clone();
            }
        }
    }

    let mut summaries: Vec<EntitySummary> = by_name
        .into_iter()
        .map(|(name, aggregate)| EntitySummary {
            name,
            entity_type: aggregate.entity_type,
            document_count: aggregate.document_count,
            confidence: if aggregate.confidence_count > 0 {
                aggregate.confidence_sum / aggregate.confidence_count as f32
            } else {
                0.0
            },
            evidence_snippet: aggregate.evidence_snippet,
        })
        .collect();
    summaries.sort_by(|left, right| {
        right
            .document_count
            .cmp(&left.document_count)
            .then_with(|| left.name.cmp(&right.name))
    });

    Json(EntitySummaryResponse {
        entities: summaries,
    })
}

#[utoipa::path(
    get,
    path = "/api/v1/entities/{name}",
    params(
        ("name" = String, Path, description = "Entity name")
    ),
    responses(
        (status = 200, description = "Entity detail with associated claims", body = EntityDetailResponse),
        (status = 404, description = "Entity not found", body = EntityError),
    )
)]
pub async fn get_entity(
    State(state): State<ApiState>,
    Path(name): Path<String>,
) -> Result<Json<EntityDetailResponse>, (StatusCode, Json<EntityError>)> {
    let extractions = state.store.list_extractions().await.unwrap_or_default();

    let mut found_entity: Option<ExtractedEntity> = None;
    let mut associated_claims: Vec<ExtractedClaim> = Vec::new();
    let mut document_count = 0;

    for extraction in &extractions {
        for entity in &extraction.entities {
            if entity.name == name {
                if found_entity.is_none() {
                    found_entity = Some(entity.clone());
                }
                document_count += 1;
            }
        }
        for claim in &extraction.claims {
            if claim.subject_name == name
                || claim
                    .object_name
                    .as_deref()
                    .map(|o| o == name)
                    .unwrap_or(false)
            {
                associated_claims.push(claim.clone());
            }
        }
    }

    match found_entity {
        Some(entity) => Ok(Json(EntityDetailResponse {
            entity,
            claims: associated_claims,
            document_count,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(EntityError {
                error: format!("entity not found: {name}"),
            }),
        )),
    }
}

#[derive(Debug)]
struct EntityAggregate {
    entity_type: EntityType,
    confidence_sum: f32,
    confidence_count: usize,
    document_count: usize,
    evidence_snippet: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EntityMergeRequest {
    /// Source entity name to merge from. Will be removed.
    pub source: String,
    /// Target entity name to merge into. Will be preserved.
    pub target: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EntityMergeResponse {
    pub source: String,
    pub target: String,
    pub entities_merged: usize,
    pub claims_rewritten: usize,
    pub relationships_rewritten: usize,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/entities/merge",
    request_body = EntityMergeRequest,
    responses(
        (status = 200, description = "Source entity merged into target", body = EntityMergeResponse),
        (status = 400, description = "Invalid merge request", body = EntityError),
        (status = 404, description = "Source entity not found", body = EntityError)
    )
)]
pub async fn merge_entities(
    State(state): State<ApiState>,
    Json(request): Json<EntityMergeRequest>,
) -> Result<Json<EntityMergeResponse>, (StatusCode, Json<EntityError>)> {
    let source = request.source.trim().to_string();
    let target = request.target.trim().to_string();

    if source.is_empty() || target.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(EntityError {
                error: "source and target are required".to_string(),
            }),
        ));
    }
    if source == target {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(EntityError {
                error: "source and target must be different".to_string(),
            }),
        ));
    }

    let extractions = state.store.list_extractions().await.unwrap_or_default();

    let mut source_present = false;
    for extraction in &extractions {
        for entity in &extraction.entities {
            if entity.name == source {
                source_present = true;
                break;
            }
        }
        if source_present {
            break;
        }
    }
    if !source_present {
        return Err((
            StatusCode::NOT_FOUND,
            Json(EntityError {
                error: format!("source entity not found: {source}"),
            }),
        ));
    }

    let mut entities_merged = 0usize;
    let mut claims_rewritten = 0usize;
    let mut relationships_rewritten = 0usize;
    let mut updated_extractions = Vec::with_capacity(extractions.len());

    for mut extraction in extractions {
        extraction.entities.retain(|entity| {
            if entity.name == source {
                entities_merged += 1;
                false
            } else {
                true
            }
        });
        for claim in extraction.claims.iter_mut() {
            if claim.subject_name == source {
                claim.subject_name = target.clone();
                claims_rewritten += 1;
            }
            if claim.object_name.as_deref() == Some(source.as_str()) {
                claim.object_name = Some(target.clone());
                claims_rewritten += 1;
            }
        }
        for relationship in extraction.relationships.iter_mut() {
            if relationship.from_entity_name == source {
                relationship.from_entity_name = target.clone();
                relationships_rewritten += 1;
            }
            if relationship.to_entity_name == source {
                relationship.to_entity_name = target.clone();
                relationships_rewritten += 1;
            }
        }
        updated_extractions.push(extraction);
    }

    if let Err(error) = state.store.replace_extractions(updated_extractions).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EntityError {
                error: format!("failed to persist merge: {error}"),
            }),
        ));
    }

    Ok(Json(EntityMergeResponse {
        source,
        target,
        entities_merged,
        claims_rewritten,
        relationships_rewritten,
        message: "merge complete".to_string(),
    }))
}
