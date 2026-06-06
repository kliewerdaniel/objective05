//! `NoopRuntime` — a faithful, side-effect-free implementation
//! of [`ModelRuntime`] used as the v1 default provider.
//!
//! The runtime never loads a model and never calls an external
//! service. It exists so the rest of the platform can wire up
//! `RuntimeExtractionService` and the per-chunk fallback path
//! before any real inference backend lands. Two strategies are
//! exposed:
//!
//! * [`NoopStrategy::Heuristic`] — pretends the runtime is
//!   busy or unavailable for non-`Heuristic` inference kinds,
//!   forcing `RuntimeExtractionService` to exercise the
//!   per-chunk fallback path. Useful for tests that want to
//!   confirm the orchestrator honours the fallback contract.
//! * [`NoopStrategy::Passthrough`] — accepts every task and
//!   returns a synthetic `InferenceResult` whose `text` is
//!   the trimmed input. Used when the trait must be
//!   satisfied but no actual inference is desired.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use objective_core::traits::{
    InferenceKind, InferenceResult, InferenceTask, ModelError, ModelId, ModelInfo, ModelResult,
    ModelRuntime, ModelState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoopStrategy {
    /// Report the runtime as unavailable for any
    /// non-`Embedding` kind so the orchestrator must fall
    /// back to the heuristic provider.
    Heuristic,
    /// Accept every task and return a synthetic
    /// passthrough result.
    Passthrough,
}

#[derive(Debug, Clone)]
pub struct NoopRuntime {
    strategy: NoopStrategy,
}

impl NoopRuntime {
    pub fn heuristic() -> Self {
        Self {
            strategy: NoopStrategy::Heuristic,
        }
    }

    pub fn passthrough() -> Self {
        Self {
            strategy: NoopStrategy::Passthrough,
        }
    }

    pub fn strategy(&self) -> NoopStrategy {
        self.strategy
    }
}

impl Default for NoopRuntime {
    fn default() -> Self {
        Self::passthrough()
    }
}

#[async_trait]
impl ModelRuntime for NoopRuntime {
    fn provider(&self) -> &'static str {
        "noop"
    }

    async fn inventory(&self) -> ModelResult<Vec<ModelInfo>> {
        match self.strategy {
            NoopStrategy::Heuristic => Ok(Vec::new()),
            NoopStrategy::Passthrough => Ok(vec![ModelInfo {
                id: ModelId::Mistral7BInstruct,
                state: ModelState::NotLoaded,
                path: None,
                last_used_at: None,
                last_error: None,
            }]),
        }
    }

    async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult> {
        let started = Instant::now();
        match self.strategy {
            NoopStrategy::Heuristic => {
                // Surface a synthetic Unavailable for every kind
                // that we want the orchestrator to fall back
                // from. Embedding is the only kind the noop
                // runtime "supports" so we can exercise both
                // success and failure paths in tests.
                match task.kind {
                    InferenceKind::Embedding => Ok(InferenceResult::text_only(
                        task.model,
                        task.kind,
                        format!("noop:embedding:{}", task.input.len()),
                        started.elapsed(),
                    )),
                    _ => Err(ModelError::Unavailable { kind: task.kind }),
                }
            }
            NoopStrategy::Passthrough => Ok(InferenceResult::text_only(
                task.model,
                task.kind,
                task.input.trim().to_string(),
                started.elapsed(),
            )),
        }
    }

    async fn warmup(&self, _model: ModelId) -> ModelResult<()> {
        // Noop: no model to load.
        Ok(())
    }

    async fn shutdown(&self) -> ModelResult<()> {
        Ok(())
    }
}

/// Smallest acceptable "inference duration" the test suite can
/// assert on. The noop runtime should always be effectively
/// instant.
pub const NOOP_INFERENCE_BUDGET: Duration = Duration::from_millis(50);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn heuristic_strategy_rejects_non_embedding_kinds() {
        let runtime = NoopRuntime::heuristic();
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::ClaimExtraction,
            "Apple Inc announced a 10% expansion in Austin.",
        );
        let err = runtime.infer(task).await.unwrap_err();
        assert!(matches!(
            err,
            ModelError::Unavailable {
                kind: InferenceKind::ClaimExtraction
            }
        ));
    }

    #[tokio::test]
    async fn heuristic_strategy_accepts_embedding() {
        let runtime = NoopRuntime::heuristic();
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "document body",
        );
        let result = runtime.infer(task).await.unwrap();
        assert!(result.text.starts_with("noop:embedding:"));
        assert!(result.elapsed < NOOP_INFERENCE_BUDGET);
    }

    #[tokio::test]
    async fn passthrough_strategy_returns_input_trimmed() {
        let runtime = NoopRuntime::passthrough();
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::ClaimExtraction,
            "  Apple Inc announced.  ",
        );
        let result = runtime.infer(task).await.unwrap();
        assert_eq!(result.text, "Apple Inc announced.");
    }

    #[tokio::test]
    async fn passthrough_strategy_reports_provider_as_noop() {
        let runtime = NoopRuntime::passthrough();
        assert_eq!(runtime.provider(), "noop");
        let inventory = runtime.inventory().await.unwrap();
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].id, ModelId::Mistral7BInstruct);
    }

    #[tokio::test]
    async fn warmup_and_shutdown_are_noops() {
        let runtime = NoopRuntime::passthrough();
        runtime.warmup(ModelId::Mistral7BInstruct).await.unwrap();
        runtime.shutdown().await.unwrap();
    }
}
