use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Narrative {
    pub id: String,
    pub title: String,
    pub description: String,
    pub event_count: usize,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NarrativesResponse {
    pub narratives: Vec<Narrative>,
    pub total: usize,
}

#[utoipa::path(
    get,
    path = "/api/v1/narratives",
    responses((status = 200, description = "List narratives (aggregated story threads)", body = NarrativesResponse))
)]
pub async fn list_narratives(State(_state): State<ApiState>) -> Json<NarrativesResponse> {
    Json(NarrativesResponse {
        narratives: Vec::new(),
        total: 0,
    })
}
