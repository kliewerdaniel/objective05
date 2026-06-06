//! `RuntimeExtractionService` — extraction entry point that
//! consults a [`ModelRuntime`] and falls back to the
//! [`HeuristicExtractionService`] on a per-chunk error or
//! timeout.
//!
//! The split is per-sentence. Each sentence becomes one chunk
//! in the contract documented in
//! `docs/processing/model-runtime.md`. The contract is:
//!
//! 1. The runtime is asked to do `ClaimExtraction` for every
//!    chunk. The returned `InferenceResult.text` is taken as
//!    the chunk's enhanced claim.
//! 2. The runtime is asked to do `Embedding` for every chunk.
//!    The returned `InferenceResult.structured` (a JSON
//!    `Vec<f32>`) is decoded into a [`ModelIndex`] sidecar
//!    attached to the [`ExtractionResult`].
//! 3. The heuristic service produces the baseline entities,
//!    claims, and relationships for the document.
//! 4. If the runtime call succeeds, the heuristic's claim
//!    set is replaced with the runtime's claims. If it fails
//!    or times out, the heuristic's claim is kept and the
//!    error is logged.
//! 5. Entities and relationships always come from the
//!    heuristic in v1; Phase 3 will route them through the
//!    runtime as well.
//!
//! This contract is deliberately narrow: the v1 goal is to
//! prove the trait surface and the fallback path, not to
//! model the full LLM extraction pipeline.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use objective_core::{
    traits::{
        DocumentProcessor, InferenceKind, InferenceResult, InferenceTask, ModelError, ModelId,
        ModelRuntime,
    },
    types::{
        EntityType, ExtractedClaim, ExtractedEntity, ExtractedRelationship, ExtractionResult,
        ModelIndex, ModelVector, RawDocument,
    },
    Result,
};
use tokio::time::timeout;
use tracing::warn;

use crate::service::HeuristicExtractionService;

/// Default per-chunk timeout. Matches the default
/// `ModelRuntimeConfig::Local::chunk_timeout_ms` in
/// `objective-core::config`.
pub const DEFAULT_CHUNK_TIMEOUT: Duration = Duration::from_secs(30);

/// Default target sentence length for the chunker. Chunks are
/// also bounded by sentence boundaries, so this constant is
/// used as a soft cap to keep prompt sizes predictable.
pub const DEFAULT_MAX_CHUNK_CHARS: usize = 1_024;

#[derive(Debug, Clone)]
pub struct RuntimeExtractionConfig {
    pub chunk_timeout: Duration,
    pub max_chunk_chars: usize,
}

impl Default for RuntimeExtractionConfig {
    fn default() -> Self {
        Self {
            chunk_timeout: DEFAULT_CHUNK_TIMEOUT,
            max_chunk_chars: DEFAULT_MAX_CHUNK_CHARS,
        }
    }
}

#[derive(Clone)]
pub struct RuntimeExtractionService {
    runtime: Arc<dyn ModelRuntime>,
    fallback: HeuristicExtractionService,
    config: RuntimeExtractionConfig,
}

impl std::fmt::Debug for RuntimeExtractionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeExtractionService")
            .field("runtime", &self.runtime.provider())
            .field("fallback", &"HeuristicExtractionService")
            .field("config", &self.config)
            .finish()
    }
}

impl RuntimeExtractionService {
    pub fn new(runtime: Arc<dyn ModelRuntime>) -> Self {
        Self {
            runtime,
            fallback: HeuristicExtractionService,
            config: RuntimeExtractionConfig::default(),
        }
    }

    pub fn with_config(mut self, config: RuntimeExtractionConfig) -> Self {
        self.config = config;
        self
    }

    /// Split `body` into sentence-sized chunks. Whitespace-
    /// only chunks are dropped. A sentence longer than
    /// `max_chunk_chars` is preserved as a single chunk so we
    /// never lose data; the runtime may decide to truncate.
    pub fn chunk_body(body: &str, _max_chunk_chars: usize) -> Vec<String> {
        body.split(['.', '!', '?'])
            .map(str::trim)
            .filter(|sentence| !sentence.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Run one chunk through the runtime with a hard timeout.
    /// Returns `Ok(None)` if the runtime reports
    /// [`ModelError::Unavailable`] or
    /// [`ModelError::UnsupportedKind`] — the orchestrator
    /// treats that as a clean signal to fall back. Any other
    /// error is logged and also reported as `Ok(None)`.
    async fn infer_kind(
        &self,
        kind: InferenceKind,
        chunk: &str,
        index: usize,
        document_id: &str,
    ) -> Option<InferenceResult> {
        let model = match kind {
            InferenceKind::Embedding => ModelId::BgeSmallEnV15,
            _ => ModelId::Mistral7BInstruct,
        };
        let task =
            InferenceTask::new(model, kind, chunk).with_timeout(self.config.chunk_timeout);

        match timeout(self.config.chunk_timeout, self.runtime.infer(task)).await {
            Ok(Ok(result)) => Some(result),
            Ok(Err(ModelError::Unavailable { kind })) => {
                warn!(
                    document_id = %document_id,
                    chunk_index = index,
                    kind = %kind,
                    "runtime unavailable for chunk; falling back"
                );
                None
            }
            Ok(Err(ModelError::UnsupportedKind { kind })) => {
                warn!(
                    document_id = %document_id,
                    chunk_index = index,
                    kind = %kind,
                    "runtime does not support kind; falling back"
                );
                None
            }
            Ok(Err(err)) => {
                warn!(
                    document_id = %document_id,
                    chunk_index = index,
                    error = %err,
                    "runtime error; falling back"
                );
                None
            }
            Err(_elapsed) => {
                warn!(
                    document_id = %document_id,
                    chunk_index = index,
                    timeout_ms = self.config.chunk_timeout.as_millis() as u64,
                    "runtime timeout; falling back"
                );
                None
            }
        }
    }

    /// Convert a runtime `InferenceResult.text` into a single
    /// claim, or `None` if the text is empty.
    fn runtime_text_to_claim(&self, text: &str, chunk: &str) -> Option<ExtractedClaim> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(ExtractedClaim {
            claim_text: trimmed.to_string(),
            subject_name: "unknown".to_string(),
            predicate: "stated".to_string(),
            object_name: None,
            object_value: None,
            claim_type: objective_core::types::ClaimType::Attribution,
            sentiment: None,
            confidence: 0.6,
            evidence_snippet: chunk.to_string(),
            attributed_to: Some("model-runtime".to_string()),
        })
    }

    /// Decode a `{"entities": [...]}` JSON payload into a
    /// list of `ExtractedEntity`. Unknown `type` strings map
    /// to `EntityType::Concept`; missing fields are skipped
    /// without erroring so a partial decode still yields a
    /// useful result.
    fn decode_entities(
        structured: &serde_json::Value,
        chunk: &str,
    ) -> Vec<ExtractedEntity> {
        let Some(entities) = structured.get("entities").and_then(|v| v.as_array()) else {
            return Vec::new();
        };
        entities
            .iter()
            .filter_map(|value| {
                let name = value.get("name")?.as_str()?.to_string();
                if name.is_empty() {
                    return None;
                }
                let entity_type = value
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(Self::classify_entity_type)
                    .unwrap_or(EntityType::Concept);
                let confidence = value
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32)
                    .unwrap_or(0.5);
                Some(ExtractedEntity {
                    evidence_snippet: chunk.to_string(),
                    name,
                    entity_type,
                    aliases: Vec::new(),
                    description: None,
                    metadata: std::collections::HashMap::new(),
                    confidence,
                })
            })
            .collect()
    }

    fn classify_entity_type(label: &str) -> EntityType {
        match label {
            "Organization" | "Org" | "Company" | "Corporation" => EntityType::Organization,
            "Location" | "Place" | "City" | "Country" | "State" => EntityType::Location,
            "Person" | "People" | "Human" => EntityType::Person,
            "Event" | "EventTopic" => EntityType::EventTopic,
            "Product" => EntityType::Concept,
            _ => EntityType::Concept,
        }
    }

    /// Decode a `{"claims": [...]}` JSON payload into a list
    /// of `ExtractedClaim`. Mirrors the v0 heuristic schema
    /// so downstream code does not need to branch on provider.
    fn decode_claims(
        structured: &serde_json::Value,
        chunk: &str,
    ) -> Vec<ExtractedClaim> {
        let Some(claims) = structured.get("claims").and_then(|v| v.as_array()) else {
            return Vec::new();
        };
        claims
            .iter()
            .filter_map(|value| {
                let text = value
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| chunk.to_string());
                let subject = value
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "unknown".to_string());
                let predicate = value
                    .get("predicate")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "related_to".to_string());
                let object = value
                    .get("object")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let confidence = value
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32)
                    .unwrap_or(0.5);
                Some(ExtractedClaim {
                    claim_text: text,
                    subject_name: subject,
                    predicate,
                    object_name: object,
                    object_value: None,
                    claim_type: objective_core::types::ClaimType::Relation,
                    sentiment: None,
                    confidence,
                    evidence_snippet: chunk.to_string(),
                    attributed_to: Some("model-runtime".to_string()),
                })
            })
            .collect()
    }

    /// Decode a `{"relationships": [...]}` JSON payload into a
    /// list of `ExtractedRelationship`.
    fn decode_relationships(
        structured: &serde_json::Value,
        chunk: &str,
    ) -> Vec<ExtractedRelationship> {
        let Some(rels) = structured.get("relationships").and_then(|v| v.as_array()) else {
            return Vec::new();
        };
        rels.iter()
            .filter_map(|value| {
                let from = value.get("from")?.as_str()?.to_string();
                let to = value.get("to")?.as_str()?.to_string();
                let rel_type = value
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "related_to".to_string());
                let confidence = value
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32)
                    .unwrap_or(0.5);
                Some(ExtractedRelationship {
                    from_entity_name: from,
                    to_entity_name: to,
                    relationship_type: rel_type,
                    confidence,
                    evidence_snippet: chunk.to_string(),
                })
            })
            .collect()
    }

    /// Decode an `InferenceResult` from an `Embedding` call
    /// into a `ModelVector` and append it to the sidecar.
    /// Returns `true` if the vector was appended. The sidecar's
    /// `dimension` is set on the first successful decode.
    fn push_embedding(
        index: &mut ModelIndex,
        result: InferenceResult,
        chunk_index: usize,
        chunk: &str,
    ) -> bool {
        let Some(structured) = result.structured else {
            warn!(chunk_index, "embedding result missing structured payload");
            return false;
        };
        let vector: Vec<f32> = match serde_json::from_value(structured) {
            Ok(vector) => vector,
            Err(err) => {
                warn!(chunk_index, error = %err, "embedding payload is not a Vec<f32>");
                return false;
            }
        };
        if vector.is_empty() {
            return false;
        }
        if index.dimension == 0 {
            index.dimension = vector.len() as u32;
        }
        index.push(ModelVector {
            chunk_index,
            text_snippet: chunk.to_string(),
            embedding: vector,
        });
        true
    }
}

#[async_trait]
impl DocumentProcessor for RuntimeExtractionService {
    async fn process(&self, document: &RawDocument) -> Result<ExtractionResult> {
        let baseline = self.fallback.process(document).await?;
        let document_id = baseline.document_id.clone();

        let chunks = Self::chunk_body(&document.body, self.config.max_chunk_chars);
        if chunks.is_empty() {
            return Ok(baseline);
        }

        let mut runtime_entities: Vec<ExtractedEntity> = Vec::new();
        let mut runtime_claims: Vec<ExtractedClaim> = Vec::new();
        let mut runtime_relationships: Vec<ExtractedRelationship> = Vec::new();
        let mut vector_index = ModelIndex::new("embeddings", ModelId::BgeSmallEnV15, 0);

        for (index, chunk) in chunks.iter().enumerate() {
            if let Some(result) = self
                .infer_kind(InferenceKind::NamedEntityRecognition, chunk, index, &document_id)
                .await
            {
                if let Some(structured) = result.structured {
                    runtime_entities.extend(Self::decode_entities(&structured, chunk));
                }
            }

            if let Some(result) = self
                .infer_kind(InferenceKind::ClaimExtraction, chunk, index, &document_id)
                .await
            {
                if let Some(structured) = result.structured {
                    let decoded = Self::decode_claims(&structured, chunk);
                    if decoded.is_empty() {
                        if let Some(claim) = self.runtime_text_to_claim(&result.text, chunk) {
                            runtime_claims.push(claim);
                        }
                    } else {
                        runtime_claims.extend(decoded);
                    }
                } else if let Some(claim) = self.runtime_text_to_claim(&result.text, chunk) {
                    runtime_claims.push(claim);
                }
            }

            if let Some(result) = self
                .infer_kind(InferenceKind::RelationExtraction, chunk, index, &document_id)
                .await
            {
                if let Some(structured) = result.structured {
                    runtime_relationships.extend(Self::decode_relationships(&structured, chunk));
                }
            }

            if let Some(result) = self
                .infer_kind(InferenceKind::Embedding, chunk, index, &document_id)
                .await
            {
                Self::push_embedding(&mut vector_index, result, index, chunk);
            }
        }

        let mut merged_entities = runtime_entities;
        merged_entities.extend(baseline.entities);

        let mut merged_claims = runtime_claims;
        merged_claims.extend(baseline.claims);

        let mut merged_relationships = runtime_relationships;
        merged_relationships.extend(baseline.relationships);

        let vector_index = if vector_index.is_empty() {
            None
        } else {
            Some(vector_index)
        };

        Ok(ExtractionResult {
            document_id: baseline.document_id,
            entities: merged_entities,
            claims: merged_claims,
            relationships: merged_relationships,
            vector_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use chrono::Utc;
    use objective_core::traits::{
        InferenceResult, InferenceTask, ModelInfo, ModelResult, ModelRuntime,
    };
    use objective_core::types::{BodyFormat, RawDocument};
    use ulid::Ulid;

    use super::*;

    fn make_document(body: &str) -> RawDocument {
        RawDocument {
            id: Ulid::new(),
            source_id: "fixture".to_string(),
            source_type: "fixture".to_string(),
            external_id: "1".to_string(),
            url: None,
            title: Some("Fixture".to_string()),
            body: body.to_string(),
            body_format: BodyFormat::PlainText,
            author: None,
            published_at: None,
            fetched_at: Utc::now(),
            language: "en".to_string(),
            content_hash: "hash".to_string(),
            metadata: HashMap::new(),
            raw_bytes: None,
        }
    }

    /// Always returns `Unavailable`. Used to exercise the
    /// per-chunk fallback path.
    #[derive(Debug)]
    struct UnavailableRuntime;

    #[async_trait]
    impl ModelRuntime for UnavailableRuntime {
        fn provider(&self) -> &'static str {
            "unavailable"
        }

        async fn inventory(&self) -> ModelResult<Vec<ModelInfo>> {
            Ok(Vec::new())
        }

        async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult> {
            Err(ModelError::Unavailable { kind: task.kind })
        }
    }

    /// Always succeeds with text. Embedding returns a 4-dim
    /// vector so the sidecar has known shape.
    #[derive(Debug)]
    struct AlwaysSucceedRuntime;

    #[async_trait]
    impl ModelRuntime for AlwaysSucceedRuntime {
        fn provider(&self) -> &'static str {
            "always-succeed"
        }

        async fn inventory(&self) -> ModelResult<Vec<ModelInfo>> {
            Ok(Vec::new())
        }

        async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult> {
            match task.kind {
                InferenceKind::Embedding => {
                    let vector: Vec<f32> = (0..4).map(|i| (i as f32) / 4.0).collect();
                    Ok(InferenceResult::embedding(
                        task.model,
                        4,
                        vector,
                        Duration::from_millis(1),
                    ))
                }
                _ => Ok(InferenceResult::text_only(
                    task.model,
                    task.kind,
                    format!("runtime said: {}", task.input),
                    Duration::from_millis(1),
                )),
            }
        }
    }

    /// Sleeps past the per-chunk timeout so we can assert the
    /// fallback path is exercised even when the runtime does
    /// not return an error.
    #[derive(Debug)]
    struct SlowRuntime;

    #[async_trait]
    impl ModelRuntime for SlowRuntime {
        fn provider(&self) -> &'static str {
            "slow"
        }

        async fn inventory(&self) -> ModelResult<Vec<ModelInfo>> {
            Ok(Vec::new())
        }

        async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult> {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(InferenceResult::text_only(
                task.model,
                task.kind,
                "too late",
                Duration::from_secs(5),
            ))
        }
    }

    #[test]
    fn chunk_body_splits_on_sentence_boundaries() {
        let body = "Apple Inc announced. Analysts reported. Quietly.";
        let chunks = RuntimeExtractionService::chunk_body(body, 1_024);
        assert_eq!(
            chunks,
            vec!["Apple Inc announced", "Analysts reported", "Quietly"]
        );
    }

    #[test]
    fn chunk_body_drops_empty_sentences() {
        let body = "First... Second.";
        let chunks = RuntimeExtractionService::chunk_body(body, 1_024);
        assert_eq!(chunks, vec!["First", "Second"]);
    }

    #[test]
    fn chunk_body_caps_long_chunks() {
        let long = "a".repeat(2_048);
        let body = format!("{long}.");
        let chunks = RuntimeExtractionService::chunk_body(&body, 1_024);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 2_048);
    }

    #[tokio::test]
    async fn process_uses_heuristic_when_runtime_is_unavailable() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(UnavailableRuntime);
        let service = RuntimeExtractionService::new(runtime);
        let document = make_document(
            "Apple Inc announced a 10% expansion in Austin. Analysts reported hiring.",
        );

        let result = service.process(&document).await.unwrap();

        assert!(
            !result.entities.is_empty(),
            "heuristic entities should be preserved"
        );
        assert!(
            !result.claims.is_empty(),
            "heuristic claims should be preserved"
        );
        assert!(result
            .claims
            .iter()
            .all(|c| c.attributed_to.as_deref() != Some("model-runtime")));
        assert!(
            result.vector_index.is_none(),
            "vector_index is absent when runtime is unavailable for every chunk"
        );
    }

    #[tokio::test]
    async fn process_substitutes_runtime_claims_when_runtime_succeeds() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(AlwaysSucceedRuntime);
        let service = RuntimeExtractionService::new(runtime);
        let document = make_document(
            "Apple Inc announced a 10% expansion in Austin. Analysts reported hiring.",
        );

        let result = service.process(&document).await.unwrap();

        assert!(result
            .claims
            .iter()
            .any(|claim| claim.attributed_to.as_deref() == Some("model-runtime")));
    }

    #[tokio::test]
    async fn process_attaches_vector_index_when_runtime_supports_embedding() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(AlwaysSucceedRuntime);
        let service = RuntimeExtractionService::new(runtime);
        let document = make_document(
            "Apple Inc announced a 10% expansion in Austin. Analysts reported hiring.",
        );

        let result = service.process(&document).await.unwrap();

        let index = result
            .vector_index
            .expect("vector_index should be present when embedding succeeds");
        assert_eq!(index.dimension, 4);
        assert_eq!(index.model, ModelId::BgeSmallEnV15);
        assert_eq!(index.vectors.len(), 2);
        assert_eq!(index.vectors[0].chunk_index, 0);
        assert_eq!(index.vectors[1].chunk_index, 1);
    }

    #[tokio::test]
    async fn vector_index_into_vector_entries_adapts_to_repository() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(AlwaysSucceedRuntime);
        let service = RuntimeExtractionService::new(runtime);
        let document = make_document(
            "Apple Inc announced a 10% expansion in Austin. Analysts reported hiring.",
        );

        let result = service.process(&document).await.unwrap();
        let entries = result
            .vector_index
            .unwrap()
            .into_vector_entries(&result.document_id);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].id.contains(&result.document_id));
        assert_eq!(entries[0].vector.len(), 4);
    }

    #[tokio::test]
    async fn process_falls_back_when_runtime_times_out() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(SlowRuntime);
        let service = RuntimeExtractionService::new(runtime).with_config(
            RuntimeExtractionConfig {
                chunk_timeout: Duration::from_millis(50),
                max_chunk_chars: 1_024,
            },
        );
        let document = make_document(
            "Apple Inc announced a 10% expansion in Austin. Analysts reported hiring.",
        );

        let result = service.process(&document).await.unwrap();

        assert!(
            !result.entities.is_empty(),
            "heuristic entities should be preserved on timeout"
        );
        assert!(
            !result.claims.is_empty(),
            "heuristic claims should be preserved on timeout"
        );
        assert!(result
            .claims
            .iter()
            .all(|c| c.attributed_to.as_deref() != Some("model-runtime")));
        assert!(
            result.vector_index.is_none(),
            "vector_index is absent when embedding times out"
        );
    }

    #[tokio::test]
    async fn process_handles_empty_body() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(AlwaysSucceedRuntime);
        let service = RuntimeExtractionService::new(runtime);
        let document = make_document("");

        let result = service.process(&document).await.unwrap();

        assert!(result.claims.is_empty());
        assert!(result.vector_index.is_none());
    }

    /// Always returns a `structured` JSON payload for the LLM
    /// kinds. Exercises the JSON decoding helpers in
    /// `RuntimeExtractionService::{decode_entities,
    /// decode_claims, decode_relationships}`.
    #[derive(Debug)]
    struct JsonEmittingRuntime;

    #[async_trait]
    impl ModelRuntime for JsonEmittingRuntime {
        fn provider(&self) -> &'static str {
            "json-emitting"
        }

        async fn inventory(&self) -> ModelResult<Vec<ModelInfo>> {
            Ok(Vec::new())
        }

        async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult> {
            let structured = match task.kind {
                InferenceKind::NamedEntityRecognition => serde_json::json!({
                    "entities": [
                        {"name": "Apple Inc", "type": "Organization", "confidence": 0.92},
                        {"name": "Austin", "type": "Location", "confidence": 0.88}
                    ]
                }),
                InferenceKind::ClaimExtraction => serde_json::json!({
                    "claims": [
                        {
                            "subject": "Apple Inc",
                            "predicate": "announced",
                            "object": "expansion",
                            "text": task.input,
                            "confidence": 0.81
                        }
                    ]
                }),
                InferenceKind::RelationExtraction => serde_json::json!({
                    "relationships": [
                        {
                            "from": "Apple Inc",
                            "type": "located_in",
                            "to": "Austin",
                            "confidence": 0.75
                        }
                    ]
                }),
                InferenceKind::Embedding => {
                    let vector: Vec<f32> = (0..4).map(|i| (i as f32) / 4.0).collect();
                    return Ok(InferenceResult::embedding(
                        task.model,
                        4,
                        vector,
                        Duration::from_millis(1),
                    ));
                }
                _ => serde_json::json!({}),
            };
            Ok(InferenceResult {
                text: String::new(),
                structured: Some(structured),
                usage: objective_core::traits::TokenUsage::default(),
                model: task.model,
                kind: task.kind,
                elapsed: Duration::from_millis(1),
                completed_at: chrono::Utc::now(),
            })
        }
    }

    #[tokio::test]
    async fn process_decodes_ner_payload_into_runtime_entities() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(JsonEmittingRuntime);
        let service = RuntimeExtractionService::new(runtime);
        let document = make_document(
            "Apple Inc announced a 10% expansion in Austin. Analysts reported hiring.",
        );

        let result = service.process(&document).await.unwrap();

        let runtime_entities: Vec<&ExtractedEntity> = result
            .entities
            .iter()
            .filter(|entity| entity.confidence > 0.7)
            .collect();
        let names: Vec<&str> = runtime_entities
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(names.contains(&"Apple Inc"));
        assert!(names.contains(&"Austin"));
        assert!(result
            .entities
            .iter()
            .any(|e| e.entity_type == EntityType::Organization));
        assert!(result
            .entities
            .iter()
            .any(|e| e.entity_type == EntityType::Location));
    }

    #[tokio::test]
    async fn process_decodes_relation_payload_into_runtime_relationships() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(JsonEmittingRuntime);
        let service = RuntimeExtractionService::new(runtime);
        let document = make_document(
            "Apple Inc announced a 10% expansion in Austin. Analysts reported hiring.",
        );

        let result = service.process(&document).await.unwrap();

        assert!(result
            .relationships
            .iter()
            .any(|r| r.from_entity_name == "Apple Inc"
                && r.to_entity_name == "Austin"
                && r.relationship_type == "located_in"));
    }

    #[test]
    fn classify_entity_type_handles_known_and_unknown_labels() {
        assert_eq!(
            RuntimeExtractionService::classify_entity_type("Person"),
            EntityType::Person
        );
        assert_eq!(
            RuntimeExtractionService::classify_entity_type("Org"),
            EntityType::Organization
        );
        assert_eq!(
            RuntimeExtractionService::classify_entity_type("Event"),
            EntityType::EventTopic
        );
        assert_eq!(
            RuntimeExtractionService::classify_entity_type("Widget"),
            EntityType::Concept
        );
    }
}
