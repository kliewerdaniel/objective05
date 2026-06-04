use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use objective_store::recovery::{RecoveryCheck, RecoveryService, RecoveryState};
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct RecoveryResponse {
    pub state: Option<RecoveryState>,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecoveryCheckResponse {
    pub check: RecoveryCheck,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecoveryErrorBody {
    pub error: String,
}

/// Return the latest recovery state, including the current pipeline
/// status, check counters, and recent history.
#[utoipa::path(
    get,
    path = "/api/v1/recovery",
    responses(
        (status = 200, description = "Recovery service state", body = RecoveryResponse),
        (status = 503, description = "Recovery service not configured", body = RecoveryResponse)
    )
)]
pub async fn get_recovery_state(State(state): State<ApiState>) -> Response {
    match state.recovery.as_ref() {
        Some(recovery) => {
            let snapshot = recovery.get_state().await;
            (
                StatusCode::OK,
                Json(RecoveryResponse {
                    state: Some(snapshot),
                    message: "ok".to_string(),
                }),
            )
                .into_response()
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(RecoveryResponse {
                state: None,
                message: "recovery service not configured".to_string(),
            }),
        )
            .into_response(),
    }
}

/// Force an immediate recovery check and return the result. This is
/// useful for operators and tests to confirm the watcher is alive and
/// to observe the live pipeline status.
#[utoipa::path(
    post,
    path = "/api/v1/recovery/check",
    responses(
        (status = 200, description = "Recovery check result", body = RecoveryCheckResponse),
        (status = 503, description = "Recovery service not configured", body = RecoveryErrorBody)
    )
)]
pub async fn post_recovery_check(State(state): State<ApiState>) -> Response {
    let recovery: Arc<RecoveryService> = match state.recovery.as_ref() {
        Some(recovery) => Arc::clone(recovery),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(RecoveryErrorBody {
                    error: "recovery service not configured".to_string(),
                }),
            )
                .into_response();
        }
    };

    match recovery.force_check().await {
        Ok(check) => (StatusCode::OK, Json(RecoveryCheckResponse { check })).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RecoveryErrorBody {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}
