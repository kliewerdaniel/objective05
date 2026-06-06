//! `ModelIndex` — the embedding sidecar attached to an
//! `ExtractionResult`. The sidecar exists so the existing
//! `LanceDB` stub and the future `LanceDB`-backed repository
//! can consume vector output from a `ModelRuntime` without
//! having to know about the runtime's wire format.
//!
//! The runtime returns the embedding in
//! `InferenceResult::structured` as a JSON `Vec<f32>` (the v1
//! contract). The orchestrator decodes that and builds a
//! [`ModelIndex`] keyed on the document id and the chunk index.
//! `ModelIndex::into_vector_entries` adapts the sidecar to the
//! existing `VectorRepository::store_vectors` API.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::traits::{ModelId, VectorEntry};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ModelIndex {
    /// Unique sidecar identifier. The orchestrator uses
    /// `format!("{document_id}::embeddings")` by default.
    pub id: String,
    /// Which model produced these vectors. Matches the
    /// `ModelId` from the runtime that emitted them.
    pub model: ModelId,
    /// Asserted embedding dimension. The runtime reports this
    /// when it loads the model; a mismatch at decode time is a
    /// `ModelError::Backend` and triggers the per-chunk
    /// fallback.
    pub dimension: u32,
    /// One entry per source chunk, in chunk order. The
    /// `chunk_index` matches the index in
    /// `RuntimeExtractionService::chunk_body`.
    pub vectors: Vec<ModelVector>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ModelVector {
    pub chunk_index: usize,
    pub text_snippet: String,
    pub embedding: Vec<f32>,
}

impl ModelIndex {
    pub fn new(id: impl Into<String>, model: ModelId, dimension: u32) -> Self {
        Self {
            id: id.into(),
            model,
            dimension,
            vectors: Vec::new(),
        }
    }

    pub fn push(&mut self, vector: ModelVector) {
        self.vectors.push(vector);
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Adapt the sidecar to the existing `VectorRepository`
    /// surface. Metadata carries the model name and the
    /// document id so callers can filter on either.
    pub fn into_vector_entries(self, document_id: &str) -> Vec<VectorEntry> {
        self.vectors
            .into_iter()
            .map(|vector| {
                let mut metadata = HashMap::new();
                metadata.insert("document_id".to_string(), document_id.to_string());
                metadata.insert("chunk_index".to_string(), vector.chunk_index.to_string());
                metadata.insert("model".to_string(), self.model.as_str().to_string());
                metadata.insert("snippet".to_string(), vector.text_snippet);
                VectorEntry {
                    id: format!("{}::{}::{}", document_id, self.id, vector.chunk_index),
                    vector: vector.embedding,
                    metadata,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_index_default_is_empty() {
        let index = ModelIndex::new("test", ModelId::BgeSmallEnV15, 384);
        assert_eq!(index.id, "test");
        assert_eq!(index.dimension, 384);
        assert!(index.is_empty());
    }

    #[test]
    fn into_vector_entries_assigns_unique_ids() {
        let mut index = ModelIndex::new("embeddings", ModelId::BgeSmallEnV15, 4);
        index.push(ModelVector {
            chunk_index: 0,
            text_snippet: "first".to_string(),
            embedding: vec![0.1, 0.2, 0.3, 0.4],
        });
        index.push(ModelVector {
            chunk_index: 1,
            text_snippet: "second".to_string(),
            embedding: vec![0.5, 0.6, 0.7, 0.8],
        });

        let entries = index.into_vector_entries("doc-1");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "doc-1::embeddings::0");
        assert_eq!(entries[1].id, "doc-1::embeddings::1");
        assert_eq!(entries[0].metadata.get("document_id").map(String::as_str), Some("doc-1"));
        assert_eq!(entries[1].metadata.get("chunk_index").map(String::as_str), Some("1"));
    }
}
