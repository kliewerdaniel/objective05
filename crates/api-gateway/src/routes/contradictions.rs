use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::routes::auxiliary::{ContradictionRecord, ContradictionStatus};
use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Contradiction {
    pub id: String,
    pub claim_a: String,
    pub claim_b: String,
    pub entity_name: String,
    pub confidence: f32,
    pub severity: f32,
    pub status: ContradictionStatus,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolution_note: Option<String>,
}

impl From<ContradictionRecord> for Contradiction {
    fn from(record: ContradictionRecord) -> Self {
        Self {
            id: record.id,
            claim_a: record.claim_a,
            claim_b: record.claim_b,
            entity_name: record.entity_name,
            confidence: record.confidence,
            severity: record.severity,
            status: record.status,
            detected_at: record.detected_at,
            resolved_at: record.resolved_at,
            resolution_note: record.resolution_note,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContradictionsResponse {
    pub contradictions: Vec<Contradiction>,
    pub total: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContradictionDetailResponse {
    pub contradiction: Contradiction,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContradictionError {
    pub error: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ContradictionResolveRequest {
    /// Optional resolution status. Defaults to "resolved".
    pub status: Option<ContradictionStatus>,
    /// Optional free-text note describing the resolution.
    pub note: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/contradictions",
    responses((status = 200, description = "List detected contradictions between claims", body = ContradictionsResponse))
)]
pub async fn list_contradictions(State(state): State<ApiState>) -> Json<ContradictionsResponse> {
    let records = state.auxiliary.contradictions.list().await;
    let total = records.len();
    let contradictions: Vec<Contradiction> = records.into_iter().map(Into::into).collect();
    Json(ContradictionsResponse {
        contradictions,
        total,
    })
}

#[utoipa::path(
    get,
    path = "/api/v1/contradictions/{id}",
    params(("id" = String, Path, description = "Contradiction ID")),
    responses(
        (status = 200, description = "Contradiction detail", body = ContradictionDetailResponse),
        (status = 404, description = "Contradiction not found", body = ContradictionError)
    )
)]
pub async fn get_contradiction(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ContradictionDetailResponse>, (StatusCode, Json<ContradictionError>)> {
    match state.auxiliary.contradictions.get(&id).await {
        Some(record) => Ok(Json(ContradictionDetailResponse {
            contradiction: record.into(),
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ContradictionError {
                error: format!("contradiction not found: {id}"),
            }),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/contradictions/{id}/resolve",
    params(("id" = String, Path, description = "Contradiction ID")),
    request_body = ContradictionResolveRequest,
    responses(
        (status = 200, description = "Contradiction resolved", body = ContradictionDetailResponse),
        (status = 404, description = "Contradiction not found", body = ContradictionError)
    )
)]
pub async fn resolve_contradiction(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(request): Json<ContradictionResolveRequest>,
) -> Result<Json<ContradictionDetailResponse>, (StatusCode, Json<ContradictionError>)> {
    let status = request.status.unwrap_or(ContradictionStatus::Resolved);
    match state
        .auxiliary
        .contradictions
        .resolve(&id, status, request.note)
        .await
    {
        Some(record) => Ok(Json(ContradictionDetailResponse {
            contradiction: record.into(),
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ContradictionError {
                error: format!("contradiction not found: {id}"),
            }),
        )),
    }
}
