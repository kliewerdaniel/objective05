use axum::{extract::State, http::StatusCode, Json};
use objective_store::monitoring::PipelineMetrics;
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct MonitoringResponse {
    pub metrics: Option<PipelineMetrics>,
    pub message: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/monitoring",
    responses((status = 200, description = "Pipeline monitoring metrics", body = MonitoringResponse))
)]
pub async fn get_metrics(State(state): State<ApiState>) -> (StatusCode, Json<MonitoringResponse>) {
    match &state.monitoring {
        Some(monitoring) => {
            let metrics = monitoring.persist_and_get();
            (
                StatusCode::OK,
                Json(MonitoringResponse {
                    metrics: Some(metrics),
                    message: "ok".to_string(),
                }),
            )
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(MonitoringResponse {
                metrics: None,
                message: "monitoring service not configured".to_string(),
            }),
        ),
    }
}
