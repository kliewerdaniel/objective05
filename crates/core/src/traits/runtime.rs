//! Model runtime trait surface for the Objective platform.
//!
//! The trait abstracts over the inference substrate so extraction,
//! correlation, broadcast, and contradiction can all ask for an
//! inference call without depending on llama.cpp, ONNX, or any
//! specific runtime. The v1 ships a `NoopRuntime` provider (see
//! `crates/model-runtime`) so the trait is always satisfied; Phase
//! 2 will add the `onnx` feature and Phase 3 the `llama` feature
//! per the design in `docs/processing/model-runtime.md` and
//! ADR-015.
//!
//! Every call returns an [`InferenceResult`] that the caller can
//! either accept or fall back from. The trait itself never blocks
//! the caller on a hard timeout — implementers are expected to
//! honour the per-task `timeout` and callers are still expected to
//! wrap the call in a `tokio::time::timeout` for hard upper bounds.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use utoipa::ToSchema;

/// Stable identifier for a known model slot.
///
/// The runtime does not own the actual model file lookup; the
/// integration crate maps a `ModelId` onto a concrete llama.cpp
/// or ONNX session. The `Custom` variant is the escape hatch for
/// user-registered backends (e.g. a plugin-provided runtime).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModelId {
    Mistral7BInstruct,
    Mixtral8x7BInstruct,
    BgeSmallEnV15,
    Custom(String),
}

impl ModelId {
    pub fn as_str(&self) -> &str {
        match self {
            ModelId::Mistral7BInstruct => "mistral-7b-instruct",
            ModelId::Mixtral8x7BInstruct => "mixtral-8x7b-instruct",
            ModelId::BgeSmallEnV15 => "bge-small-en-v1.5",
            ModelId::Custom(name) => name.as_str(),
        }
    }
}

/// The kind of inference the caller is asking for.
///
/// New variants are non-breaking; an older runtime returns
/// [`ModelError::UnsupportedKind`] when it sees a kind it does
/// not recognise, which the orchestrator translates into a
/// per-chunk fallback to the heuristic provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceKind {
    NamedEntityRecognition,
    ClaimExtraction,
    RelationExtraction,
    Embedding,
    TitleGeneration,
    SummaryGeneration,
    ContradictionEvaluation,
    ReportDrafting,
}

impl InferenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            InferenceKind::NamedEntityRecognition => "ner",
            InferenceKind::ClaimExtraction => "claim_extraction",
            InferenceKind::RelationExtraction => "relation_extraction",
            InferenceKind::Embedding => "embedding",
            InferenceKind::TitleGeneration => "title_generation",
            InferenceKind::SummaryGeneration => "summary_generation",
            InferenceKind::ContradictionEvaluation => "contradiction_evaluation",
            InferenceKind::ReportDrafting => "report_drafting",
        }
    }
}

impl std::fmt::Display for InferenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-task inference request. The runtime reads `input` and the
/// optional `system_prompt`, then returns a single
/// [`InferenceResult`].
#[derive(Debug, Clone)]
pub struct InferenceTask {
    pub model: ModelId,
    pub kind: InferenceKind,
    pub input: String,
    pub system_prompt: Option<String>,
    pub max_output_tokens: u32,
    pub temperature: f32,
    pub stop: Vec<String>,
    pub timeout: Duration,
}

impl InferenceTask {
    pub fn new(model: ModelId, kind: InferenceKind, input: impl Into<String>) -> Self {
        Self {
            model,
            kind,
            input: input.into(),
            system_prompt: None,
            max_output_tokens: 512,
            temperature: 0.0,
            stop: Vec::new(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }
}

/// Token accounting reported by the runtime. The runtime may
/// return `None` for backends that do not track tokens.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// Successful or partially-successful inference result.
///
/// `text` is always populated when the runtime returns `Ok`.
/// `structured` is `Some` when the runtime was able to parse
/// JSON out of the LLM output (or when the backend is
/// non-generative, e.g. an ONNX embedding). `usage` is best-
/// effort; backends that do not track tokens leave it as
/// `TokenUsage::default()`.
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub text: String,
    pub structured: Option<JsonValue>,
    pub usage: TokenUsage,
    pub model: ModelId,
    pub kind: InferenceKind,
    pub elapsed: Duration,
    pub completed_at: DateTime<Utc>,
}

impl InferenceResult {
    pub fn text_only(
        model: ModelId,
        kind: InferenceKind,
        text: impl Into<String>,
        elapsed: Duration,
    ) -> Self {
        Self {
            text: text.into(),
            structured: None,
            usage: TokenUsage::default(),
            model,
            kind,
            elapsed,
            completed_at: Utc::now(),
        }
    }

    /// Build a result whose `structured` payload is a JSON
    /// `Vec<f32>`. Used by embedding providers so the
    /// orchestrator can decode the sidecar into a `ModelIndex`
    /// without round-tripping through `text`.
    pub fn embedding(
        model: ModelId,
        dimension: u32,
        vector: Vec<f32>,
        elapsed: Duration,
    ) -> Self {
        assert_eq!(
            vector.len() as u32,
            dimension,
            "InferenceResult::embedding called with vector of length {} but dimension {}",
            vector.len(),
            dimension
        );
        let array = JsonValue::Array(
            vector
                .into_iter()
                .map(|value| {
                    JsonValue::Number(
                        serde_json::Number::from_f64(value as f64)
                            .unwrap_or_else(|| serde_json::Number::from(0)),
                    )
                })
                .collect(),
        );
        Self {
            text: String::new(),
            structured: Some(array),
            usage: TokenUsage::default(),
            model,
            kind: InferenceKind::Embedding,
            elapsed,
            completed_at: Utc::now(),
        }
    }
}

/// State of a single model slot within a runtime. Used by the
/// inventory endpoint and the future `/api/v1/model-runtime`
/// health surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelState {
    NotLoaded,
    Loading,
    Ready,
    Busy,
    Error,
    Unloading,
}

/// State of a single model slot within a runtime, enriched
/// with the live saturation counts and the most recent
/// transition timestamp. Surfaced via
/// `/api/v1/model-runtime` so the dashboard can render queue
/// depth and busyness per slot. Phase 4 of the model-runtime
/// design (`docs/processing/model-runtime.md`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case", tag = "phase")]
pub enum ModelSlotState {
    /// The slot is configured but no model file has been
    /// loaded yet (or the runtime has been freshly
    /// constructed).
    NotLoaded,
    /// The runtime is loading the model file from disk.
    Loading,
    /// The slot is idle and ready to accept new inference
    /// tasks.
    Ready,
    /// The slot is processing one or more inference tasks.
    /// `active` is the number of in-flight calls; `queued`
    /// is the number of callers waiting for a permit.
    Busy { active: u32, queued: u32 },
    /// The slot is shutting down and is not accepting new
    /// tasks; in-flight tasks are draining.
    Draining { active: u32 },
    /// The most recent call on this slot failed. The last
    /// error message is surfaced for the dashboard.
    Error { message: String },
    /// The slot is releasing resources and is not accepting
    /// new tasks.
    Unloading,
}

impl ModelSlotState {
    pub fn is_accepting(&self) -> bool {
        matches!(self, ModelSlotState::Ready | ModelSlotState::Busy { .. })
    }
}

/// A single transition of the per-slot state machine. Kept
/// short — the last `N` transitions are surfaced through
/// `/api/v1/model-runtime` so an operator can see how the
/// slot moved from `Ready` -> `Busy` -> `Error` over the
/// last few seconds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct ModelSlotTransition {
    pub from: ModelSlotState,
    pub to: ModelSlotState,
    pub at: DateTime<Utc>,
    pub reason: Option<String>,
}

/// Per-`InferenceKind` latency summary. Stored in the
/// runtime's metrics table; cheap to clone, serialised into
/// the `/api/v1/model-runtime` snapshot and the
/// `/api/v1/monitoring` block.
///
/// The percentiles are computed from a fixed 11-bucket
/// histogram (see [`LATENCY_BUCKETS_MS`]) so the cost of
/// `observe` is O(buckets) and `snapshot` is O(1). This is
/// intentionally approximate; a future phase can swap in a
/// real P-square estimator without changing the wire
/// format.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct LatencyHistogram {
    pub count: u64,
    pub sum_ms: u64,
    pub min_ms: u64,
    pub max_ms: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
    /// Number of calls that timed out (queue timeout or
    /// chunk timeout).
    pub timeouts: u64,
    /// Number of calls that returned a non-timeout error.
    pub errors: u64,
    /// Per-bucket counts. The first bucket is `<=1ms`, the
    /// last is `>30s`. `buckets` has length
    /// `LATENCY_BUCKETS_MS.len() + 1`.
    pub buckets: Vec<u64>,
}

/// Bucket boundaries (milliseconds) for the latency
/// histogram. The last bucket is the `+Inf` overflow
/// bucket — values that exceed the largest boundary. Keep
/// the list sorted.
pub const LATENCY_BUCKETS_MS: &[u64] = &[
    1, 5, 10, 25, 50, 100, 250, 500, 1_000, 5_000, 30_000,
];

fn bucket_index(elapsed_ms: u64) -> usize {
    // Linear scan — there are only ~12 buckets, so this is
    // faster than a binary search in practice.
    for (i, bound) in LATENCY_BUCKETS_MS.iter().enumerate() {
        if elapsed_ms <= *bound {
            return i;
        }
    }
    LATENCY_BUCKETS_MS.len()
}

fn percentile_from_buckets(buckets: &[u64], percentile: f64) -> u64 {
    let total: u64 = buckets.iter().sum();
    if total == 0 {
        return 0;
    }
    // Use the ceil so a single sample at the 50th
    // percentile resolves to itself rather than 0.
    let target = ((total as f64) * percentile).ceil() as u64;
    let mut cumulative = 0u64;
    for (i, count) in buckets.iter().enumerate() {
        cumulative = cumulative.saturating_add(*count);
        if cumulative >= target {
            return LATENCY_BUCKETS_MS
                .get(i)
                .copied()
                .unwrap_or(u64::MAX);
        }
    }
    LATENCY_BUCKETS_MS
        .last()
        .copied()
        .unwrap_or(u64::MAX)
}

impl LatencyHistogram {
    fn ensure_buckets(&mut self) {
        let expected = LATENCY_BUCKETS_MS.len() + 1;
        if self.buckets.len() != expected {
            self.buckets.resize(expected, 0);
        }
    }

    pub fn observe(&mut self, elapsed_ms: u64) {
        self.count = self.count.saturating_add(1);
        self.sum_ms = self.sum_ms.saturating_add(elapsed_ms);
        if self.count == 1 || elapsed_ms < self.min_ms {
            self.min_ms = elapsed_ms;
        }
        if elapsed_ms > self.max_ms {
            self.max_ms = elapsed_ms;
        }
        self.ensure_buckets();
        let idx = bucket_index(elapsed_ms);
        self.buckets[idx] = self.buckets[idx].saturating_add(1);
        self.p50_ms = percentile_from_buckets(&self.buckets, 0.50);
        self.p95_ms = percentile_from_buckets(&self.buckets, 0.95);
        self.p99_ms = percentile_from_buckets(&self.buckets, 0.99);
    }

    pub fn observe_timeout(&mut self) {
        self.timeouts = self.timeouts.saturating_add(1);
    }

    pub fn observe_error(&mut self) {
        self.errors = self.errors.saturating_add(1);
    }
}

/// Per-runtime aggregate metrics. Returned by
/// `ModelRuntime::metrics()` (Phase 4) and surfaced through
/// `/api/v1/model-runtime` and `/api/v1/monitoring`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ModelRuntimeMetrics {
    /// Total number of calls observed since the runtime was
    /// constructed (or last reset by `reload`).
    pub total_calls: u64,
    /// Total number of fallbacks (orchestrator decided to
    /// fall back to the heuristic on a per-chunk basis).
    pub total_fallbacks: u64,
    /// Per-`InferenceKind` latency histogram.
    pub by_kind: std::collections::BTreeMap<InferenceKind, LatencyHistogram>,
    /// Per-`ModelId` latency histogram.
    pub by_model: std::collections::BTreeMap<ModelId, LatencyHistogram>,
}

/// Per-slot view that combines the configured model, the
/// live state machine, the queue depth, and the
/// `ModelInfo` snapshot. Surfaced as part of the
/// `/api/v1/model-runtime` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ModelSlotView {
    pub model: ModelId,
    pub path: Option<String>,
    pub state: ModelSlotState,
    pub max_concurrency: u32,
    pub active: u32,
    pub queued: u32,
    pub last_error: Option<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub transitions: Vec<ModelSlotTransition>,
}

/// Snapshot of a single model the runtime knows about. The
/// runtime reports this from [`ModelRuntime::inventory`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: ModelId,
    pub state: ModelState,
    pub path: Option<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

/// Sub-kind of a [`ModelError::Timeout`]. Distinguishes a
/// queue-acquire timeout (the runtime refused to accept the
/// call because the slot was saturated) from a chunk
/// timeout (the inner backend took longer than the
/// configured `chunk_timeout_ms`). Phase 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelTimeoutKind {
    /// The call was rejected because the per-slot
    /// `queue_timeout_ms` elapsed while waiting for a
    /// permit. The orchestrator falls back to the
    /// heuristic.
    Queue,
    /// The inner backend took longer than the configured
    /// `chunk_timeout_ms`. The orchestrator falls back to
    /// the heuristic.
    Chunk,
}

impl ModelTimeoutKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelTimeoutKind::Queue => "queue",
            ModelTimeoutKind::Chunk => "chunk",
        }
    }
}

/// Errors a runtime may surface. The orchestrator maps
/// [`ModelError::Unavailable`] onto the per-chunk fallback path;
/// everything else is logged and the chunk is skipped.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("no model is available for kind {kind}")]
    Unavailable { kind: InferenceKind },

    #[error("runtime does not support kind {kind}")]
    UnsupportedKind { kind: InferenceKind },

    #[error("backend error: {0}")]
    Backend(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    /// Timeout with a sub-kind. Existing callers matching
    /// on `ModelError::Timeout(d)` should be updated to
    /// match the structured `kind` and `elapsed` fields.
    #[error("{kind:?} timeout after {elapsed:?}")]
    Timeout {
        kind: ModelTimeoutKind,
        elapsed: Duration,
    },
}

impl From<ModelError> for crate::ObjectiveError {
    fn from(err: ModelError) -> Self {
        crate::ObjectiveError::ModelRuntime(err.to_string())
    }
}

pub type ModelResult<T> = std::result::Result<T, ModelError>;

/// The runtime abstraction. Every call site that wants
/// inference goes through this trait; downstream crates
/// (extraction, correlation, broadcast) never import
/// `llama-cpp-rs` or `ort` directly.
#[async_trait]
pub trait ModelRuntime: Send + Sync {
    /// Stable identifier for the provider ("local-llama-cpp-onnx",
    /// "noop", "remote-openai-compatible", etc.).
    fn provider(&self) -> &'static str;

    /// Snapshot of every model the runtime currently knows about.
    async fn inventory(&self) -> ModelResult<Vec<ModelInfo>>;

    /// Run a single inference task. Implementations are expected
    /// to honour the per-task `timeout`.
    async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult>;

    /// Optional warmup so the first inference does not pay the
    /// load cost. The default implementation is a no-op.
    async fn warmup(&self, _model: ModelId) -> ModelResult<()> {
        Ok(())
    }

    /// Optional shutdown hook for resource cleanup. The default
    /// implementation is a no-op.
    async fn shutdown(&self) -> ModelResult<()> {
        Ok(())
    }

    /// Snapshot the per-runtime metrics (latency histograms,
    /// per-kind counters, per-model counters). The default
    /// implementation returns an empty `ModelRuntimeMetrics`
    /// so older runtimes do not need to be updated; Phase 4
    /// runtimes override this with real data.
    async fn metrics(&self) -> ModelRuntimeMetrics {
        ModelRuntimeMetrics::default()
    }

    /// Snapshot the per-slot state, queue depth, and last
    /// transition log. The default implementation returns
    /// an empty list so older runtimes do not need to be
    /// updated; Phase 4 runtimes override this.
    async fn slot_views(&self) -> Vec<ModelSlotView> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_tracks_min_max_and_count() {
        let mut h = LatencyHistogram::default();
        h.observe(100);
        h.observe(50);
        h.observe(300);
        assert_eq!(h.count, 3);
        assert_eq!(h.sum_ms, 450);
        assert_eq!(h.min_ms, 50);
        assert_eq!(h.max_ms, 300);
    }

    #[test]
    fn observe_records_bucket_counts() {
        let mut h = LatencyHistogram::default();
        h.observe(1);
        h.observe(10);
        h.observe(2_000);
        h.observe(60_000);
        assert_eq!(h.buckets.len(), LATENCY_BUCKETS_MS.len() + 1);
        let one_ms_idx = LATENCY_BUCKETS_MS.iter().position(|b| *b == 1).unwrap();
        let ten_ms_idx = LATENCY_BUCKETS_MS.iter().position(|b| *b == 10).unwrap();
        let five_s_idx = LATENCY_BUCKETS_MS.iter().position(|b| *b == 5_000).unwrap();
        let overflow = LATENCY_BUCKETS_MS.len();
        assert_eq!(h.buckets[one_ms_idx], 1);
        assert_eq!(h.buckets[ten_ms_idx], 1);
        assert_eq!(h.buckets[five_s_idx], 1);
        assert_eq!(h.buckets[overflow], 1);
    }

    #[test]
    fn percentiles_track_bucket_distribution() {
        let mut h = LatencyHistogram::default();
        // 50 samples at 10ms, 50 at 500ms.
        for _ in 0..50 {
            h.observe(10);
        }
        for _ in 0..50 {
            h.observe(500);
        }
        assert_eq!(h.count, 100);
        // p50 should be in the 10ms bucket.
        let ten_ms_idx = LATENCY_BUCKETS_MS.iter().position(|b| *b == 10).unwrap();
        assert_eq!(h.p50_ms, LATENCY_BUCKETS_MS[ten_ms_idx]);
        // p95 / p99 should be in the 500ms bucket.
        let five_hundred_ms_idx = LATENCY_BUCKETS_MS.iter().position(|b| *b == 500).unwrap();
        assert_eq!(h.p95_ms, LATENCY_BUCKETS_MS[five_hundred_ms_idx]);
        assert_eq!(h.p99_ms, LATENCY_BUCKETS_MS[five_hundred_ms_idx]);
    }

    #[test]
    fn observe_timeout_and_error_increment_separate_counters() {
        let mut h = LatencyHistogram::default();
        h.observe(100);
        h.observe_timeout();
        h.observe_error();
        h.observe_error();
        assert_eq!(h.count, 1);
        assert_eq!(h.timeouts, 1);
        assert_eq!(h.errors, 2);
    }

    #[test]
    fn model_timeout_kind_serialises_to_snake_case() {
        assert_eq!(ModelTimeoutKind::Queue.as_str(), "queue");
        assert_eq!(ModelTimeoutKind::Chunk.as_str(), "chunk");
        let json = serde_json::to_string(&ModelTimeoutKind::Queue).unwrap();
        assert_eq!(json, "\"queue\"");
    }

    #[test]
    fn model_slot_state_serialises_with_phase_tag() {
        let state = ModelSlotState::Busy { active: 2, queued: 1 };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["phase"], "busy");
        assert_eq!(json["active"], 2);
        assert_eq!(json["queued"], 1);
    }

    #[test]
    fn model_slot_view_round_trips_through_json() {
        let view = ModelSlotView {
            model: ModelId::BgeSmallEnV15,
            path: Some("/tmp/bge.onnx".to_string()),
            state: ModelSlotState::Ready,
            max_concurrency: 4,
            active: 0,
            queued: 0,
            last_error: None,
            last_used_at: None,
            transitions: Vec::new(),
        };
        let json = serde_json::to_string(&view).unwrap();
        let parsed: ModelSlotView = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, view.model);
        assert_eq!(parsed.state, view.state);
        assert_eq!(parsed.max_concurrency, view.max_concurrency);
    }
}
