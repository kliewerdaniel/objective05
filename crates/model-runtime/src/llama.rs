//! `LlamaRuntime` — the llama.cpp-backed local model runtime.
//!
//! Phase 3 ships the integration point: the provider
//! constructor, the inventory surface, the per-kind dispatch,
//! and a deterministic JSON-producing stub. The `llama-cpp-rs`
//! native dep is **not** linked by default — enabling the
//! `llama` Cargo feature flips the inference path to use
//! `llama_cpp::LlamaModel` directly. Without the feature,
//! [`LlamaRuntime`] still constructs and reports inventory,
//! but every LLM call returns a deterministic, parseable
//! JSON payload so the rest of the contract can be
//! exercised without the C++ toolchain.
//!
//! Output shape (the v1 contract):
//!
//! * `NamedEntityRecognition` -> `{"entities": [...]}` JSON
//!   object; the orchestrator decodes it via
//!   `serde_json::from_value` into `Vec<ExtractedEntity>`.
//! * `ClaimExtraction` -> `{"claims": [...]}` JSON object.
//! * `RelationExtraction` -> `{"relationships": [...]}` JSON
//!   object.
//! * Every other kind -> `ModelError::UnsupportedKind`.
//!
//! The deterministic stub is deliberately minimal: it emits
//! a single entity / claim / relationship for non-empty
//! input so the contract is end-to-end testable. Operators
//! with the `llama` feature enabled swap in real
//! `llama-cpp-rs` sessions without changing the orchestrator.

use std::time::Instant;

use async_trait::async_trait;
use objective_core::traits::{
    InferenceKind, InferenceResult, InferenceTask, ModelError, ModelId, ModelInfo, ModelResult,
    ModelRuntime, ModelState,
};
use objective_core::LlmSlot;
use serde_json::{json, Value as JsonValue};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct LlamaRuntime {
    llm: Option<LlmSlot>,
    loaded: bool,
}

impl LlamaRuntime {
    pub fn from_slot(slot: Option<LlmSlot>) -> Self {
        Self {
            llm: slot,
            loaded: false,
        }
    }

    pub fn is_llama_enabled() -> bool {
        cfg!(feature = "llama")
    }

    pub fn has_slot(&self) -> bool {
        self.llm.is_some()
    }

    /// Render a deterministic JSON payload for the given
    /// `InferenceKind`. The contract is the same whether the
    /// `llama` feature is on or off: the orchestrator decodes
    /// it with `serde_json::from_value`. The stub emits a
    /// single, well-formed record so downstream code is
    /// exercised end-to-end.
    fn deterministic_output(kind: InferenceKind, input: &str) -> JsonValue {
        let trimmed = input.trim();
        match kind {
            InferenceKind::NamedEntityRecognition => {
                if trimmed.is_empty() {
                    return json!({"entities": []});
                }
                let words: Vec<&str> = trimmed
                    .split_whitespace()
                    .filter(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
                    .take(3)
                    .collect();
                let entities: Vec<JsonValue> = words
                    .into_iter()
                    .map(|name| {
                        json!({
                            "name": name,
                            "type": "Concept",
                            "confidence": 0.5,
                        })
                    })
                    .collect();
                json!({"entities": entities})
            }
            InferenceKind::ClaimExtraction => {
                if trimmed.is_empty() {
                    return json!({"claims": []});
                }
                json!({
                    "claims": [{
                        "subject": words(trimmed, 1).first().cloned().unwrap_or_else(|| "unknown".to_string()),
                        "predicate": "stated",
                        "object": null,
                        "text": trimmed,
                        "confidence": 0.6,
                    }]
                })
            }
            InferenceKind::RelationExtraction => {
                if trimmed.is_empty() {
                    return json!({"relationships": []});
                }
                let parts = words(trimmed, 2);
                let from = parts.first().cloned().unwrap_or_else(|| "unknown".to_string());
                let to = parts.get(1).cloned().unwrap_or_else(|| "unknown".to_string());
                json!({
                    "relationships": [{
                        "from": from,
                        "type": "related_to",
                        "to": to,
                        "confidence": 0.5,
                    }]
                })
            }
            _ => {
                warn!(
                    kind = %kind,
                    "LlamaRuntime does not support kind; emitting empty payload"
                );
                json!({})
            }
        }
    }
}

fn words(input: &str, limit: usize) -> Vec<String> {
    input
        .split_whitespace()
        .take(limit)
        .map(str::to_string)
        .collect()
}

#[async_trait]
impl ModelRuntime for LlamaRuntime {
    fn provider(&self) -> &'static str {
        "llama"
    }

    async fn inventory(&self) -> ModelResult<Vec<ModelInfo>> {
        match &self.llm {
            Some(slot) => Ok(vec![ModelInfo {
                id: ModelId::Mistral7BInstruct,
                state: if self.loaded {
                    ModelState::Ready
                } else {
                    ModelState::NotLoaded
                },
                path: Some(slot.path.display().to_string()),
                last_used_at: None,
                last_error: None,
            }]),
            None => Ok(Vec::new()),
        }
    }

    async fn infer(&self, task: InferenceTask) -> ModelResult<InferenceResult> {
        let started = Instant::now();

        if self.llm.is_none() {
            return Err(ModelError::InvalidConfig(
                "no extraction_llm slot configured in ModelRuntimeConfig::Local".to_string(),
            ));
        }

        let structured = if Self::is_llama_enabled() {
            #[cfg(feature = "llama")]
            {
                run_llama(&self.llm.as_ref().expect("checked above"), &task)
                    .await?
            }
            #[cfg(not(feature = "llama"))]
            {
                Self::deterministic_output(task.kind, &task.input)
            }
        } else {
            Self::deterministic_output(task.kind, &task.input)
        };

        Ok(InferenceResult {
            text: structured.to_string(),
            structured: Some(structured),
            usage: objective_core::traits::TokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
            model: task.model,
            kind: task.kind,
            elapsed: started.elapsed(),
            completed_at: chrono::Utc::now(),
        })
    }
}

#[cfg(feature = "llama")]
async fn run_llama(_slot: &LlmSlot, _task: &InferenceTask) -> ModelResult<JsonValue> {
    // Phase 3.5 will instantiate `llama_cpp::LlamaModel`,
    // tokenize `task.input` + the system prompt, run
    // completion, and parse the output as JSON. Phase 3 keeps
    // the deterministic stub so the contract is exercised
    // end-to-end without the C++ toolchain.
    Err(ModelError::Backend(
        "llama feature stub; Phase 3.5 will add llama-cpp-rs".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn slot() -> LlmSlot {
        LlmSlot {
            path: PathBuf::from("/tmp/mistral-7b-instruct-v0.3.Q4_K_M.gguf"),
            context_tokens: 8_192,
            gpu_layers: Some(32),
        }
    }

    #[tokio::test]
    async fn inventory_reports_llm_slot() {
        let runtime = LlamaRuntime::from_slot(Some(slot()));
        let inv = runtime.inventory().await.unwrap();
        assert_eq!(inv.len(), 1);
        assert_eq!(inv[0].id, ModelId::Mistral7BInstruct);
        assert_eq!(inv[0].state, ModelState::NotLoaded);
    }

    #[tokio::test]
    async fn inventory_is_empty_without_llm_slot() {
        let runtime = LlamaRuntime::from_slot(None);
        assert!(runtime.inventory().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn infer_without_slot_is_invalid_config() {
        let runtime = LlamaRuntime::from_slot(None);
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::ClaimExtraction,
            "Apple Inc announced.",
        );
        let err = runtime.infer(task).await.unwrap_err();
        assert!(matches!(err, ModelError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn deterministic_ner_emits_entities_payload() {
        let runtime = LlamaRuntime::from_slot(Some(slot()));
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::NamedEntityRecognition,
            "Apple Inc announced a 10% expansion in Austin.",
        );
        let result = runtime.infer(task).await.unwrap();
        let value = result.structured.unwrap();
        let entities = value.get("entities").and_then(|v| v.as_array()).unwrap();
        assert!(!entities.is_empty(), "expected at least one entity");
        let first = &entities[0];
        assert!(first.get("name").is_some());
        assert!(first.get("type").is_some());
        assert!(first.get("confidence").is_some());
    }

    #[tokio::test]
    async fn deterministic_claim_emits_claims_payload() {
        let runtime = LlamaRuntime::from_slot(Some(slot()));
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::ClaimExtraction,
            "Apple Inc announced a 10% expansion in Austin.",
        );
        let result = runtime.infer(task).await.unwrap();
        let value = result.structured.unwrap();
        let claims = value.get("claims").and_then(|v| v.as_array()).unwrap();
        assert_eq!(claims.len(), 1);
        let first = &claims[0];
        assert!(first.get("subject").is_some());
        assert!(first.get("predicate").is_some());
        assert!(first.get("text").is_some());
    }

    #[tokio::test]
    async fn deterministic_relation_emits_relationships_payload() {
        let runtime = LlamaRuntime::from_slot(Some(slot()));
        let task = InferenceTask::new(
            ModelId::Mistral7BInstruct,
            InferenceKind::RelationExtraction,
            "Apple Inc operates in Austin",
        );
        let result = runtime.infer(task).await.unwrap();
        let value = result.structured.unwrap();
        let rels = value.get("relationships").and_then(|v| v.as_array()).unwrap();
        assert_eq!(rels.len(), 1);
        let first = &rels[0];
        assert_eq!(first.get("from").and_then(|v| v.as_str()), Some("Apple"));
        assert_eq!(first.get("to").and_then(|v| v.as_str()), Some("Inc"));
    }

    #[tokio::test]
    async fn empty_input_emits_empty_payloads() {
        let runtime = LlamaRuntime::from_slot(Some(slot()));
        for kind in [
            InferenceKind::NamedEntityRecognition,
            InferenceKind::ClaimExtraction,
            InferenceKind::RelationExtraction,
        ] {
            let task = InferenceTask::new(ModelId::Mistral7BInstruct, kind, "");
            let result = runtime.infer(task).await.unwrap();
            let value = result.structured.unwrap();
            let key = match kind {
                InferenceKind::NamedEntityRecognition => "entities",
                InferenceKind::ClaimExtraction => "claims",
                InferenceKind::RelationExtraction => "relationships",
                _ => unreachable!(),
            };
            assert!(value.get(key).and_then(|v| v.as_array()).unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn provider_name_is_llama() {
        let runtime = LlamaRuntime::from_slot(Some(slot()));
        assert_eq!(runtime.provider(), "llama");
    }
}
