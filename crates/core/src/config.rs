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
    }
}
