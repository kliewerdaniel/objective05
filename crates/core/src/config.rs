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
    /// Phase 3 will add `extraction_llm` (llama.cpp).
    #[serde(default)]
    pub models: ModelSlots,
    /// Context window in tokens. Defaults to 4096.
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    /// Maximum concurrent inference requests. Defaults to 1.
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: u32,
    /// Per-chunk timeout in milliseconds. Defaults to 30s.
    #[serde(default = "default_chunk_timeout_ms")]
    pub chunk_timeout_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSlots {
    /// ONNX-backed embedding slot. Phase 2 only.
    pub embedding: Option<EmbeddingSlot>,
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

fn default_context_window() -> u32 {
    4096
}

fn default_max_concurrency() -> u32 {
    1
}

fn default_chunk_timeout_ms() -> u64 {
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
                write!(
                    f,
                    "local({}ms, ctx={}, conc={}, embed={})",
                    cfg.chunk_timeout_ms, cfg.context_window, cfg.max_concurrency, embedding
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
            context_window: 4096,
            max_concurrency: 1,
            chunk_timeout_ms: 30_000,
        });
        assert!(local.requires_runtime());
    }

    #[test]
    fn chunk_timeout_matches_configured_value() {
        let disabled = ModelRuntimeConfig::Disabled;
        assert!(disabled.chunk_timeout().is_none());

        let local = ModelRuntimeConfig::Local(LocalModelConfig {
            models: ModelSlots::default(),
            context_window: 4096,
            max_concurrency: 2,
            chunk_timeout_ms: 5_000,
        });
        assert_eq!(
            local.chunk_timeout(),
            Some(std::time::Duration::from_millis(5_000))
        );
    }

    #[test]
    fn display_is_stable_for_log_lines() {
        assert_eq!(ModelRuntimeConfig::Disabled.to_string(), "disabled");
        assert_eq!(ModelRuntimeConfig::Heuristic.to_string(), "heuristic");
        assert_eq!(
            ModelRuntimeConfig::Local(LocalModelConfig {
                models: ModelSlots::default(),
                context_window: 4096,
                max_concurrency: 1,
                chunk_timeout_ms: 5_000,
            })
            .to_string(),
            "local(5000ms, ctx=4096, conc=1, embed=off)"
        );
        assert_eq!(
            ModelRuntimeConfig::Local(LocalModelConfig {
                models: ModelSlots {
                    embedding: Some(EmbeddingSlot {
                        path: PathBuf::from(".objective/models/bge-small-en-v1.5/model.onnx"),
                        dimension: 384,
                    }),
                },
                context_window: 4096,
                max_concurrency: 1,
                chunk_timeout_ms: 5_000,
            })
            .to_string(),
            "local(5000ms, ctx=4096, conc=1, embed=on)"
        );
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
