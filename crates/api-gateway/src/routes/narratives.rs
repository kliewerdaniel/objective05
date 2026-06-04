use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::routes::auxiliary::{NarrativeRecord, NarrativeStatus};
use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Narrative {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: NarrativeStatus,
    pub event_count: usize,
    pub claim_ids: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<NarrativeRecord> for Narrative {
    fn from(record: NarrativeRecord) -> Self {
        Self {
            id: record.id,
            title: record.title,
            description: record.description,
            status: record.status,
            event_count: record.event_count,
            claim_ids: record.claim_ids,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NarrativesResponse {
    pub narratives: Vec<Narrative>,
    pub total: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NarrativeDetailResponse {
    pub narrative: Narrative,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NarrativeError {
    pub error: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/narratives",
    responses((status = 200, description = "List narratives (aggregated story threads)", body = NarrativesResponse))
)]
pub async fn list_narratives(State(state): State<ApiState>) -> Json<NarrativesResponse> {
    let records = state.auxiliary.narratives.list().await;
    let total = records.len();
    let narratives: Vec<Narrative> = records.into_iter().map(Into::into).collect();
    Json(NarrativesResponse { narratives, total })
}

#[utoipa::path(
    get,
    path = "/api/v1/narratives/{id}",
    params(("id" = String, Path, description = "Narrative ID")),
    responses(
        (status = 200, description = "Narrative detail", body = NarrativeDetailResponse),
        (status = 404, description = "Narrative not found", body = NarrativeError)
    )
)]
pub async fn get_narrative(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<NarrativeDetailResponse>, (StatusCode, Json<NarrativeError>)> {
    match state.auxiliary.narratives.get(&id).await {
        Some(record) => Ok(Json(NarrativeDetailResponse {
            narrative: record.into(),
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(NarrativeError {
                error: format!("narrative not found: {id}"),
            }),
        )),
    }
}
