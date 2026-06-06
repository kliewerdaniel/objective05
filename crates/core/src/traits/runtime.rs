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

/// Stable identifier for a known model slot.
///
/// The runtime does not own the actual model file lookup; the
/// integration crate maps a `ModelId` onto a concrete llama.cpp
/// or ONNX session. The `Custom` variant is the escape hatch for
/// user-registered backends (e.g. a plugin-provided runtime).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    #[error("operation timed out after {0:?}")]
    Timeout(Duration),
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
}
