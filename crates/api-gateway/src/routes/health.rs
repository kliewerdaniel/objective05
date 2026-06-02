use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub services: Vec<ServiceHealth>,
    pub uptime_seconds: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ServiceHealth {
    pub name: String,
    pub status: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    responses((status = 200, description = "System health summary", body = HealthResponse))
)]
pub async fn get_health(State(state): State<ApiState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        uptime_seconds: (chrono::Utc::now() - state.started_at).num_seconds(),
        services: vec![
            ServiceHealth {
                name: "api-gateway".to_string(),
                status: "healthy".to_string(),
            },
            ServiceHealth {
                name: "store".to_string(),
                status: "healthy".to_string(),
            },
        ],
    })
}
