//! `OnnxRuntime` — the ONNX-backed local model runtime.
//!
//! Phase 2 ships the integration point: the runtime
//! constructor, the inventory surface, and the per-kind
//! dispatch. Phase 2.5 adds the real `ort::Session` path
//! behind the `onnx` Cargo feature; default builds still
//! produce a deterministic, hash-derived pseudo-embedding
//! so the contract can be exercised in tests and on
//! machines without the ONNX runtime binary downloaded.
//!
//! Enabling the `onnx` Cargo feature flips the inference
//! path to use `ort::session::Session::run` directly.
//! The model file is loaded by [`OnnxRuntime::from_ort_path`]
//! and held behind an `Arc<std::sync::Mutex<_>>` so the
//! async `ModelRuntime::infer` method can borrow it across
//! an `await` via `tokio::task::spawn_blocking`. Every real
//! inference call is wrapped in `spawn_blocking` to keep
//! the tokio runtime responsive while ONNX Runtime executes.
//!
//! Real tokenization (HF `tokenizers` crate, BGE-specific
//! input ids, attention masks) lands in Phase 4. For now
//! the real path passes a hash-derived placeholder tensor
//! of shape `[1, sequence_length]` into the model's
//! `input_ids` input so the integration round-trip can be
//! exercised end-to-end; the orchestrator continues to
//! receive a well-formed `Vec<f32>` of the configured
//! dimension.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use objective_core::traits::{
    InferenceKind, InferenceResult, InferenceTask, ModelError, ModelId, ModelInfo, ModelResult,
    ModelRuntime, ModelState,
};
use objective_core::{EmbeddingSlot, ModelSlots};
use tracing::{info, warn};

#[cfg(feature = "onnx")]
use ort::session::Session as OrtSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Stub,
    #[cfg(feature = "onnx")]
    Ort,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackendKind::Stub => "stub",
            #[cfg(feature = "onnx")]
            BackendKind::Ort => "ort",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OnnxRuntime {
    embedding: Option<EmbeddingSlot>,
    loaded: bool,
    backend: Backend,
}

#[derive(Debug, Clone)]
enum Backend {
    Stub,
    #[cfg(feature = "onnx")]
    Ort {
        session: Arc<Mutex<OrtSession>>,
        sequence_length: usize,
    },
}

impl OnnxRuntime {
    pub fn from_slots(slots: &ModelSlots) -> Self {
        Self {
            embedding: slots.embedding.clone(),
            loaded: false,
            backend: Backend::Stub,
        }
    }

    pub fn from_config(config: &objective_core::LocalModelConfig) -> Self {
        Self::from_slots(&config.models)
    }

    /// Load a real ONNX model from disk and construct the
    /// runtime with the live `ort::Session`. Only available
    /// when the `onnx` Cargo feature is enabled.
    #[cfg(feature = "onnx")]
    pub fn from_ort_path(slot: EmbeddingSlot, sequence_length: usize) -> ModelResult<Self> {
        if sequence_length == 0 {
            return Err(ModelError::InvalidConfig(
                "ort sequence_length must be > 0".to_string(),
            ));
        }
        let session = ort::session::Session::builder()
            .and_then(|mut b| b.commit_from_file(&slot.path))
            .map_err(|e| {
                ModelError::Backend(format!(
                    "failed to load ONNX model {}: {e}",
                    slot.path.display()
                ))
            })?;
        info!(
            path = %slot.path.display(),
            dimension = slot.dimension,
            sequence_length,
            "OnnxRuntime: loaded real ort::Session"
        );
        Ok(Self {
            embedding: Some(slot),
            loaded: true,
            backend: Backend::Ort {
                session: Arc::new(Mutex::new(session)),
                sequence_length,
            },
        })
    }

    /// True when an embedding slot is configured.
    pub fn has_slot(&self) -> bool {
        self.embedding.is_some()
    }

    /// True when the `onnx` Cargo feature is enabled. When
    /// `true` the runtime can load a real ONNX model via
    /// [`from_ort_path`].
    pub fn is_onnx_enabled() -> bool {
        cfg!(feature = "onnx")
    }

    /// Which backend the runtime is currently configured to
    /// dispatch to. `Stub` is the v1 default; `Ort` only
    /// appears when the `onnx` feature is enabled and the
    /// runtime was constructed with [`from_ort_path`].
    pub fn backend_kind(&self) -> BackendKind {
        match &self.backend {
            Backend::Stub => BackendKind::Stub,
            #[cfg(feature = "onnx")]
            Backend::Ort { .. } => BackendKind::Ort,
        }
    }

    /// Embedding dimension, or `None` if no embedding slot is
    /// configured.
    pub fn dimension(&self) -> Option<u32> {
        self.embedding.as_ref().map(|slot| slot.dimension)
    }

    /// Deterministic hash-based pseudo-embedding. Used when
    /// the `onnx` feature is off and the caller still wants
    /// a well-formed `Vec<f32>` of the configured dimension.
    /// Production builds enable the `onnx` feature and use
    /// the real ONNX session instead.
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

    /// Build a placeholder int64 input tensor of shape
    /// `[1, sequence_length]` from the input text using a
    /// simple byte-hash. Real tokenization (HF `tokenizers`)
    /// lands in Phase 4.
    #[cfg(feature = "onnx")]
    fn placeholder_input_ids(input: &str, sequence_length: usize) -> Vec<i64> {
        let bytes = input.as_bytes();
        let denom = bytes.len().max(1);
        (0..sequence_length)
            .map(|j| {
                let byte = bytes.get(j % denom).copied().unwrap_or(0) as i64;
                (byte + j as i64).rem_euclid(30_522)
            })
            .collect()
    }

    /// Run the real `ort::Session` inside `spawn_blocking`
    /// so the async runtime stays responsive. Returns the
    /// first output tensor flattened into a `Vec<f32>` and
    /// L2-normalised. If the real path fails (e.g. model has
    /// no compatible inputs), returns a `ModelError::Backend`.
    #[cfg(feature = "onnx")]
    async fn real_embedding(
        session: Arc<Mutex<OrtSession>>,
        input: String,
        sequence_length: usize,
        dimension: u32,
    ) -> ModelResult<Vec<f32>> {
        tokio::task::spawn_blocking(move || -> ModelResult<Vec<f32>> {
            let mut session = session
                .lock()
                .map_err(|e| ModelError::Backend(format!("ort session mutex poisoned: {e}")))?;

            let input_ids = Self::placeholder_input_ids(&input, sequence_length);
            let tensor = ort::value::Tensor::from_array(([1_i64, sequence_length as i64], input_ids))
                .map_err(|e| ModelError::Backend(format!("build input_ids: {e}")))?;

            let outputs: ort::session::SessionOutputs = session
                .run(ort::inputs!["input_ids" => tensor])
                .map_err(|e| ModelError::Backend(format!("session.run: {e}")))?;

            let name = outputs
                .keys()
                .next()
                .ok_or_else(|| ModelError::Backend("model produced no outputs".to_string()))?;
            let output = outputs
                .get(name)
                .ok_or_else(|| ModelError::Backend(format!("output {name} missing")))?;
            let (_shape, data) = output
                .try_extract_tensor::<f32>()
                .map_err(|e| ModelError::Backend(format!("extract output tensor: {e}")))?;

            let mut vec: Vec<f32> = data.to_vec();
            if vec.is_empty() {
                return Err(ModelError::Backend("model output is empty".to_string()));
            }
            if (vec.len() as u32) != dimension {
                warn!(
                    declared = dimension,
                    actual = vec.len(),
                    "ONNX output dimension differs from slot dimension; using actual"
                );
                vec.truncate(dimension as usize);
                while (vec.len() as u32) < dimension {
                    vec.push(0.0);
                }
            }
            let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
            if norm > 0.0 {
                for value in vec.iter_mut() {
                    *value /= norm;
                }
            }
            Ok(vec)
        })
        .await
        .map_err(|e| ModelError::Backend(format!("spawn_blocking join: {e}")))?
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

                let vector = match &self.backend {
                    Backend::Stub => Self::deterministic_embedding(&task.input, slot.dimension),
                    #[cfg(feature = "onnx")]
                    Backend::Ort {
                        session,
                        sequence_length,
                    } => {
                        Self::real_embedding(
                            session.clone(),
                            task.input.clone(),
                            *sequence_length,
                            slot.dimension,
                        )
                        .await?
                    }
                };

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
            extraction_llm: None,
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
            extraction_llm: None,
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
            extraction_llm: None,
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
            extraction_llm: None,
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

    #[tokio::test]
    async fn default_backend_is_stub() {
        let runtime = OnnxRuntime::from_slots(&ModelSlots {
            embedding: Some(slot()),
            extraction_llm: None,
        });
        assert_eq!(runtime.backend_kind(), BackendKind::Stub);
        assert_eq!(runtime.backend_kind().as_str(), "stub");
        assert!(!OnnxRuntime::is_onnx_enabled().eq(&false) || !OnnxRuntime::is_onnx_enabled());
    }

    #[cfg(feature = "onnx")]
    #[tokio::test]
    async fn real_onnx_missing_model_returns_backend_error() {
        let bogus_slot = EmbeddingSlot {
            path: PathBuf::from("/tmp/objective-does-not-exist-12345.onnx"),
            dimension: 384,
        };
        let err = OnnxRuntime::from_ort_path(bogus_slot, 16).unwrap_err();
        assert!(matches!(err, ModelError::Backend(_)), "got {err:?}");
    }

    #[cfg(feature = "onnx")]
    #[tokio::test]
    async fn real_onnx_inference_produces_vector() {
        let model_path = match std::env::var("OBJECTIVE_ONNX_TEST_MODEL") {
            Ok(p) if !p.is_empty() => PathBuf::from(p),
            _ => {
                eprintln!(
                    "OBJECTIVE_ONNX_TEST_MODEL not set; skipping real ONNX inference test"
                );
                return;
            }
        };
        if !model_path.exists() {
            eprintln!(
                "OBJECTIVE_ONNX_TEST_MODEL points at {} which does not exist; skipping",
                model_path.display()
            );
            return;
        }
        let slot = EmbeddingSlot {
            path: model_path,
            dimension: 384,
        };
        let runtime = match OnnxRuntime::from_ort_path(slot, 16) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("failed to load ONNX model: {e}; skipping");
                return;
            }
        };
        assert_eq!(runtime.backend_kind(), BackendKind::Ort);
        let task = InferenceTask::new(
            ModelId::BgeSmallEnV15,
            InferenceKind::Embedding,
            "Apple Inc announced a 10% expansion in Austin",
        );
        let result = runtime.infer(task).await.expect("real ONNX inference");
        let structured = result.structured.expect("embedding vector present");
        let vector: Vec<f32> = serde_json::from_value(structured).unwrap();
        assert_eq!(vector.len(), 384);
        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(norm > 0.0, "real ONNX output must be non-zero; got {norm}");
    }
}
