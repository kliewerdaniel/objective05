use std::path::PathBuf;

use objective_core::{ObjectiveError, Result};
use tokio::sync::RwLock;

use crate::types::AudioRecord;

#[async_trait::async_trait]
pub trait AudioRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<AudioRecord>>;
    async fn get(&self, id: &str) -> Result<Option<AudioRecord>>;
    async fn insert(&self, record: AudioRecord) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<bool>;
}

pub struct FileAudioRepository {
    path: PathBuf,
    cache: RwLock<Vec<AudioRecord>>,
}

impl FileAudioRepository {
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
            .map_err(|e| ObjectiveError::Storage(format!("serialize audio records: {e}")))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ObjectiveError::Storage(format!("create audio dir: {e}")))?;
        }
        std::fs::write(&self.path, json)
            .map_err(|e| ObjectiveError::Storage(format!("write audio records: {e}")))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AudioRepository for FileAudioRepository {
    async fn list(&self) -> Result<Vec<AudioRecord>> {
        let mut records = self.cache.read().await.clone();
        records.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(records)
    }

    async fn get(&self, id: &str) -> Result<Option<AudioRecord>> {
        let records = self.cache.read().await;
        Ok(records.iter().find(|r| r.id == id).cloned())
    }

    async fn insert(&self, record: AudioRecord) -> Result<()> {
        self.cache.write().await.push(record);
        self.persist().await
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let mut guard = self.cache.write().await;
        let len_before = guard.len();
        guard.retain(|r| r.id != id);
        let deleted = guard.len() < len_before;
        if deleted {
            self.persist().await?;
        }
        Ok(deleted)
    }
}
