use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Broadcast {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub event_count: usize,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BroadcastsResponse {
    pub broadcasts: Vec<Broadcast>,
    pub total: usize,
}

#[utoipa::path(
    get,
    path = "/api/v1/broadcasts",
    responses((status = 200, description = "List recent broadcasts", body = BroadcastsResponse))
)]
pub async fn list_broadcasts(State(_state): State<ApiState>) -> Json<BroadcastsResponse> {
    Json(BroadcastsResponse {
        broadcasts: Vec::new(),
        total: 0,
    })
}
