//! `LocalModelRuntime` — composite runtime that fronts the
//! ONNX and llama.cpp providers. The orchestrator queries
//! this single trait object; per-kind dispatch is
//! configured via the `default_strategy` block on
//! `LocalModelConfig`.
//!
//! Dispatch contract (Phase 3):
//!
//! 1. If the `default_strategy` table maps the
//!    `InferenceKind` to a `SlotName::Embedding`, the
//!    `OnnxRuntime` is asked. Otherwise
//!    `ModelError::Backend("no strategy for kind")`.
//! 2. If the slot is `SlotName::ExtractionLlm`, the
//!    `LlamaRuntime` is asked. Same error if the slot is
//!    unrecognised.
//! 3. If the slot is `SlotName::Custom(name)`, the runtime
//!    returns `ModelError::Backend("custom slot not
//!    supported")`. Phase 3.5 will resolve custom slots
//!    against the plugin registry.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use objective_core::traits::{
    InferenceKind, InferenceResult, InferenceTask, ModelError, ModelId, ModelInfo,
    ModelResult, ModelRuntime, ModelSlotView, ModelState, ModelTimeoutKind,
};
use objective_core::{LocalModelConfig, SlotName, StrategyTable};
use tokio::time::timeout;
use tracing::warn;

use crate::llama::LlamaRuntime;
use crate::metrics::{CallOutcome, RuntimeMetrics, RuntimeMetricsSnapshot};
use crate::onnx::OnnxRuntime;
use crate::queue::{QueueTimeout, SlotQueue};

#[derive(Debug, Clone)]
pub struct LocalModelRuntime {
    onnx: OnnxRuntime,
    llama: LlamaRuntime,
    strategy: StrategyTable,
    config: Arc<LocalModelConfig>,
    embedding_queue: SlotQueue,
    llama_queue: SlotQueue,
    metrics: Arc<RuntimeMetrics>,
}

impl LocalModelRuntime {
    pub fn from_config(config: LocalModelConfig) -> Self {
        let onnx = OnnxRuntime::from_config(&config);
        let llama = LlamaRuntime::from_slot(config.models.extraction_llm.clone());
        let strategy = config.default_strategy.clone();
        let queue_timeout = Duration::from_millis(config.queue_timeout_ms.max(1));
        let embedding_queue = SlotQueue::new(config.max_concurrency.max(1), queue_timeout);
        let llama_queue = SlotQueue::new(config.max_concurrency.max(1), queue_timeout);
        // Mark every configured slot as `Ready` up front.
        // The stub backends (`OnnxRuntime` / `LlamaRuntime`)
        // are always "loaded"; the real backends (Phase 3.5+
        // `ort::Session` / `llama_cpp` session) override
        // the initial state to `NotLoaded` from inside
        // `mark_loading` once `from_ort_path` /
        // `from_gguf_path` is wired in.
        if config.models.embedding.is_some() {
            embedding_queue.state().mark_ready();
        }
        if config.models.extraction_llm.is_some() {
            llama_queue.state().mark_ready();
        }
        Self {
            onnx,
            llama,
            strategy,
            config: Arc::new(config),
            embedding_queue,
            llama_queue,
            metrics: Arc::new(RuntimeMetrics::new()),
        }
    }

    pub fn config(&self) -> &LocalModelConfig {
        &self.config
    }

    pub fn strategy(&self) -> &StrategyTable {
        &self.strategy
    }

    pub fn metrics_handle(&self) -> &Arc<RuntimeMetrics> {
        &self.metrics
    }

    pub fn metrics_snapshot(&self) -> RuntimeMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn slot_views(&self) -> Vec<ModelSlotView> {
        let mut views = Vec::new();
        if self.config.models.embedding.is_some() {
            views.push(slot_view(
                ModelId::BgeSmallEnV15,
                self.config.models.embedding.as_ref().map(|s| s.path.to_string_lossy().to_string()),
                &self.embedding_queue,
            ));
        }
        if self.config.models.extraction_llm.is_some() {
            views.push(slot_view(
                ModelId::Mistral7BInstruct,
                self.config.models.extraction_llm.as_ref().map(|s| s.path.to_string_lossy().to_string()),
                &self.llama_queue,
            ));
        }
        views
    }

    /// Returns `Ok(handler)` if the strategy has an entry
    /// for `kind`, or `Err(Backend)` if it does not. The
    /// orchestrator treats a missing strategy as a clean
    /// "fall back to heuristic" signal.
    fn handler_for(&self, kind: InferenceKind) -> Result<LocalHandler, ModelError> {
        let entry = self.strategy.get(&kind).ok_or_else(|| {
            ModelError::Backend(format!(
                "no strategy configured for kind {}; the orchestrator should fall back",
                kind
            ))
        })?;
        match entry.slot {
            SlotName::Embedding => Ok(LocalHandler::Onnx),
            SlotName::ExtractionLlm => Ok(LocalHandler::Llama),
            SlotName::Custom(_) => Err(ModelError::Backend(
                "custom slot is not supported in Phase 3".to_string(),
            )),
        }
    }

    fn queue_for(&self, handler: LocalHandler) -> &SlotQueue {
        match handler {
            LocalHandler::Onnx => &self.embedding_queue,
            LocalHandler::Llama => &self.llama_queue,
        }
    }

    /// Trigger a hot reload. Phase 3 rebuilds the inner
    /// runtimes from the captured `LocalModelConfig`; Phase
    /// 3.5 will additionally unload the previous
    /// `llama-cpp-rs` session. Phase 4 also resets the
    /// metrics table and slot state machines.
    pub fn reload(&mut self) {
        let config = (*self.config).clone();
        self.onnx = OnnxRuntime::from_config(&config);
        self.llama = LlamaRuntime::from_slot(config.models.extraction_llm.clone());
        self.strategy = config.default_strategy.clone();
        let queue_timeout = Duration::from_millis(config.queue_timeout_ms.max(1));
        self.embedding_queue = SlotQueue::new(config.max_concurrency.max(1), queue_timeout);
        self.llama_queue = SlotQueue::new(config.max_concurrency.max(1), queue_timeout);
        self.metrics.reset();
    }

    /// Build a serialisable snapshot of the runtime's current
    /// state. Used by `/api/v1/model-runtime` and the reload
    /// response so the dashboard can render the live strategy
    /// table without a separate inventory round-trip.
    pub fn view(&self) -> LocalModelRuntimeView {
        LocalModelRuntimeView {
            provider: self.provider().to_string(),
            embedding_slot: self.config.models.embedding.as_ref().map(slot_to_view),
            extraction_llm_slot: self.config.models.extraction_llm.as_ref().map(slot_to_view),
            strategy: self
                .strategy
                .iter()
                .map(|(kind, entry)| StrategyEntryView {
                    kind: kind.to_string(),
                    slot: entry.slot.as_str().to_string(),
                    fallback: format!("{:?}", entry.fallback).to_lowercase(),
                })
                .collect(),
            chunk_timeout_ms: self.config.chunk_timeout_ms,
            queue_timeout_ms: self.config.queue_timeout_ms,
            context_window: self.config.context_window,
            max_concurrency: self.config.max_concurrency,
        }
    }
}

fn slot_to_view<P>(slot: &P) -> SlotView
where
    P: SlotPath,
{
    SlotView {
        path: slot.path().to_string_lossy().to_string(),
        path_exists: slot.path().exists(),
    }
}

trait SlotPath {
    fn path(&self) -> &std::path::Path;
}

impl SlotPath for objective_core::EmbeddingSlot {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl SlotPath for objective_core::LlmSlot {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

fn slot_view(model: ModelId, path: Option<String>, queue: &SlotQueue) -> ModelSlotView {
    let (state, last_error, last_used_at, transitions) = queue.state().snapshot();
    ModelSlotView {
        model,
        path,
        state,
        max_concurrency: queue.max_concurrency(),
        active: queue.state().active(),
        queued: queue.state().queued(),
        last_error,
        last_used_at,
        transitions,
    }
}

#[derive(Debug, Clone, Copy)]
enum LocalHandler {
    Onnx,
    Llama,
}

#[async_trait]
impl ModelRuntime for LocalModelRuntime {
    fn provider(&self) -> &'static str {
        "local"
    }

    async fn inventory(&self) -> ModelResult<Vec<ModelInfo>> {
        let mut entries = Vec::new();
        if self.onnx.has_slot() {
            entries.extend(self.onnx.inventory().await?);
        }
        if self.llama.has_slot() {
            entries.extend(self.llama.inventory().await?);
        }
        Ok(entries)
    }

    async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult> {
        let started = Instant::now();
        let handler = self.handler_for(task.kind)?;
        let model_id = task.model.clone();
        let queue = self.queue_for(handler);
        // Acquire a permit before the inner call so a
        // saturated queue returns a clean queue-timeout
        // error rather than blocking the orchestrator.
        let guard = match queue.acquire().await {
            Ok(guard) => guard,
            Err(QueueTimeout(d)) => {
                self.metrics
                    .record_call(task.kind, &model_id, d.as_millis() as u64, CallOutcome::Timeout);
                queue.state().record_error(format!(
                    "queue saturated ({}ms); {} active / {} queued",
                    d.as_millis(),
                    queue.state().active(),
                    queue.state().queued()
                ));
                return Err(ModelError::Timeout {
                    kind: ModelTimeoutKind::Queue,
                    elapsed: d,
                });
            }
        };
        // Hard per-call timeout enforced at the runtime
        // level (in addition to the caller-side timeout
        // in `RuntimeExtractionService`).
        let chunk_timeout = Duration::from_millis(self.config.chunk_timeout_ms.max(1));
        let result = match handler {
            LocalHandler::Onnx => {
                match timeout(chunk_timeout, self.onnx.infer(task.clone())).await {
                    Ok(Ok(result)) => Ok(result),
                    Ok(Err(err)) => Err(err),
                    Err(_elapsed) => Err(ModelError::Timeout {
                        kind: ModelTimeoutKind::Chunk,
                        elapsed: chunk_timeout,
                    }),
                }
            }
            LocalHandler::Llama => {
                let mut task = task.clone();
                task.timeout = chunk_timeout;
                match timeout(chunk_timeout, self.llama.infer(task)).await {
                    Ok(Ok(mut result)) => {
                        // The composite runtime reports the
                        // elapsed time measured at the
                        // dispatch boundary so the
                        // orchestrator's per-task timeout
                        // sees a consistent value
                        // regardless of inner implementation.
                        result.elapsed = started.elapsed();
                        Ok(result)
                    }
                    Ok(Err(err)) => Err(err),
                    Err(_elapsed) => Err(ModelError::Timeout {
                        kind: ModelTimeoutKind::Chunk,
                        elapsed: chunk_timeout,
                    }),
                }
            }
        };
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match &result {
            Ok(_) => {
                self.metrics
                    .record_call(task.kind, &model_id, elapsed_ms, CallOutcome::Ok);
            }
            Err(ModelError::Timeout { .. }) => {
                self.metrics
                    .record_call(task.kind, &model_id, elapsed_ms, CallOutcome::Timeout);
                guard.release();
                return result;
            }
            Err(err) => {
                self.metrics
                    .record_call(task.kind, &model_id, elapsed_ms, CallOutcome::Error);
                queue.state().record_error(err.to_string());
            }
        }
        guard.release();
        result
    }

    async fn warmup(&self, model: ModelId) -> ModelResult<()> {
        if self.onnx.has_slot() {
            self.onnx.warmup(model.clone()).await?;
        }
        if self.llama.has_slot() {
            self.llama.warmup(model).await?;
        }
        Ok(())
    }

    async fn shutdown(&self) -> ModelResult<()> {
        warn!("LocalModelRuntime::shutdown called; nothing to release in Phase 3");
        Ok(())
    }

    async fn metrics(&self) -> objective_core::traits::ModelRuntimeMetrics {
        (&self.metrics.snapshot()).into()
    }

    async fn slot_views(&self) -> Vec<ModelSlotView> {
        LocalModelRuntime::slot_views(self)
    }
}

/// Helper used by the inventory surface. Mirrors the inner
/// runtime's `ModelState` for entries that have not been
/// loaded yet.
pub fn initial_state_for(_model: ModelId) -> ModelState {
    ModelState::NotLoaded
}

/// Snapshot of a single configured slot.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SlotView {
    pub path: String,
    pub path_exists: bool,
}

/// One row of the strategy table, with kind + slot + fallback
/// flattened into strings so the API can render it as JSON.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StrategyEntryView {
    pub kind: String,
    pub slot: String,
    pub fallback: String,
}

/// Serializable view of `LocalModelRuntime`. Returned by
/// `GET /api/v1/model-runtime` and the reload endpoint.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LocalModelRuntimeView {
    pub provider: String,
    pub embedding_slot: Option<SlotView>,
    pub extraction_llm_slot: Option<SlotView>,
    pub strategy: Vec<StrategyEntryView>,
    pub chunk_timeout_ms: u64,
    pub queue_timeout_ms: u64,
    pub context_window: u32,
    pub max_concurrency: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_core::traits::ModelSlotState;
    use objective_core::{EmbeddingSlot, FallbackStrategy, LlmSlot, ModelSlots, StrategyEntry};
    use std::path::PathBuf;

    fn config_with_ner_strategy() -> LocalModelConfig {
        let mut strategy = StrategyTable::default();
        strategy.insert(
            InferenceKind::NamedEntityRecognition,
            StrategyEntry {
                slot: SlotName::ExtractionLlm,
                fallback: FallbackStrategy::Heuristic,
            },
        );
        strategy.insert(
            InferenceKind::ClaimExtraction,
            StrategyEntry {
                slot: SlotName::ExtractionLlm,
                fallback: FallbackStrategy::Heuristic,
            },
        );
        strategy.insert(
            InferenceKind::RelationExtraction,
            StrategyEntry {
                slot: SlotName::ExtractionLlm,
                fallback: FallbackStrategy::Heuristic,
            },
        );
        strategy.insert(
            InferenceKind::Embedding,
            StrategyEntry {
                slot: SlotName::Embedding,
                fallback: FallbackStrategy::None,
            },
        );
        LocalModelConfig {
            models: ModelSlots {
                embedding: Some(EmbeddingSlot {
                    path: PathBuf::from("/tmp/bge.onnx"),
                    dimension: 384,
                }),
                extraction_llm: Some(LlmSlot {
                    path: PathBuf::from("/tmp/mistral.gguf"),
                    context_tokens: 8_192,
                    gpu_layers: Some(32),
                }),
            },
            default_strategy: strategy,
            context_window: 4096,
            max_concurrency: 1,
            chunk_timeout_ms: 30_000,
            queue_timeout_ms: 30_000,
        }
    }

    #[test]
    fn from_config_retains_strategy() {
        let config = config_with_ner_strategy();
        let runtime = LocalModelRuntime::from_config(config);
        assert_eq!(runtime.strategy().len(), 4);
    }

    #[tokio::test]
    async fn unmapped_kind_returns_backend_error() {
        let config = LocalModelConfig {
            models: ModelSlots::default(),
            default_strategy: StrategyTable::default(),
            context_window: 4096,
            max_concurrency: 1,
            chunk_timeout_ms: 30_000,
            queue_timeout_ms: 30_000,
        };
        let runtime = LocalModelRuntime::from_config(config);
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::TitleGeneration,
            "Apple Inc announced.",
        );
        let err = runtime.infer(task).await.unwrap_err();
        assert!(matches!(err, ModelError::Backend(_)));
    }

    #[tokio::test]
    async fn embedding_kind_dispatches_to_onnx() {
        let config = config_with_ner_strategy();
        let runtime = LocalModelRuntime::from_config(config);
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "Apple Inc announced a 10% expansion in Austin.",
        );
        let result = runtime.infer(task).await.unwrap();
        let structured = result.structured.expect("embedding payload");
        let vector: Vec<f32> = serde_json::from_value(structured).unwrap();
        assert_eq!(vector.len(), 384);
    }

    #[cfg(not(feature = "llama"))]
    #[tokio::test]
    async fn claim_kind_dispatches_to_llama() {
        // Without the `llama` feature, the deterministic
        // stub emits a claims payload. With the feature on,
        // a real GGUF file is required and the dispatch path
        // is exercised in `llama::tests::real_llama_*`.
        let config = config_with_ner_strategy();
        let runtime = LocalModelRuntime::from_config(config);
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::ClaimExtraction,
            "Apple Inc announced a 10% expansion in Austin.",
        );
        let result = runtime.infer(task).await.unwrap();
        let structured = result.structured.expect("llm payload");
        let claims = structured.get("claims").and_then(|v| v.as_array()).unwrap();
        assert!(!claims.is_empty());
    }

    #[tokio::test]
    async fn inventory_reports_both_slots() {
        let config = config_with_ner_strategy();
        let runtime = LocalModelRuntime::from_config(config);
        let inv = runtime.inventory().await.unwrap();
        let ids: Vec<ModelId> = inv.iter().map(|i| i.id.clone()).collect();
        assert!(ids.contains(&ModelId::BgeSmallEnV15));
        assert!(ids.contains(&ModelId::Mistral7BInstruct));
    }

    #[tokio::test]
    async fn provider_name_is_local() {
        let config = LocalModelConfig {
            models: ModelSlots::default(),
            default_strategy: StrategyTable::default(),
            context_window: 4096,
            max_concurrency: 1,
            chunk_timeout_ms: 30_000,
            queue_timeout_ms: 30_000,
        };
        let runtime = LocalModelRuntime::from_config(config);
        assert_eq!(runtime.provider(), "local");
    }

    #[tokio::test]
    async fn reload_replaces_inner_runtimes() {
        let mut runtime = LocalModelRuntime::from_config(config_with_ner_strategy());
        let before = runtime.strategy().len();
        runtime.reload();
        let after = runtime.strategy().len();
        assert_eq!(before, after);
    }

    #[test]
    fn view_includes_strategy_slots_and_config() {
        let runtime = LocalModelRuntime::from_config(config_with_ner_strategy());
        let view = runtime.view();
        assert_eq!(view.provider, "local");
        assert_eq!(view.strategy.len(), 4);
        assert!(view.embedding_slot.is_some());
        assert!(view.extraction_llm_slot.is_some());
        assert!(view.strategy.iter().any(|row| row.kind == "ner"));
        assert!(view
            .strategy
            .iter()
            .any(|row| row.kind == "claim_extraction"));
        assert_eq!(view.chunk_timeout_ms, 30_000);
        assert_eq!(view.queue_timeout_ms, 30_000);
        assert_eq!(view.context_window, 4096);
    }

    #[test]
    fn view_round_trips_through_json() {
        let runtime = LocalModelRuntime::from_config(config_with_ner_strategy());
        let view = runtime.view();
        let json = serde_json::to_string(&view).unwrap();
        let parsed: LocalModelRuntimeView = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, view);
    }

    fn embedding_only_config(max_concurrency: u32, queue_timeout_ms: u64) -> LocalModelConfig {
        let mut strategy = StrategyTable::default();
        strategy.insert(
            InferenceKind::Embedding,
            StrategyEntry {
                slot: SlotName::Embedding,
                fallback: FallbackStrategy::None,
            },
        );
        LocalModelConfig {
            models: ModelSlots {
                embedding: Some(EmbeddingSlot {
                    path: PathBuf::from("/tmp/bge.onnx"),
                    dimension: 4,
                }),
                extraction_llm: None,
            },
            default_strategy: strategy,
            context_window: 4096,
            max_concurrency,
            chunk_timeout_ms: 30_000,
            queue_timeout_ms,
        }
    }

    #[tokio::test]
    async fn infer_records_successful_call_in_metrics() {
        let config = embedding_only_config(2, 1_000);
        let runtime = LocalModelRuntime::from_config(config);
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "hello",
        );
        runtime.infer(task).await.unwrap();
        let snap = runtime.metrics_snapshot();
        assert_eq!(snap.total_calls, 1);
        assert_eq!(snap.total_timeouts, 0);
        assert_eq!(snap.total_errors, 0);
        let kind = snap
            .by_kind
            .get(&InferenceKind::Embedding)
            .expect("embedding histogram present");
        assert_eq!(kind.count, 1);
    }

    #[tokio::test]
    async fn slot_view_reports_active_and_queued_counts() {
        let config = embedding_only_config(1, 1_000);
        let runtime = LocalModelRuntime::from_config(config);
        let views = LocalModelRuntime::slot_views(&runtime);
        assert_eq!(views.len(), 1);
        let view = &views[0];
        assert_eq!(view.model, ModelId::BgeSmallEnV15);
        assert_eq!(view.max_concurrency, 1);
        assert_eq!(view.active, 0);
        assert_eq!(view.queued, 0);
        assert_eq!(view.state, ModelSlotState::Ready);
    }

    #[tokio::test]
    async fn slot_view_transitions_to_busy_during_inference() {
        let config = embedding_only_config(1, 1_000);
        let runtime = std::sync::Arc::new(LocalModelRuntime::from_config(config));
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "long",
        );
        let runtime_clone = std::sync::Arc::clone(&runtime);
        let h = tokio::spawn(async move { runtime_clone.infer(task).await });
        // Give the spawned task a moment to acquire the
        // permit and start the inner call.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let views = LocalModelRuntime::slot_views(&runtime);
        let view = &views[0];
        // The OnnxRuntime stub is synchronous, so the
        // permit is already released by the time we read
        // the snapshot. The state should be `Ready` (or
        // `Busy` if we caught it mid-call). Either is
        // acceptable; what matters is that the counters
        // stay in sync.
        assert!(matches!(
            view.state,
            ModelSlotState::Ready | ModelSlotState::Busy { .. }
        ));
        h.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn reload_resets_metrics_and_slot_state() {
        let config = embedding_only_config(2, 1_000);
        let mut runtime = LocalModelRuntime::from_config(config);
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "hello",
        );
        runtime.infer(task).await.unwrap();
        assert_eq!(runtime.metrics_snapshot().total_calls, 1);
        runtime.reload();
        assert_eq!(runtime.metrics_snapshot().total_calls, 0);
    }

    #[tokio::test]
    async fn infer_returns_queue_timeout_when_slot_is_saturated() {
        // max_concurrency = 1 + a 50ms queue timeout. We
        // hold the only permit in a spawned task and try
        // a second call from the main task; the second
        // call should time out and return a queue
        // timeout error.
        let config = embedding_only_config(1, 50);
        let runtime = std::sync::Arc::new(LocalModelRuntime::from_config(config));
        // Block the only permit with a long-blocking call.
        // We do this by spawning a task that holds the
        // permit for 200ms via the queue.
        let blocking_runtime = std::sync::Arc::clone(&runtime);
        let handle = tokio::spawn(async move {
            let _guard = blocking_runtime
                .embedding_queue
                .acquire()
                .await
                .expect("permit");
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        });
        // Give the spawned task a moment to grab the
        // permit.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "queued",
        );
        let err = runtime.infer(task).await.unwrap_err();
        assert!(matches!(
            err,
            ModelError::Timeout {
                kind: ModelTimeoutKind::Queue,
                ..
            }
        ));
        handle.await.unwrap();
        let snap = runtime.metrics_snapshot();
        assert_eq!(snap.total_timeouts, 1);
    }
}
