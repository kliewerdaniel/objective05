use axum::{extract::State, Json};
use objective_core::traits::MessageBus;
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct StatsResponse {
    pub documents: usize,
    pub extractions: usize,
    pub events: usize,
}

#[utoipa::path(
    get,
    path = "/api/v1/stats",
    responses((status = 200, description = "System statistics", body = StatsResponse))
)]
pub async fn get_stats(State(state): State<ApiState>) -> Json<StatsResponse> {
    let documents = state
        .store
        .list_documents()
        .await
        .map_or(0, |documents| documents.len());
    let extractions = state
        .store
        .list_extractions()
        .await
        .map_or(0, |extractions| extractions.len());
    let events = state.bus.events().await.map_or(0, |events| events.len());

    Json(StatsResponse {
        documents,
        extractions,
        events,
    })
}
