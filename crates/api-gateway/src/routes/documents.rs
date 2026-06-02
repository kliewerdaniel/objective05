use axum::{extract::State, Json};
use objective_core::types::RawDocument;
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentsResponse {
    pub documents: Vec<RawDocument>,
}

#[utoipa::path(
    get,
    path = "/api/v1/documents",
    responses((status = 200, description = "List normalized documents", body = DocumentsResponse))
)]
pub async fn list_documents(State(state): State<ApiState>) -> Json<DocumentsResponse> {
    let documents = state.store.list_documents().await.unwrap_or_default();

    Json(DocumentsResponse { documents })
}
