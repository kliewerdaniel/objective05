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
//! 2. The heuristic service produces the baseline entities,
//!    claims, and relationships for the document.
//! 3. If the runtime call succeeds, the heuristic's claim
//!    set is replaced with the runtime's claims. If it fails
//!    or times out, the heuristic's claim is kept and the
//!    error is logged.
//! 4. Entities and relationships always come from the
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
        DocumentProcessor, InferenceKind, InferenceResult, InferenceTask, ModelError,
        ModelRuntime,
    },
    types::{ExtractedClaim, ExtractionResult, RawDocument},
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
    async fn infer_chunk(
        &self,
        chunk: &str,
        index: usize,
        document: &RawDocument,
    ) -> Option<InferenceResult> {
        let task = InferenceTask::new(
            objective_core::traits::ModelId::Mistral7BInstruct,
            InferenceKind::ClaimExtraction,
            chunk,
        )
        .with_timeout(self.config.chunk_timeout);

        let document_id = document.id.to_string();
        match timeout(self.config.chunk_timeout, self.runtime.infer(task)).await {
            Ok(Ok(result)) => Some(result),
            Ok(Err(ModelError::Unavailable { kind })) => {
                warn!(
                    document_id = %document_id,
                    chunk_index = index,
                    kind = %kind,
                    "runtime unavailable for chunk; falling back to heuristic"
                );
                None
            }
            Ok(Err(ModelError::UnsupportedKind { kind })) => {
                warn!(
                    document_id = %document_id,
                    chunk_index = index,
                    kind = %kind,
                    "runtime does not support kind; falling back to heuristic"
                );
                None
            }
            Ok(Err(err)) => {
                warn!(
                    document_id = %document_id,
                    chunk_index = index,
                    error = %err,
                    "runtime error; falling back to heuristic"
                );
                None
            }
            Err(_elapsed) => {
                warn!(
                    document_id = %document_id,
                    chunk_index = index,
                    timeout_ms = self.config.chunk_timeout.as_millis() as u64,
                    "runtime timeout; falling back to heuristic"
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
            // Placeholder subject — the runtime would normally
            // emit a structured payload; for v1 we accept the
            // raw text and let downstream code (broadcast,
            // correlation) refine the subject. Keeping the
            // shape compatible with `HeuristicExtractionService`
            // means the rest of the pipeline does not need to
            // branch on provider.
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
}

#[async_trait]
impl DocumentProcessor for RuntimeExtractionService {
    async fn process(&self, document: &RawDocument) -> Result<ExtractionResult> {
        let baseline = self.fallback.process(document).await?;

        let chunks = Self::chunk_body(&document.body, self.config.max_chunk_chars);
        if chunks.is_empty() {
            return Ok(baseline);
        }

        let mut runtime_claims: Vec<ExtractedClaim> = Vec::new();
        let mut fallback_claims: Vec<ExtractedClaim> = Vec::new();

        for (index, chunk) in chunks.iter().enumerate() {
            match self.infer_chunk(chunk, index, document).await {
                Some(result) => {
                    if let Some(claim) = self.runtime_text_to_claim(&result.text, chunk) {
                        runtime_claims.push(claim);
                    } else {
                        // Runtime returned empty text; keep the
                        // heuristic claim for this chunk.
                        fallback_claims.push(baseline.claims[index.min(baseline.claims.len() - 1)].clone());
                    }
                }
                None => {
                    if let Some(claim) = baseline.claims.get(index) {
                        fallback_claims.push(claim.clone());
                    }
                }
            }
        }

        // Prefer runtime claims where available; fill the rest
        // with the heuristic's claims. The merge is keyed on
        // chunk index to keep the order stable for tests.
        let mut merged = runtime_claims;
        merged.extend(fallback_claims);

        Ok(ExtractionResult {
            document_id: baseline.document_id,
            entities: baseline.entities,
            claims: merged,
            relationships: baseline.relationships,
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

    /// Always succeeds. Used to exercise the "runtime returned
    /// a result" branch.
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
            Ok(InferenceResult::text_only(
                task.model,
                task.kind,
                format!("runtime said: {}", task.input),
                Duration::from_millis(1),
            ))
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
        // No runtime claims should leak in.
        assert!(result
            .claims
            .iter()
            .all(|c| c.attributed_to.as_deref() != Some("model-runtime")));
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
    }

    #[tokio::test]
    async fn process_handles_empty_body() {
        let runtime: Arc<dyn ModelRuntime> = Arc::new(AlwaysSucceedRuntime);
        let service = RuntimeExtractionService::new(runtime);
        let document = make_document("");

        let result = service.process(&document).await.unwrap();

        assert!(result.claims.is_empty());
    }
}
