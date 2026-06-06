//! `objective-model-runtime` — inference provider surface.
//!
//! This crate is the integration point between the trait
//! defined in `objective-core::traits::runtime` and the actual
//! inference backends (Phase 1: none, Phase 2: ONNX, Phase 3:
//! llama.cpp). The v1 ships a single provider,
//! [`NoopRuntime`], which is itself a no-op but a faithful
//! implementation of the trait: it lets the rest of the system
//! wire up `RuntimeExtractionService` and the per-chunk
//! fallback path before any model integration lands.
//!
//! See `docs/processing/model-runtime.md` and ADR-015 for the
//! phasing plan.

pub mod noop;

pub use noop::{NoopRuntime, NoopStrategy};

use std::sync::Arc;

use objective_core::traits::{ModelInfo, ModelRuntime};

/// Convenience constructor for the v1 provider. Phase 1 always
/// returns a `NoopRuntime` regardless of the configured
/// `ModelRuntimeConfig` variant; later phases will dispatch on
/// `ModelRuntimeConfig` here.
pub fn default_runtime() -> Arc<dyn ModelRuntime> {
    Arc::new(NoopRuntime::passthrough())
}

/// Factory used by `crates/objective` to pick a runtime based
/// on `ModelRuntimeConfig`. Phase 1 maps every non-`Disabled`
/// variant onto `NoopRuntime`; Phase 2/3 will return ONNX and
/// llama.cpp implementations.
pub fn runtime_for(
    config: &objective_core::ModelRuntimeConfig,
) -> Result<Arc<dyn ModelRuntime>, objective_core::ObjectiveError> {
    match config {
        objective_core::ModelRuntimeConfig::Disabled => {
            Err(objective_core::ObjectiveError::Config(
                "runtime_for() called with ModelRuntimeConfig::Disabled; \
                 callers should use HeuristicExtractionService directly in that case"
                    .to_string(),
            ))
        }
        objective_core::ModelRuntimeConfig::Heuristic => Ok(Arc::new(NoopRuntime::heuristic())),
        objective_core::ModelRuntimeConfig::Local(_) => Ok(Arc::new(NoopRuntime::passthrough())),
    }
}

/// Helper for the future `/api/v1/model-runtime` endpoint —
/// returns the inventory of a runtime as a serializable JSON
/// value. Lives here so the API crate does not have to depend
/// on the trait directly.
pub fn inventory(_runtime: &Arc<dyn ModelRuntime>) -> Vec<ModelInfo> {
    // The trait method is async; the future inventory endpoint
    // will await it. For now we return an empty list when no
    // runtime is wired (the API crate will eventually call this
    // from a tokio handler).
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_core::{LocalModelConfig, ModelRuntimeConfig};

    #[test]
    fn runtime_for_disabled_returns_error() {
        let result = runtime_for(&ModelRuntimeConfig::Disabled);
        assert!(result.is_err());
    }

    #[test]
    fn runtime_for_heuristic_returns_noop_with_heuristic_strategy() {
        let runtime = runtime_for(&ModelRuntimeConfig::Heuristic).unwrap();
        assert_eq!(runtime.provider(), "noop");
    }

    #[test]
    fn runtime_for_local_returns_passthrough_noop() {
        let local = ModelRuntimeConfig::Local(LocalModelConfig {
            model_path: std::path::PathBuf::from("/tmp/model"),
            context_window: 4096,
            max_concurrency: 1,
            chunk_timeout_ms: 30_000,
        });
        let runtime = runtime_for(&local).unwrap();
        assert_eq!(runtime.provider(), "noop");
    }

    #[tokio::test]
    async fn default_runtime_is_usable() {
        let runtime = default_runtime();
        assert_eq!(runtime.provider(), "noop");
    }
}
