use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::types::Float32Type;
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, RecordBatchReader,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::connect;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Table;
use objective_core::{
    traits::{VectorEntry, VectorRepository},
    ObjectiveError, Result,
};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug)]
pub struct LanceVectorStore {
    db_path: std::path::PathBuf,
    table_name: String,
    table: RwLock<Option<Arc<Table>>>,
}

impl LanceVectorStore {
    pub async fn new<P: AsRef<std::path::Path>>(path: P, table_name: &str) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&db_path)
            .map_err(|e| ObjectiveError::Storage(format!("create vector store dir: {e}")))?;
        Ok(Self {
            db_path,
            table_name: table_name.to_string(),
            table: RwLock::new(None),
        })
    }

    pub fn is_stub(&self) -> bool {
        false
    }

    pub async fn create_vector_index(&self) -> Result<()> {
        let table = match self.get_or_open_table().await? {
            Some(t) => t,
            None => return Ok(()),
        };
        table
            .create_index(&["vector"], lancedb::index::Index::Auto)
            .execute()
            .await
            .map_err(|e| ObjectiveError::Storage(format!("lancedb create index: {e}")))?;
        info!(table = %self.table_name, "Created vector index");
        Ok(())
    }

    async fn get_or_open_table(&self) -> Result<Option<Arc<Table>>> {
        {
            let read = self.table.read().await;
            if let Some(ref table) = *read {
                return Ok(Some(Arc::clone(table)));
            }
        }

        let db = connect(self.db_path.to_str().unwrap_or_default())
            .execute()
            .await
            .map_err(|e| ObjectiveError::Storage(format!("lancedb connect: {e}")))?;

        match db.open_table(&self.table_name).execute().await {
            Ok(t) => {
                info!(table = %self.table_name, "Opened existing LanceDB vector table");
                let table = Arc::new(t);
                *self.table.write().await = Some(Arc::clone(&table));
                Ok(Some(table))
            }
            Err(_) => Ok(None),
        }
    }

    fn entries_to_batch(entries: &[VectorEntry], dim: usize) -> Result<RecordBatch> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    dim as i32,
                ),
                false,
            ),
            Field::new("metadata", DataType::Utf8, true),
        ]));

        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        let id_array = StringArray::from(ids);

        let vector_iter = entries
            .iter()
            .map(|e| Some(e.vector.iter().copied().map(Some).collect::<Vec<_>>()));
        let vector_array =
            FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(vector_iter, dim as i32);

        let meta_strings: Vec<String> = entries
            .iter()
            .map(|e| serde_json::to_string(&e.metadata).unwrap_or_else(|_| "{}".to_string()))
            .collect();
        let meta_refs: Vec<&str> = meta_strings.iter().map(|s| s.as_str()).collect();
        let meta_array = StringArray::from(meta_refs);

        RecordBatch::try_new(schema, vec![
            Arc::new(id_array),
            Arc::new(vector_array),
            Arc::new(meta_array),
        ])
        .map_err(|e| ObjectiveError::Storage(format!("lancedb create batch: {e}")))
    }

    fn batch_to_entries(batch: &RecordBatch) -> Result<Vec<VectorEntry>> {
        let id_col = batch
            .column_by_name("id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                ObjectiveError::Storage("missing or invalid id column".to_string())
            })?;

        let vec_col = batch
            .column_by_name("vector")
            .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>())
            .ok_or_else(|| {
                ObjectiveError::Storage("missing or invalid vector column".to_string())
            })?;

        let meta_col = batch
            .column_by_name("metadata")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                ObjectiveError::Storage("missing or invalid metadata column".to_string())
            })?;

        let mut entries = Vec::with_capacity(batch.num_rows());
        for i in 0..batch.num_rows() {
            let id = id_col.value(i).to_string();

            let inner = vec_col.value(i);
            let floats = inner
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or_else(|| ObjectiveError::Storage("invalid vector data".to_string()))?;
            let vector = floats.values().to_vec();

            let metadata: HashMap<String, String> = if meta_col.is_null(i) {
                HashMap::new()
            } else {
                serde_json::from_str(meta_col.value(i)).unwrap_or_default()
            };

            entries.push(VectorEntry { id, vector, metadata });
        }

        Ok(entries)
    }
}

#[async_trait]
impl VectorRepository for LanceVectorStore {
    async fn store_vectors(&self, entries: Vec<VectorEntry>) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let dim = entries[0].vector.len();
        let batch = Self::entries_to_batch(&entries, dim)?;

        match self.get_or_open_table().await? {
            Some(table) => {
                let schema = batch.schema();
                let batches: Vec<arrow_array::RecordBatch> = vec![batch];
                let reader = RecordBatchIterator::new(batches.into_iter().map(Ok), schema);
                let boxed: Box<dyn RecordBatchReader + Send> = Box::new(reader);
                table
                    .add(boxed)
                    .execute()
                    .await
                    .map_err(|e| ObjectiveError::Storage(format!("lancedb add vectors: {e}")))?;
            }
            None => {
                let db = connect(self.db_path.to_str().unwrap_or_default())
                    .execute()
                    .await
                    .map_err(|e| ObjectiveError::Storage(format!("lancedb connect: {e}")))?;
                let table = db
                    .create_table(&self.table_name, batch)
                    .execute()
                    .await
                    .map_err(|e| {
                        ObjectiveError::Storage(format!("lancedb create table: {e}"))
                    })?;
                info!(
                    table = %self.table_name,
                    dim,
                    "Created LanceDB vector table"
                );
                *self.table.write().await = Some(Arc::new(table));
            }
        }

        Ok(())
    }

    async fn search_similar(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorEntry>> {
        let table = match self.get_or_open_table().await? {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let stream = table
            .query()
            .nearest_to(query_vector)
            .map_err(|e| ObjectiveError::Storage(format!("lancedb nearest_to: {e}")))?
            .limit(limit)
            .execute()
            .await
            .map_err(|e| ObjectiveError::Storage(format!("lancedb search: {e}")))?;

        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| ObjectiveError::Storage(format!("lancedb collect results: {e}")))?;

        let mut results = Vec::new();
        for batch in &batches {
            results.extend(Self::batch_to_entries(batch)?);
        }

        Ok(results)
    }

    async fn get_vector(&self, id: &str) -> Result<Option<VectorEntry>> {
        let table = match self.get_or_open_table().await? {
            Some(t) => t,
            None => return Ok(None),
        };

        let escaped = id.replace('\'', "''");
        let filter: &str = &format!("id = '{}'", escaped);
        let stream = table
            .query()
            .only_if(filter)
            .execute()
            .await
            .map_err(|e| ObjectiveError::Storage(format!("lancedb get_vector: {e}")))?;

        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| ObjectiveError::Storage(format!("lancedb collect get_vector: {e}")))?;

        for batch in &batches {
            if batch.num_rows() > 0 {
                let entries = Self::batch_to_entries(batch)?;
                if !entries.is_empty() {
                    return Ok(Some(entries[0].clone()));
                }
            }
        }

        Ok(None)
    }

    async fn delete_vector(&self, id: &str) -> Result<()> {
        let table = match self.get_or_open_table().await? {
            Some(t) => t,
            None => return Ok(()),
        };

        let escaped = id.replace('\'', "''");
        let predicate: &str = &format!("id = '{}'", escaped);
        table
            .delete(predicate)
            .await
            .map_err(|e| ObjectiveError::Storage(format!("lancedb delete_vector: {e}")))?;

        Ok(())
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
    async fn test_lance_db_stores_and_retrieves_vectors() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("lancedb");
        let store = LanceVectorStore::new(&path, "test_vectors")
            .await
            .unwrap();
        assert!(!store.is_stub());

        store
            .store_vectors(vec![entry("1", vec![1.0, 0.0, 0.0])])
            .await
            .unwrap();
        let retrieved = store.get_vector("1").await.unwrap().unwrap();
        assert_eq!(retrieved.id, "1");
        assert_eq!(retrieved.vector, vec![1.0, 0.0, 0.0]);
        assert_eq!(
            retrieved.metadata.get("source").map(|s| s.as_str()),
            Some("test")
        );
    }

    #[tokio::test]
    async fn test_lance_db_search_similar_orders_by_similarity() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("lancedb");
        let store = LanceVectorStore::new(&path, "test_vectors")
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
    async fn test_lance_db_delete_removes_vector() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("lancedb");
        let store = LanceVectorStore::new(&path, "test_vectors")
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

    #[tokio::test]
    async fn test_lance_db_empty_table_returns_empty_results() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("lancedb");
        let store = LanceVectorStore::new(&path, "empty_test")
            .await
            .unwrap();

        let results = store.search_similar(&[1.0, 0.0, 0.0], 5).await.unwrap();
        assert!(results.is_empty());
        assert!(store.get_vector("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_lance_db_reopened_survives_restart() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().join("lancedb");

        let store = LanceVectorStore::new(&path, "persist_test")
            .await
            .unwrap();
        store
            .store_vectors(vec![entry("persist", vec![0.5, 0.5, 0.5])])
            .await
            .unwrap();
        drop(store);

        let reopened = LanceVectorStore::new(&path, "persist_test")
            .await
            .unwrap();
        let retrieved = reopened.get_vector("persist").await.unwrap().unwrap();
        assert_eq!(retrieved.id, "persist");
    }
}
