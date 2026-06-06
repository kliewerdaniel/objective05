//! Model runtime routes.
//!
//! - `GET  /api/v1/model-runtime`         — current strategy + slot snapshot
//! - `POST /api/v1/model-runtime/reload`  — rebuild the inner ONNX/llama.cpp
//!   providers from the captured `LocalModelConfig`
//!
//! Both routes return 503 when no runtime has been attached to the
//! `ApiState`. Phase 3 builds the inner `LocalModelRuntime` once at
//! startup; `POST /reload` rebuilds the captured handle, but the
//! `Arc<dyn ModelRuntime>` cloned into `RuntimeExtractionService`
//! still points at the pre-reload instance. The response surfaces
//! a `note` field so dashboard users know a daemon restart is
//! required to rewire the live processor. Phase 3.5 swaps the
//! `Arc<dyn ModelRuntime>` atomically.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use objective_model_runtime::LocalModelRuntimeView;
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::{ApiState, ModelRuntimeHandle};

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelRuntimeResponse {
    pub provider: String,
    pub embedding_slot: Option<SlotSummary>,
    pub extraction_llm_slot: Option<SlotSummary>,
    pub strategy: Vec<StrategyRow>,
    pub chunk_timeout_ms: u64,
    pub context_window: u32,
    pub max_concurrency: u32,
}

impl From<LocalModelRuntimeView> for ModelRuntimeResponse {
    fn from(view: LocalModelRuntimeView) -> Self {
        Self {
            provider: view.provider,
            embedding_slot: view.embedding_slot.map(SlotSummary::from),
            extraction_llm_slot: view.extraction_llm_slot.map(SlotSummary::from),
            strategy: view
                .strategy
                .into_iter()
                .map(|row| StrategyRow {
                    kind: row.kind,
                    slot: row.slot,
                    fallback: row.fallback,
                })
                .collect(),
            chunk_timeout_ms: view.chunk_timeout_ms,
            context_window: view.context_window,
            max_concurrency: view.max_concurrency,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SlotSummary {
    pub path: String,
    pub path_exists: bool,
}

impl From<objective_model_runtime::local::SlotView> for SlotSummary {
    fn from(view: objective_model_runtime::local::SlotView) -> Self {
        Self {
            path: view.path,
            path_exists: view.path_exists,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StrategyRow {
    pub kind: String,
    pub slot: String,
    pub fallback: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelRuntimeErrorResponse {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelRuntimeReloadResponse {
    pub reloaded: bool,
    pub strategy_entries: u32,
    pub provider: String,
    pub view: ModelRuntimeResponse,
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ModelRuntimeErrorResponse {
            error: "model_runtime_unavailable".to_string(),
            message: "model runtime is not configured for this daemon".to_string(),
        }),
    )
        .into_response()
}

fn handle(state: &ApiState) -> Result<Arc<ModelRuntimeHandle>, Response> {
    match state.model_runtime.as_ref() {
        Some(handle) => Ok(Arc::clone(handle)),
        None => Err(unavailable()),
    }
}

/// `GET /api/v1/model-runtime`
#[utoipa::path(
    get,
    path = "/api/v1/model-runtime",
    tag = "model-runtime",
    responses(
        (status = 200, description = "Runtime snapshot", body = ModelRuntimeResponse),
        (status = 503, description = "Runtime not configured", body = ModelRuntimeErrorResponse),
    )
)]
pub async fn get_model_runtime(State(state): State<ApiState>) -> Response {
    let handle = match handle(&state) {
        Ok(h) => h,
        Err(resp) => return resp,
    };
    let view = handle.snapshot().await;
    Json(ModelRuntimeResponse::from(view)).into_response()
}

/// `POST /api/v1/model-runtime/reload`
///
/// Atomically rebuilds the `LocalModelRuntime` from the
/// captured `LocalModelConfig` and swaps the inner
/// `Arc<dyn ModelRuntime>` the live
/// `RuntimeExtractionService` is reading. The next
/// `process` call picks up the new instance without a
/// daemon restart. Phase 3.5a.
#[utoipa::path(
    post,
    path = "/api/v1/model-runtime/reload",
    tag = "model-runtime",
    responses(
        (status = 200, description = "Reload complete", body = ModelRuntimeReloadResponse),
        (status = 503, description = "Runtime not configured", body = ModelRuntimeErrorResponse),
    )
)]
pub async fn post_model_runtime_reload(State(state): State<ApiState>) -> Response {
    let handle = match handle(&state) {
        Ok(h) => h,
        Err(resp) => return resp,
    };
    let view = handle.reload().await;
    let provider = view.provider.clone();
    let strategy_entries = view.strategy.len() as u32;
    Json(ModelRuntimeReloadResponse {
        reloaded: true,
        strategy_entries,
        provider,
        view: ModelRuntimeResponse::from(view),
    })
    .into_response()
}
