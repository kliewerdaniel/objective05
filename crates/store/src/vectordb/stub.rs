use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use objective_core::{
    traits::{VectorEntry, VectorRepository},
    Result,
};
use tracing::warn;

#[derive(Debug, Default)]
pub struct LanceVectorStore {
    entries: RwLock<HashMap<String, VectorEntry>>,
}

impl LanceVectorStore {
    pub async fn new<P: AsRef<std::path::Path>>(_path: P, _table_name: &str) -> Result<Self> {
        warn!(
            "LanceVectorStore is currently an in-memory stub; the path and table name are ignored. \
             Enable the `lancedb` feature for the real LanceDB-backed implementation."
        );
        Ok(Self::default())
    }

    pub fn is_stub(&self) -> bool {
        true
    }
}

#[async_trait]
impl VectorRepository for LanceVectorStore {
    async fn store_vectors(&self, entries: Vec<VectorEntry>) -> Result<()> {
        let mut guard = self
            .entries
            .write()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?;
        for entry in entries {
            guard.insert(entry.id.clone(), entry);
        }
        Ok(())
    }

    async fn search_similar(&self, query_vector: &[f32], limit: usize) -> Result<Vec<VectorEntry>> {
        let guard = self
            .entries
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?;
        let mut scored: Vec<(f32, VectorEntry)> = guard
            .values()
            .map(|entry| {
                let score = cosine_similarity(&entry.vector, query_vector);
                (score, entry.clone())
            })
            .collect();
        scored.sort_by(|left, right| {
            right
                .0
                .partial_cmp(&left.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(scored
            .into_iter()
            .take(limit)
            .map(|(_, entry)| entry)
            .collect())
    }

    async fn get_vector(&self, id: &str) -> Result<Option<VectorEntry>> {
        Ok(self
            .entries
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .get(id)
            .cloned())
    }

    async fn delete_vector(&self, id: &str) -> Result<()> {
        self.entries
            .write()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .remove(id);
        Ok(())
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entry(id: &str, vector: Vec<f32>) -> VectorEntry {
        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), "test".to_string());
        VectorEntry {
            id: id.to_string(),
            vector,
            metadata,
        }
    }

    #[tokio::test]
    async fn test_lance_stub_stores_and_retrieves_vectors() {
        let tempdir = tempdir().unwrap();
        let store = LanceVectorStore::new(tempdir.path(), "test_vectors")
            .await
            .unwrap();
        assert!(store.is_stub());

        store
            .store_vectors(vec![entry("1", vec![1.0, 0.0, 0.0])])
            .await
            .unwrap();
        let retrieved = store.get_vector("1").await.unwrap().unwrap();
        assert_eq!(retrieved.id, "1");
        assert_eq!(retrieved.vector, vec![1.0, 0.0, 0.0]);
    }

    #[tokio::test]
    async fn test_lance_stub_search_similar_orders_by_similarity() {
        let tempdir = tempdir().unwrap();
        let store = LanceVectorStore::new(tempdir.path(), "test_vectors")
            .await
            .unwrap();

        store
            .store_vectors(vec![
                entry("a", vec![1.0, 0.0, 0.0]),
                entry("b", vec![0.0, 1.0, 0.0]),
                entry("c", vec![0.9, 0.1, 0.0]),
            ])
            .await
            .unwrap();

        let results = store.search_similar(&[1.0, 0.0, 0.0], 2).await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        assert_eq!(results[1].id, "c");
    }

    #[tokio::test]
    async fn test_lance_stub_delete_removes_vector() {
        let tempdir = tempdir().unwrap();
        let store = LanceVectorStore::new(tempdir.path(), "test_vectors")
            .await
            .unwrap();

        store
            .store_vectors(vec![entry("1", vec![1.0, 0.0, 0.0])])
            .await
            .unwrap();
        assert!(store.get_vector("1").await.unwrap().is_some());
        store.delete_vector("1").await.unwrap();
        assert!(store.get_vector("1").await.unwrap().is_none());
    }
}
