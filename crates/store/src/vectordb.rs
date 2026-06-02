use std::path::Path;

use async_trait::async_trait;
use lancedb::connection::Connection;
use objective_core::{
    traits::{VectorEntry, VectorRepository},
    ObjectiveError, Result,
};
use tracing::info;

pub struct LanceVectorStore {
    _connection: Connection,
    table_name: String,
}

impl LanceVectorStore {
    pub async fn new(path: &Path, table_name: &str) -> Result<Self> {
        let connection = lancedb::connect(path.to_str().unwrap_or("."))
            .execute()
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to connect to LanceDB: {error}"))
            })?;

        let store = Self {
            _connection: connection.clone(),
            table_name: table_name.to_string(),
        };

        store.initialize_table().await?;
        Ok(store)
    }

    async fn initialize_table(&self) -> Result<()> {
        info!("initializing LanceDB vector table: {}", self.table_name);

        let tables = self._connection.table_names().await.map_err(|error| {
            ObjectiveError::Storage(format!("failed to list LanceDB tables: {error}"))
        })?;

        if !tables.contains(&self.table_name) {
            let schema = arrow::datatypes::Schema::new(vec![
                arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Utf8, false),
                arrow::datatypes::Field::new(
                    "vector",
                    arrow::datatypes::DataType::FixedSizeList(
                        Box::new(arrow::datatypes::Field::new("item", arrow::datatypes::DataType::Float32, true)),
                        128,
                    ),
                    false,
                ),
                arrow::datatypes::Field::new(
                    "metadata",
                    arrow::datatypes::DataType::Utf8,
                    true,
                ),
            ]);

            self._connection
                .create_empty_table(&self.table_name, schema.into())
                .execute()
                .await
                .map_err(|error| {
                    ObjectiveError::Storage(format!("failed to create LanceDB table: {error}"))
                })?;

            info!("created LanceDB vector table: {}", self.table_name);
        }

        Ok(())
    }
}

#[async_trait]
impl VectorRepository for LanceVectorStore {
    async fn store_vectors(&self, entries: Vec<VectorEntry>) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let batch = self.create_record_batch(&entries).await?;

        let table = self
            ._connection
            .open_table(&self.table_name)
            .execute()
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to open LanceDB table: {error}"))
            })?;

        table
            .add(batch)
            .execute()
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to add vectors to LanceDB: {error}"))
            })?;

        Ok(())
    }

    async fn search_similar(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorEntry>> {
        let table = self
            ._connection
            .open_table(&self.table_name)
            .execute()
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to open LanceDB table: {error}"))
            })?;

        let results = table
            .vector_search(query_vector)
            .limit(limit)
            .execute()
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to search LanceDB: {error}"))
            })?;

        let mut entries = Vec::new();
        let mut stream = results;
        while let Some(batch) = stream.next().await {
            let batch = batch.map_err(|error| {
                ObjectiveError::Storage(format!("failed to read LanceDB result: {error}"))
            })?;

            for i in 0..batch.num_rows() {
                let id = batch
                    .column_by_name("id")
                    .and_then(|col| col.as_any().downcast_ref::<arrow::array::StringArray>())
                    .map(|arr| arr.value(i).to_string())
                    .unwrap_or_default();

                let metadata_str = batch
                    .column_by_name("metadata")
                    .and_then(|col| col.as_any().downcast_ref::<arrow::array::StringArray>())
                    .map(|arr| arr.value(i))
                    .unwrap_or("{}");

                let metadata: std::collections::HashMap<String, String> =
                    serde_json::from_str(metadata_str).unwrap_or_default();

                entries.push(VectorEntry {
                    id,
                    vector: query_vector.to_vec(),
                    metadata,
                });
            }
        }

        Ok(entries)
    }

    async fn get_vector(&self, id: &str) -> Result<Option<VectorEntry>> {
        let table = self
            ._connection
            .open_table(&self.table_name)
            .execute()
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to open LanceDB table: {error}"))
            })?;

        let results = table
            .query()
            .filter(format!("id = '{id}'"))
            .execute()
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to query LanceDB: {error}"))
            })?;

        let mut stream = results;
        if let Some(batch) = stream.next().await {
            let batch = batch.map_err(|error| {
                ObjectiveError::Storage(format!("failed to read LanceDB result: {error}"))
            })?;

            if batch.num_rows() > 0 {
                let metadata_str = batch
                    .column_by_name("metadata")
                    .and_then(|col| col.as_any().downcast_ref::<arrow::array::StringArray>())
                    .map(|arr| arr.value(0))
                    .unwrap_or("{}");

                let metadata: std::collections::HashMap<String, String> =
                    serde_json::from_str(metadata_str).unwrap_or_default();

                return Ok(Some(VectorEntry {
                    id: id.to_string(),
                    vector: Vec::new(),
                    metadata,
                }));
            }
        }

        Ok(None)
    }

    async fn delete_vector(&self, id: &str) -> Result<()> {
        let table = self
            ._connection
            .open_table(&self.table_name)
            .execute()
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to open LanceDB table: {error}"))
            })?;

        table
            .delete(format!("id = '{id}'"))
            .await
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to delete from LanceDB: {error}"))
            })?;

        Ok(())
    }
}

impl LanceVectorStore {
    async fn create_record_batch(
        &self,
        entries: &[VectorEntry],
    ) -> Result<arrow::record_batch::RecordBatch> {
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        let metadata: Vec<&str> = entries
            .iter()
            .map(|e| serde_json::to_string(&e.metadata).unwrap_or_default())
            .collect::<Vec<String>>()
            .iter()
            .map(|s| s.as_str())
            .collect();

        let vector_dim = entries.first().map(|e| e.vector.len()).unwrap_or(128);
        let vectors: Vec<Vec<f32>> = entries
            .iter()
            .map(|e| {
                let mut v = e.vector.clone();
                v.resize(vector_dim, 0.0);
                v
            })
            .collect();

        let id_array = arrow::array::StringArray::from(ids);
        let metadata_array = arrow::array::StringArray::from(metadata);

        let vector_array = arrow::array::FixedSizeListArray::try_from(
            vectors
                .iter()
                .map(|v| v.as_slice())
                .collect::<Vec<_>>(),
        )
        .map_err(|error| {
            ObjectiveError::Storage(format!("failed to create vector array: {error}"))
        })?;

        let schema = arrow::datatypes::Schema::new(vec![
            arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Utf8, false),
            arrow::datatypes::Field::new(
                "vector",
                arrow::datatypes::DataType::FixedSizeList(
                    Box::new(arrow::datatypes::Field::new(
                        "item",
                        arrow::datatypes::DataType::Float32,
                        true,
                    )),
                    vector_dim as i32,
                ),
                false,
            ),
            arrow::datatypes::Field::new(
                "metadata",
                arrow::datatypes::DataType::Utf8,
                true,
            ),
        ]);

        let batch = arrow::record_batch::RecordBatch::try_new(
            schema.into(),
            vec![
                std::sync::Arc::new(id_array),
                std::sync::Arc::new(vector_array),
                std::sync::Arc::new(metadata_array),
            ],
        )
        .map_err(|error| {
            ObjectiveError::Storage(format!("failed to create record batch: {error}"))
        })?;

        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_lance_vector_store_store_and_search() {
        let tempdir = tempdir().unwrap();
        let store = LanceVectorStore::new(tempdir.path(), "test_vectors")
            .await
            .unwrap();

        let entries = vec![
            VectorEntry {
                id: "1".to_string(),
                vector: vec![1.0, 0.0, 0.0],
                metadata: std::collections::HashMap::new(),
            },
            VectorEntry {
                id: "2".to_string(),
                vector: vec![0.0, 1.0, 0.0],
                metadata: std::collections::HashMap::new(),
            },
        ];

        store.store_vectors(entries).await.unwrap();

        let results = store
            .search_similar(&[1.0, 0.0, 0.0], 10)
            .await
            .unwrap();
        assert!(!results.is_empty());
    }
}
