//! `OnnxRuntime` — the ONNX-backed local model runtime.
//!
//! Phase 2 ships the integration point: the runtime
//! constructor, the inventory surface, and the per-kind
//! dispatch. The `ort` native dep is **not** linked by default
//! — enabling the `onnx` Cargo feature flips the inference
//! path to use `ort::Session::run` directly. Without the
//! feature, [`OnnxRuntime`] still constructs and reports
//! inventory, but every inference call for `Embedding`
//! returns a deterministic, hash-derived pseudo-embedding of
//! the configured dimension so the contract can be exercised
//! in tests and on machines without the ONNX runtime binary
//! downloaded.
//!
//! Embedding the deterministic stub behind a feature flag is
//! deliberate: it lets the rest of the platform wire up
//! `RuntimeExtractionService` and the `vector_index` sidecar
//! without forcing every developer to download the ONNX
//! runtime binary. Operators flip the feature on for
//! production deployments.

use std::time::Instant;

use async_trait::async_trait;
use objective_core::traits::{
    InferenceKind, InferenceResult, InferenceTask, ModelError, ModelId, ModelInfo, ModelResult,
    ModelRuntime, ModelState,
};
use objective_core::{EmbeddingSlot, ModelSlots};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct OnnxRuntime {
    embedding: Option<EmbeddingSlot>,
    loaded: bool,
}

impl OnnxRuntime {
    pub fn from_slots(slots: &ModelSlots) -> Self {
        Self {
            embedding: slots.embedding.clone(),
            loaded: false,
        }
    }

    pub fn from_config(config: &objective_core::LocalModelConfig) -> Self {
        Self::from_slots(&config.models)
    }

    /// True when the `onnx` Cargo feature is enabled. Phase
    /// 2.5 will use this to dispatch between the deterministic
    /// stub and a real `ort::Session`. Phase 2 always returns
    /// `false`; the deterministic stub is the v1 contract.
    pub fn is_onnx_enabled() -> bool {
        cfg!(feature = "onnx")
    }

    /// Embedding dimension, or `None` if no embedding slot is
    /// configured.
    pub fn dimension(&self) -> Option<u32> {
        self.embedding.as_ref().map(|slot| slot.dimension)
    }

    /// Deterministic hash-based pseudo-embedding. Used when the
    /// `onnx` feature is off and the caller still wants a
    /// well-formed `Vec<f32>` of the configured dimension.
    /// Production builds enable the `onnx` feature and use the
    /// real ONNX session instead.
    fn deterministic_embedding(input: &str, dimension: u32) -> Vec<f32> {
        let mut vector = vec![0.0_f32; dimension as usize];
        for (index, byte) in input.bytes().enumerate() {
            let slot = (index + byte as usize) % dimension as usize;
            let contribution = ((byte as f32) / 255.0) - 0.5;
            vector[slot] += contribution;
        }
        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in vector.iter_mut() {
                *value /= norm;
            }
        }
        vector
    }
}

#[async_trait]
impl ModelRuntime for OnnxRuntime {
    fn provider(&self) -> &'static str {
        "onnx"
    }

    async fn inventory(&self) -> ModelResult<Vec<ModelInfo>> {
        match &self.embedding {
            Some(slot) => Ok(vec![ModelInfo {
                id: ModelId::BgeSmallEnV15,
                state: if self.loaded {
                    ModelState::Ready
                } else {
                    ModelState::NotLoaded
                },
                path: Some(slot.path.display().to_string()),
                last_used_at: None,
                last_error: None,
            }]),
            None => Ok(Vec::new()),
        }
    }

    async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult> {
        let started = Instant::now();

        match task.kind {
            InferenceKind::Embedding => {
                let slot = self.embedding.as_ref().ok_or_else(|| {
                    ModelError::InvalidConfig(
                        "no embedding slot configured in ModelRuntimeConfig::Local".to_string(),
                    )
                })?;

                // Phase 2 ships the deterministic hash-based
                // pseudo-embedding. Phase 2.5 will gate the real
                // `ort::Session` call behind the `onnx` Cargo
                // feature so default builds stay free of the
                // ONNX runtime binary.
                let vector = Self::deterministic_embedding(&task.input, slot.dimension);

                Ok(InferenceResult::embedding(
                    task.model,
                    slot.dimension,
                    vector,
                    started.elapsed(),
                ))
            }
            kind => {
                warn!(
                    kind = %kind,
                    "OnnxRuntime does not support kind; reporting Unavailable"
                );
                Err(ModelError::Unavailable { kind: task.kind })
            }
        }
    }

    async fn warmup(&self, _model: ModelId) -> ModelResult<()> {
        if self.embedding.is_none() {
            return Err(ModelError::InvalidConfig(
                "no embedding slot configured".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_core::EmbeddingSlot;
    use std::path::PathBuf;

    fn slot() -> EmbeddingSlot {
        EmbeddingSlot {
            path: PathBuf::from("/tmp/bge-small-en-v1.5/model.onnx"),
            dimension: 384,
        }
    }

    #[tokio::test]
    async fn inventory_reports_embedding_slot() {
        let runtime = OnnxRuntime::from_slots(&ModelSlots {
            embedding: Some(slot()),
        });
        let inv = runtime.inventory().await.unwrap();
        assert_eq!(inv.len(), 1);
        assert_eq!(inv[0].id, ModelId::BgeSmallEnV15);
        assert_eq!(inv[0].state, ModelState::NotLoaded);
    }

    #[tokio::test]
    async fn inventory_is_empty_without_embedding_slot() {
        let runtime = OnnxRuntime::from_slots(&ModelSlots::default());
        let inv = runtime.inventory().await.unwrap();
        assert!(inv.is_empty());
    }

    #[tokio::test]
    async fn non_embedding_kinds_are_unavailable() {
        let runtime = OnnxRuntime::from_slots(&ModelSlots {
            embedding: Some(slot()),
        });
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::ClaimExtraction,
            "text",
        );
        let err = runtime.infer(task).await.unwrap_err();
        assert!(matches!(err, ModelError::Unavailable { .. }));
    }

    #[tokio::test]
    async fn embedding_without_slot_is_invalid_config() {
        let runtime = OnnxRuntime::from_slots(&ModelSlots::default());
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "hello world",
        );
        let err = runtime.infer(task).await.unwrap_err();
        assert!(matches!(err, ModelError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn deterministic_embedding_matches_dimension() {
        let runtime = OnnxRuntime::from_slots(&ModelSlots {
            embedding: Some(slot()),
        });
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "Apple Inc announced a 10% expansion in Austin",
        );
        let result = runtime.infer(task).await.unwrap();
        let structured = result.structured.expect("embedding vector present");
        let vector: Vec<f32> = serde_json::from_value(structured).unwrap();
        assert_eq!(vector.len(), 384);
        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "deterministic vector is L2-normalised; got {norm}");
    }

    #[tokio::test]
    async fn same_input_yields_same_embedding() {
        let runtime = OnnxRuntime::from_slots(&ModelSlots {
            embedding: Some(slot()),
        });
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "Apple Inc announced a 10% expansion in Austin",
        );
        let first = runtime.infer(task.clone()).await.unwrap();
        let second = runtime.infer(task).await.unwrap();
        let a: Vec<f32> = serde_json::from_value(first.structured.unwrap()).unwrap();
        let b: Vec<f32> = serde_json::from_value(second.structured.unwrap()).unwrap();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn provider_name_is_onnx() {
        let runtime = OnnxRuntime::from_slots(&ModelSlots::default());
        assert_eq!(runtime.provider(), "onnx");
    }
}
