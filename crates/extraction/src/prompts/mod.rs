//! Default prompt templates for the Phase 3 model runtime.
//!
//! Each prompt is a self-contained instruction that asks the
//! LLM to emit strict JSON. The orchestrator decodes the
//! output and merges the structured payload with the
//! heuristic baseline. The format is deliberately minimal —
//! every field that matters to downstream code is named
//! explicitly so JSON parses are stable across model swaps.
//!
//! Render via [`PromptSet::render`]; the placeholder is
//! `{{chunk}}` and must appear exactly once per template.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use objective_core::traits::InferenceKind;

pub const NER_SYSTEM: &str = "You extract named entities from news text. \
Respond with a single JSON object: {\"entities\": [{\"name\": \"...\", \"type\": \"Organization|Location|Person|Concept\", \"confidence\": 0.0-1.0}]}. \
Do not include any prose outside the JSON object.";

pub const NER_USER_TEMPLATE: &str = "Extract named entities from the following passage. \
Output JSON only.\n\nPassage:\n{{chunk}}";

pub const CLAIM_SYSTEM: &str = "You extract factual claims from news text. \
Respond with a single JSON object: {\"claims\": [{\"subject\": \"...\", \"predicate\": \"...\", \"object\": \"... or null\", \"text\": \"...\", \"confidence\": 0.0-1.0}]}. \
Use the exact sentence text for `text`. Do not include any prose outside the JSON object.";

pub const CLAIM_USER_TEMPLATE: &str = "Extract claims from the following passage. \
Output JSON only.\n\nPassage:\n{{chunk}}";

pub const RELATION_SYSTEM: &str = "You extract binary relations between named entities. \
Respond with a single JSON object: {\"relationships\": [{\"from\": \"...\", \"type\": \"located_in|acquired|announced|reported|employed|partners_with|other\", \"to\": \"...\", \"confidence\": 0.0-1.0}]}. \
Do not include any prose outside the JSON object.";

pub const RELATION_USER_TEMPLATE: &str = "Extract relationships from the following passage. \
Output JSON only.\n\nPassage:\n{{chunk}}";

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromptTemplate {
    pub system: String,
    pub user_template: String,
}

impl PromptTemplate {
    pub fn render(&self, chunk: &str) -> RenderedPrompt {
        assert!(
            self.user_template.contains("{{chunk}}"),
            "user template must contain {{{{chunk}}}} placeholder"
        );
        let user = self.user_template.replace("{{chunk}}", chunk);
        RenderedPrompt {
            system: self.system.to_string(),
            user,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RenderedPrompt {
    pub system: String,
    pub user: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct PromptSet {
    pub named_entity_recognition: Option<PromptTemplate>,
    pub claim_extraction: Option<PromptTemplate>,
    pub relation_extraction: Option<PromptTemplate>,
}

impl PromptSet {
    pub fn with_defaults() -> Self {
        Self {
            named_entity_recognition: Some(PromptTemplate {
                system: NER_SYSTEM.to_string(),
                user_template: NER_USER_TEMPLATE.to_string(),
            }),
            claim_extraction: Some(PromptTemplate {
                system: CLAIM_SYSTEM.to_string(),
                user_template: CLAIM_USER_TEMPLATE.to_string(),
            }),
            relation_extraction: Some(PromptTemplate {
                system: RELATION_SYSTEM.to_string(),
                user_template: RELATION_USER_TEMPLATE.to_string(),
            }),
        }
    }

    pub fn template_for(&self, kind: InferenceKind) -> Option<&PromptTemplate> {
        match kind {
            InferenceKind::NamedEntityRecognition => self.named_entity_recognition.as_ref(),
            InferenceKind::ClaimExtraction => self.claim_extraction.as_ref(),
            InferenceKind::RelationExtraction => self.relation_extraction.as_ref(),
            _ => None,
        }
    }

    pub fn render(&self, kind: InferenceKind, chunk: &str) -> Option<RenderedPrompt> {
        self.template_for(kind).map(|t| t.render(chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prompt_set_covers_llm_kinds() {
        let set = PromptSet::with_defaults();
        assert!(set.template_for(InferenceKind::NamedEntityRecognition).is_some());
        assert!(set.template_for(InferenceKind::ClaimExtraction).is_some());
        assert!(set.template_for(InferenceKind::RelationExtraction).is_some());
        assert!(set.template_for(InferenceKind::Embedding).is_none());
    }

    #[test]
    fn render_substitutes_chunk_placeholder() {
        let template = PromptTemplate {
            system: "Be terse.".to_string(),
            user_template: "Passage:\n{{chunk}}".to_string(),
        };
        let rendered = template.render("Apple Inc announced a 10% expansion.");
        assert_eq!(rendered.system, "Be terse.");
        assert!(rendered.user.contains("Apple Inc announced a 10% expansion."));
        assert!(!rendered.user.contains("{{chunk}}"));
    }

    #[test]
    #[should_panic(expected = "must contain")]
    fn render_panics_when_placeholder_missing() {
        let template = PromptTemplate {
            system: "X".to_string(),
            user_template: "no placeholder here".to_string(),
        };
        let _ = template.render("anything");
    }

    #[test]
    fn prompt_set_render_round_trip() {
        let set = PromptSet::with_defaults();
        let rendered = set
            .render(InferenceKind::ClaimExtraction, "Apple Inc announced a 10% expansion in Austin.")
            .unwrap();
        assert!(rendered.user.contains("Apple Inc announced"));
        assert!(!rendered.user.contains("{{chunk}}"));
    }
}
