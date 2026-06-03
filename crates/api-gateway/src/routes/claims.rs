use axum::{extract::State, Json};
use objective_core::types::ExtractedClaim;
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct ClaimsResponse {
    pub claims: Vec<ExtractedClaim>,
}

#[utoipa::path(
    get,
    path = "/api/v1/claims",
    responses((status = 200, description = "Claims extracted from all documents", body = ClaimsResponse))
)]
pub async fn list_claims(State(state): State<ApiState>) -> Json<ClaimsResponse> {
    let extractions = state.store.list_extractions().await.unwrap_or_default();
    let claims = extractions
        .into_iter()
        .flat_map(|extraction| extraction.claims)
        .collect();

    Json(ClaimsResponse { claims })
}
