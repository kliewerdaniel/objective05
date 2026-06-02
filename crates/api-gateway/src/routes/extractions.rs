use axum::{extract::State, Json};
use objective_core::types::ExtractionResult;
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct ExtractionsResponse {
    pub extractions: Vec<ExtractionResult>,
}

#[utoipa::path(
    get,
    path = "/api/v1/extractions",
    responses((status = 200, description = "List extraction results", body = ExtractionsResponse))
)]
pub async fn list_extractions(State(state): State<ApiState>) -> Json<ExtractionsResponse> {
    let extractions = state.store.list_extractions().await.unwrap_or_default();

    Json(ExtractionsResponse { extractions })
}
