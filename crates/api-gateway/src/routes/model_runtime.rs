//! Model runtime routes.
//!
//! - `GET  /api/v1/model-runtime`         — current strategy, slot
//!   snapshot, and per-slot state machine + queue depth
//!   (Phase 4)
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
use objective_core::traits::{
    LatencyHistogram, ModelSlotState, ModelSlotTransition, ModelSlotView, ModelTimeoutKind,
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
    pub queue_timeout_ms: u64,
    pub context_window: u32,
    pub max_concurrency: u32,
    /// Live per-slot views. Phase 4. Each entry pairs a
    /// configured slot with the state-machine snapshot
    /// (`ready` / `busy` / `error` / `draining`), the live
    /// queue depth, and the last few state transitions.
    pub slots: Vec<SlotViewResponse>,
    /// Live per-runtime metrics (latency histograms, per-
    /// kind counters, per-model counters, total timeouts
    /// and fallbacks). Phase 4.
    pub metrics: Option<RuntimeMetricsSummary>,
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
            queue_timeout_ms: view.queue_timeout_ms,
            context_window: view.context_window,
            max_concurrency: view.max_concurrency,
            slots: Vec::new(),
            metrics: None,
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

/// Per-slot snapshot surfaced by the model-runtime route.
/// Wraps the live `ModelSlotView` from the core trait with
/// a stable, documented wire format.
#[derive(Debug, Serialize, ToSchema)]
pub struct SlotViewResponse {
    pub model: String,
    pub path: Option<String>,
    pub state: ModelSlotState,
    pub max_concurrency: u32,
    pub active: u32,
    pub queued: u32,
    pub last_error: Option<String>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub transitions: Vec<ModelSlotTransition>,
}

impl From<ModelSlotView> for SlotViewResponse {
    fn from(view: ModelSlotView) -> Self {
        Self {
            model: view.model.as_str().to_string(),
            path: view.path,
            state: view.state,
            max_concurrency: view.max_concurrency,
            active: view.active,
            queued: view.queued,
            last_error: view.last_error,
            last_used_at: view.last_used_at,
            transitions: view.transitions,
        }
    }
}

/// Per-runtime aggregate metrics. Mirrors
/// `objective_core::traits::ModelRuntimeMetrics` with
/// `serde_json` values for the keyed maps so the API
/// response shape stays stable across runtime versions.
#[derive(Debug, Serialize, ToSchema)]
pub struct RuntimeMetricsSummary {
    pub total_calls: u64,
    pub total_fallbacks: u64,
    pub by_kind: std::collections::BTreeMap<String, LatencyHistogram>,
    pub by_model: std::collections::BTreeMap<String, LatencyHistogram>,
}

impl From<objective_core::traits::ModelRuntimeMetrics> for RuntimeMetricsSummary {
    fn from(m: objective_core::traits::ModelRuntimeMetrics) -> Self {
        let by_kind = m
            .by_kind
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let by_model = m
            .by_model
            .into_iter()
            .map(|(k, v)| (k.as_str().to_string(), v))
            .collect();
        Self {
            total_calls: m.total_calls,
            total_fallbacks: m.total_fallbacks,
            by_kind,
            by_model,
        }
    }
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

fn handle(state: &ApiState) -> Result<Arc<ModelRuntimeHandle>, Box<Response>> {
    match state.model_runtime.as_ref() {
        Some(handle) => Ok(Arc::clone(handle)),
        None => Err(Box::new(unavailable())),
    }
}

/// `GET /api/v1/model-runtime`
#[utoipa::path(
    get,
    path = "/api/v1/model-runtime",
    tag = "model-runtime",
    responses(
        (status = 200, description = "Runtime snapshot (strategy, slot state, queue depth, latency histograms)", body = ModelRuntimeResponse),
        (status = 503, description = "Runtime not configured", body = ModelRuntimeErrorResponse),
    )
)]
pub async fn get_model_runtime(State(state): State<ApiState>) -> Response {
    let handle = match handle(&state) {
        Ok(h) => h,
        Err(resp) => return *resp,
    };
    let view = handle.snapshot().await;
    let slots = handle.slot_views().await;
    let metrics = handle.metrics().await;
    let mut response = ModelRuntimeResponse::from(view);
    response.slots = slots.into_iter().map(SlotViewResponse::from).collect();
    response.metrics = Some(RuntimeMetricsSummary::from(metrics));
    Json(response).into_response()
}

/// `POST /api/v1/model-runtime/reload`
///
/// Atomically rebuilds the `LocalModelRuntime` from the
/// captured `LocalModelConfig` and swaps the inner
/// `Arc<dyn ModelRuntime>` the live
/// `RuntimeExtractionService` is reading. The next
/// `process` call picks up the new instance without a
/// daemon restart. Phase 3.5a. Phase 4 also resets the
/// per-slot state machines and the metrics table.
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
        Err(resp) => return *resp,
    };
    let view = handle.reload().await;
    let slots = handle.slot_views().await;
    let metrics = handle.metrics().await;
    let provider = view.provider.clone();
    let strategy_entries = view.strategy.len() as u32;
    let mut response = ModelRuntimeResponse::from(view);
    response.slots = slots.into_iter().map(SlotViewResponse::from).collect();
    response.metrics = Some(RuntimeMetricsSummary::from(metrics));
    Json(ModelRuntimeReloadResponse {
        reloaded: true,
        strategy_entries,
        provider,
        view: response,
    })
    .into_response()
}

// Re-export the timeout-kind enum so OpenAPI can name it
// in the schemas block.
#[allow(dead_code)]
fn _keep_timeout_kind_in_scope() -> ModelTimeoutKind {
    ModelTimeoutKind::Queue
}
