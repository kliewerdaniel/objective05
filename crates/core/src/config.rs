use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{ObjectiveError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveConfig {
    pub data_root: PathBuf,
    pub api: ApiConfig,
    pub storage: StorageConfig,
    pub message_bus: MessageBusConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub model_runtime: ModelRuntimeConfig,
}

/// Selects the model runtime used by the extraction pipeline.
///
/// The default is `Disabled`, which preserves the v0 heuristic-only
/// pipeline. `Heuristic` and `Local` route extraction through
/// `RuntimeExtractionService`, which fans out per-chunk inference
/// to the configured runtime and falls back to the heuristic
/// provider on a per-chunk error or timeout. Phase 1 ships only
/// the `NoopRuntime` provider; Phase 2 (`onnx`) and Phase 3
/// (`llama`) will plug a real backend in here.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ModelRuntimeConfig {
    /// Default: do not construct a runtime. Extraction uses
    /// `HeuristicExtractionService` directly. v0 behaviour.
    #[default]
    Disabled,
    /// Use the heuristic fallback service; chunk-level
    /// `InferenceResult`s are produced by `NoopRuntime` and
    /// accepted verbatim. Useful for wiring tests and for
    /// demonstrating the trait surface without a model.
    Heuristic,
    /// Use an in-process local model. Phase 1 maps this onto
    /// the same `NoopRuntime`; Phase 2/3 will swap in
    /// `OnnxRuntime` or `LlamaRuntime` based on the embedded
    /// `LocalModelConfig`.
    Local(LocalModelConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalModelConfig {
    /// Model slots. Phase 2 ships the `embedding` slot (ONNX);
    /// Phase 3 adds `extraction_llm` (llama.cpp).
    #[serde(default)]
    pub models: ModelSlots,
    /// Per-kind routing table. Maps each `InferenceKind` to
    /// the slot that should handle it and the fallback
    /// strategy on a per-chunk error. Empty by default — the
    /// orchestrator falls back to the heuristic provider for
    /// every kind until the operator adds entries.
    #[serde(default)]
    pub default_strategy: StrategyTable,
    /// Context window in tokens. Defaults to 4096.
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    /// Maximum concurrent inference requests per slot.
    /// Defaults to 4 (the documented Phase 4 default).
    /// Calls beyond this limit queue up to
    /// `queue_timeout_ms` before the orchestrator falls
    /// back to the heuristic.
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: u32,
    /// Per-chunk timeout in milliseconds. Defaults to 30s.
    /// Enforced by the runtime itself in addition to the
    /// caller-side timeout in `RuntimeExtractionService`.
    #[serde(default = "default_chunk_timeout_ms")]
    pub chunk_timeout_ms: u64,
    /// Per-call queue-acquire timeout in milliseconds. A
    /// caller waiting for a permit longer than this value
    /// receives `ModelError::Timeout` and the orchestrator
    /// falls back to the heuristic for the chunk. Defaults
    /// to 30s (matches `chunk_timeout_ms`).
    #[serde(default = "default_queue_timeout_ms")]
    pub queue_timeout_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSlots {
    /// ONNX-backed embedding slot. Phase 2 only.
    pub embedding: Option<EmbeddingSlot>,
    /// llama.cpp-backed extraction LLM slot. Phase 3 only.
    pub extraction_llm: Option<LlmSlot>,
}

/// One named ONNX embedding model. The `dimension` is asserted
/// at runtime against the model's output shape; a mismatch is
/// reported as `Backend("dimension mismatch")` so the
/// orchestrator can fall back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingSlot {
    pub path: PathBuf,
    pub dimension: u32,
}

/// One named llama.cpp LLM slot. The `context_tokens` field
/// documents the model's advertised context window; the
/// runtime enforces it at chunking time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmSlot {
    pub path: PathBuf,
    pub context_tokens: u32,
    /// Optional number of layers to offload to GPU. The
    /// stub `LlamaRuntime` ignores it; Phase 3.5 will use it
    /// when wiring the real `llama-cpp-rs` session.
    #[serde(default)]
    pub gpu_layers: Option<u32>,
}

/// Per-kind routing entry. The slot name is looked up
/// against `ModelSlots` at runtime; the fallback describes
/// what to do when the call fails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrategyEntry {
    pub slot: SlotName,
    #[serde(default)]
    pub fallback: FallbackStrategy,
}

/// Name of a configured slot. Phase 3 ships two variants;
/// future phases will add `Remote(name)` and `Plugin(name)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlotName {
    Embedding,
    ExtractionLlm,
    Custom(String),
}

impl SlotName {
    pub fn as_str(&self) -> &str {
        match self {
            SlotName::Embedding => "embedding",
            SlotName::ExtractionLlm => "extraction_llm",
            SlotName::Custom(name) => name.as_str(),
        }
    }
}

/// Fallback policy when the configured slot returns
/// `Unavailable` or `Backend`. `Heuristic` routes the
/// affected chunk through the heuristic provider; `None`
/// skips the chunk entirely.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackStrategy {
    #[default]
    Heuristic,
    None,
}

/// `InferenceKind` -> `StrategyEntry`. Stored as a
/// `BTreeMap` so the YAML form is stable and diffable.
pub type StrategyTable = std::collections::BTreeMap<crate::traits::InferenceKind, StrategyEntry>;

fn default_context_window() -> u32 {
    4096
}

fn default_max_concurrency() -> u32 {
    4
}

fn default_chunk_timeout_ms() -> u64 {
    30_000
}

fn default_queue_timeout_ms() -> u64 {
    30_000
}

impl ModelRuntimeConfig {
    /// Convenience: is a runtime required for the configured mode?
    pub fn requires_runtime(&self) -> bool {
        !matches!(self, ModelRuntimeConfig::Disabled)
    }

    /// Per-chunk timeout if a runtime is configured.
    pub fn chunk_timeout(&self) -> Option<std::time::Duration> {
        match self {
            ModelRuntimeConfig::Disabled => None,
            ModelRuntimeConfig::Heuristic => Some(std::time::Duration::from_secs(30)),
            ModelRuntimeConfig::Local(cfg) => {
                Some(std::time::Duration::from_millis(cfg.chunk_timeout_ms))
            }
        }
    }

    /// Per-call queue-acquire timeout if a runtime is
    /// configured. Mirrors [`Self::chunk_timeout`].
    pub fn queue_timeout(&self) -> Option<std::time::Duration> {
        match self {
            ModelRuntimeConfig::Disabled => None,
            ModelRuntimeConfig::Heuristic => Some(std::time::Duration::from_secs(30)),
            ModelRuntimeConfig::Local(cfg) => {
                Some(std::time::Duration::from_millis(cfg.queue_timeout_ms))
            }
        }
    }
}

impl std::fmt::Display for ModelRuntimeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelRuntimeConfig::Disabled => f.write_str("disabled"),
            ModelRuntimeConfig::Heuristic => f.write_str("heuristic"),
            ModelRuntimeConfig::Local(cfg) => {
                let embedding = if cfg.models.embedding.is_some() {
                    "on"
                } else {
                    "off"
                };
                let llm = if cfg.models.extraction_llm.is_some() {
                    "on"
                } else {
                    "off"
                };
                write!(
                    f,
                    "local({}ms/queue{}ms, ctx={}, conc={}, embed={}, llm={}, strategy={})",
                    cfg.chunk_timeout_ms,
                    cfg.queue_timeout_ms,
                    cfg.context_window,
                    cfg.max_concurrency,
                    embedding,
                    llm,
                    cfg.default_strategy.len()
                )
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiConfig {
    pub rest_port: u16,
    pub websocket_port: u16,
    pub cors_allowed_origins: Vec<String>,
    pub auth_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageConfig {
    pub database_path: PathBuf,
    pub document_path: PathBuf,
    pub vector_path: PathBuf,
    pub queue_path: PathBuf,
    pub graph_path: PathBuf,
    pub embedding_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageBusConfig {
    pub nats_url: String,
    pub use_embedded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoggingConfig {
    pub level: String,
}

impl Default for ObjectiveConfig {
    fn default() -> Self {
        let data_root = PathBuf::from(".objective");
        Self {
            storage: StorageConfig {
                database_path: data_root.join("db/database"),
                document_path: data_root.join("documents"),
                vector_path: data_root.join("vectors"),
                queue_path: data_root.join("queue"),
                graph_path: data_root.join("graph"),
                embedding_path: data_root.join("embeddings"),
            },
            data_root,
            api: ApiConfig {
                rest_port: 8080,
                websocket_port: 8081,
                cors_allowed_origins: vec!["http://localhost:5173".to_string()],
                auth_enabled: false,
            },
            message_bus: MessageBusConfig {
                nats_url: "nats://127.0.0.1:4222".to_string(),
                use_embedded: true,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
            model_runtime: ModelRuntimeConfig::default(),
        }
    }
}

impl ObjectiveConfig {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let mut builder = config::Config::builder().add_source(
            config::Config::try_from(&Self::default()).map_err(|error| {
                ObjectiveError::Config(format!("failed to load default config: {error}"))
            })?,
        );

        if let Some(path) = path {
            builder = builder.add_source(config::File::from(path).required(false));
        }

        builder
            .add_source(config::Environment::with_prefix("OBJECTIVE").separator("__"))
            .build()
            .map_err(|error| ObjectiveError::Config(error.to_string()))?
            .try_deserialize()
            .map_err(|error| ObjectiveError::Config(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_uses_local_data_root() {
        let config = ObjectiveConfig::default();

        assert_eq!(config.api.rest_port, 8080);
        assert_eq!(
            config.storage.document_path,
            PathBuf::from(".objective/documents")
        );
        assert_eq!(config.storage.graph_path, PathBuf::from(".objective/graph"));
        assert_eq!(
            config.storage.embedding_path,
            PathBuf::from(".objective/embeddings")
        );
        assert_eq!(config.message_bus.nats_url, "nats://127.0.0.1:4222");
        assert_eq!(config.model_runtime, ModelRuntimeConfig::Disabled);
    }

    #[test]
    fn disabled_mode_does_not_require_runtime() {
        assert!(!ModelRuntimeConfig::Disabled.requires_runtime());
        assert!(ModelRuntimeConfig::Heuristic.requires_runtime());
        let local = ModelRuntimeConfig::Local(LocalModelConfig {
            models: ModelSlots::default(),
            default_strategy: StrategyTable::default(),
            context_window: 4096,
            max_concurrency: 1,
            chunk_timeout_ms: 30_000,
            queue_timeout_ms: 30_000,
        });
        assert!(local.requires_runtime());
    }

    #[test]
    fn chunk_timeout_matches_configured_value() {
        let disabled = ModelRuntimeConfig::Disabled;
        assert!(disabled.chunk_timeout().is_none());

        let local = ModelRuntimeConfig::Local(LocalModelConfig {
            models: ModelSlots::default(),
            default_strategy: StrategyTable::default(),
            context_window: 4096,
            max_concurrency: 2,
            chunk_timeout_ms: 5_000,
            queue_timeout_ms: 7_000,
        });
        assert_eq!(
            local.chunk_timeout(),
            Some(std::time::Duration::from_millis(5_000))
        );
    }

    #[test]
    fn queue_timeout_matches_configured_value() {
        let local = ModelRuntimeConfig::Local(LocalModelConfig {
            models: ModelSlots::default(),
            default_strategy: StrategyTable::default(),
            context_window: 4096,
            max_concurrency: 2,
            chunk_timeout_ms: 5_000,
            queue_timeout_ms: 7_000,
        });
        assert_eq!(
            local.queue_timeout(),
            Some(std::time::Duration::from_millis(7_000))
        );
    }

    #[test]
    fn queue_timeout_defaults_to_thirty_seconds() {
        let local = ModelRuntimeConfig::Local(LocalModelConfig {
            models: ModelSlots::default(),
            default_strategy: StrategyTable::default(),
            context_window: 4096,
            max_concurrency: 4,
            chunk_timeout_ms: 30_000,
            queue_timeout_ms: 30_000,
        });
        assert_eq!(
            local.queue_timeout(),
            Some(std::time::Duration::from_millis(30_000))
        );
    }

    #[test]
    fn max_concurrency_defaults_to_four() {
        let yaml = r#"
data_root: .objective
api:
  rest_port: 8080
  websocket_port: 8081
  cors_allowed_origins: ["http://localhost:5173"]
  auth_enabled: false
storage:
  database_path: .objective/db
  document_path: .objective/documents
  vector_path: .objective/vectors
  queue_path: .objective/queue
  graph_path: .objective/graph
  embedding_path: .objective/embeddings
message_bus:
  nats_url: nats://127.0.0.1:4222
  use_embedded: true
logging:
  level: info
model_runtime:
  mode: local
  models: {}
  default_strategy: {}
  context_window: 4096
  chunk_timeout_ms: 30000
"#;
        let config: ObjectiveConfig = serde_yaml::from_str(yaml).unwrap();
        match config.model_runtime {
            ModelRuntimeConfig::Local(local) => {
                assert_eq!(local.max_concurrency, 4);
                assert_eq!(local.queue_timeout_ms, 30_000);
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn display_is_stable_for_log_lines() {
        assert_eq!(ModelRuntimeConfig::Disabled.to_string(), "disabled");
        assert_eq!(ModelRuntimeConfig::Heuristic.to_string(), "heuristic");
        assert_eq!(
            ModelRuntimeConfig::Local(LocalModelConfig {
                models: ModelSlots::default(),
                default_strategy: StrategyTable::default(),
                context_window: 4096,
                max_concurrency: 1,
                chunk_timeout_ms: 5_000,
                queue_timeout_ms: 5_000,
            })
            .to_string(),
            "local(5000ms/queue5000ms, ctx=4096, conc=1, embed=off, llm=off, strategy=0)"
        );
        assert_eq!(
            ModelRuntimeConfig::Local(LocalModelConfig {
                models: ModelSlots {
                    embedding: Some(EmbeddingSlot {
                        path: PathBuf::from(".objective/models/bge-small-en-v1.5/model.onnx"),
                        dimension: 384,
                    }),
                    extraction_llm: Some(LlmSlot {
                        path: PathBuf::from(".objective/models/mistral-7b-instruct-v0.3.Q4_K_M.gguf"),
                        context_tokens: 8_192,
                        gpu_layers: Some(32),
                    }),
                },
                default_strategy: StrategyTable::default(),
                context_window: 4096,
                max_concurrency: 1,
                chunk_timeout_ms: 5_000,
                queue_timeout_ms: 5_000,
            })
            .to_string(),
            "local(5000ms/queue5000ms, ctx=4096, conc=1, embed=on, llm=on, strategy=0)"
        );
    }

    #[test]
    fn strategy_table_round_trips_through_yaml() {
        use crate::traits::InferenceKind;
        let yaml = r#"
named_entity_recognition:
  slot: extraction_llm
  fallback: heuristic
claim_extraction:
  slot: extraction_llm
  fallback: heuristic
embedding:
  slot: embedding
  fallback: none
"#;
        let table: StrategyTable = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(table.len(), 3);
        let ner = table.get(&InferenceKind::NamedEntityRecognition).unwrap();
        assert_eq!(ner.slot, SlotName::ExtractionLlm);
        assert_eq!(ner.fallback, FallbackStrategy::Heuristic);
        let embed = table.get(&InferenceKind::Embedding).unwrap();
        assert_eq!(embed.slot, SlotName::Embedding);
        assert_eq!(embed.fallback, FallbackStrategy::None);
    }

    #[test]
    fn missing_model_runtime_field_deserialises_to_disabled() {
        let yaml = r#"
data_root: .objective
api:
  rest_port: 8080
  websocket_port: 8081
  cors_allowed_origins: ["http://localhost:5173"]
  auth_enabled: false
storage:
  database_path: .objective/db
  document_path: .objective/documents
  vector_path: .objective/vectors
  queue_path: .objective/queue
  graph_path: .objective/graph
  embedding_path: .objective/embeddings
message_bus:
  nats_url: nats://127.0.0.1:4222
  use_embedded: true
logging:
  level: info
"#;
        let config: ObjectiveConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.model_runtime, ModelRuntimeConfig::Disabled);
    }
}
