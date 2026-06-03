use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Contradiction {
    pub id: String,
    pub claim_a: String,
    pub claim_b: String,
    pub entity_name: String,
    pub confidence: f32,
    pub status: String,
    pub detected_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContradictionsResponse {
    pub contradictions: Vec<Contradiction>,
    pub total: usize,
}

#[utoipa::path(
    get,
    path = "/api/v1/contradictions",
    responses((status = 200, description = "List detected contradictions between claims", body = ContradictionsResponse))
)]
pub async fn list_contradictions(State(_state): State<ApiState>) -> Json<ContradictionsResponse> {
    Json(ContradictionsResponse {
        contradictions: Vec::new(),
        total: 0,
    })
}
