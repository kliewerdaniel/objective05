use std::path::PathBuf;

use async_trait::async_trait;
use objective_core::{ObjectiveError, Result};
use tokio::sync::RwLock;

use crate::types::BroadcastRecord;

#[async_trait]
pub trait BroadcastRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<BroadcastRecord>>;
    async fn latest(&self) -> Result<Option<BroadcastRecord>>;
    async fn get(&self, id: &str) -> Result<Option<BroadcastRecord>>;
    async fn insert(&self, record: BroadcastRecord) -> Result<()>;
}

pub struct FileBroadcastRepository {
    path: PathBuf,
    cache: RwLock<Vec<BroadcastRecord>>,
}

impl FileBroadcastRepository {
    pub fn new(path: PathBuf) -> Self {
        let records = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|data| serde_json::from_str(&data).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            path,
            cache: RwLock::new(records),
        }
    }

    async fn persist(&self) -> Result<()> {
        let data = self.cache.read().await;
        let json = serde_json::to_string_pretty(&*data)
            .map_err(|e| ObjectiveError::Storage(format!("serialize broadcasts: {e}")))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ObjectiveError::Storage(format!("create broadcast dir: {e}")))?;
        }
        std::fs::write(&self.path, json)
            .map_err(|e| ObjectiveError::Storage(format!("write broadcasts: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl BroadcastRepository for FileBroadcastRepository {
    async fn list(&self) -> Result<Vec<BroadcastRecord>> {
        let mut records = self.cache.read().await.clone();
        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(records)
    }

    async fn latest(&self) -> Result<Option<BroadcastRecord>> {
        let records = self.cache.read().await;
        Ok(records.iter().max_by(|a, b| a.created_at.cmp(&b.created_at)).cloned())
    }

    async fn get(&self, id: &str) -> Result<Option<BroadcastRecord>> {
        let records = self.cache.read().await;
        Ok(records.iter().find(|r| r.id == id).cloned())
    }

    async fn insert(&self, record: BroadcastRecord) -> Result<()> {
        self.cache.write().await.push(record);
        self.persist().await
    }
}
