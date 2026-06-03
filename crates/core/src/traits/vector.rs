use async_trait::async_trait;

use crate::Result;

#[derive(Debug, Clone)]
pub struct VectorEntry {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: std::collections::HashMap<String, String>,
}

#[async_trait]
pub trait VectorRepository: Send + Sync {
    async fn store_vectors(&self, entries: Vec<VectorEntry>) -> Result<()>;
    async fn search_similar(&self, query_vector: &[f32], limit: usize) -> Result<Vec<VectorEntry>>;
    async fn get_vector(&self, id: &str) -> Result<Option<VectorEntry>>;
    async fn delete_vector(&self, id: &str) -> Result<()>;
}
