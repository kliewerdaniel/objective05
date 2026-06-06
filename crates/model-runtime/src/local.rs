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
use std::time::Instant;

use async_trait::async_trait;
use objective_core::traits::{
    InferenceKind, InferenceResult, InferenceTask, ModelError, ModelId, ModelInfo, ModelResult,
    ModelRuntime, ModelState,
};
use objective_core::{LocalModelConfig, SlotName, StrategyTable};
use tracing::warn;

use crate::llama::LlamaRuntime;
use crate::onnx::OnnxRuntime;

#[derive(Debug, Clone)]
pub struct LocalModelRuntime {
    onnx: OnnxRuntime,
    llama: LlamaRuntime,
    strategy: StrategyTable,
    config: Arc<LocalModelConfig>,
}

impl LocalModelRuntime {
    pub fn from_config(config: LocalModelConfig) -> Self {
        let onnx = OnnxRuntime::from_config(&config);
        let llama = LlamaRuntime::from_slot(config.models.extraction_llm.clone());
        let strategy = config.default_strategy.clone();
        Self {
            onnx,
            llama,
            strategy,
            config: Arc::new(config),
        }
    }

    pub fn config(&self) -> &LocalModelConfig {
        &self.config
    }

    pub fn strategy(&self) -> &StrategyTable {
        &self.strategy
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

    /// Trigger a hot reload. Phase 3 rebuilds the inner
    /// runtimes from the captured `LocalModelConfig`; Phase
    /// 3.5 will additionally unload the previous
    /// `llama-cpp-rs` session.
    pub fn reload(&mut self) {
        let config = (*self.config).clone();
        self.onnx = OnnxRuntime::from_config(&config);
        self.llama = LlamaRuntime::from_slot(config.models.extraction_llm.clone());
        self.strategy = config.default_strategy.clone();
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
        match self.handler_for(task.kind)? {
            LocalHandler::Onnx => self.onnx.infer(task).await,
            LocalHandler::Llama => {
                let mut result = self.llama.infer(task).await?;
                // The composite runtime reports the elapsed
                // time measured at the dispatch boundary so
                // the orchestrator's per-task timeout sees a
                // consistent value regardless of inner
                // implementation.
                result.elapsed = started.elapsed();
                Ok(result)
            }
        }
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
    pub context_window: u32,
    pub max_concurrency: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
