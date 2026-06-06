use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{ExtractedEntity, ModelIndex};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClaimType {
    Attribution,
    Relation,
    Quantification,
    Temporal,
    Comparison,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ExtractedClaim {
    pub claim_text: String,
    pub subject_name: String,
    pub predicate: String,
    pub object_name: Option<String>,
    pub object_value: Option<String>,
    pub claim_type: ClaimType,
    pub sentiment: Option<f32>,
    pub confidence: f32,
    pub evidence_snippet: String,
    pub attributed_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ExtractedRelationship {
    pub from_entity_name: String,
    pub to_entity_name: String,
    pub relationship_type: String,
    pub confidence: f32,
    pub evidence_snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ExtractionResult {
    pub document_id: String,
    pub entities: Vec<ExtractedEntity>,
    pub claims: Vec<ExtractedClaim>,
    pub relationships: Vec<ExtractedRelationship>,
    /// Optional embedding sidecar produced by a `ModelRuntime`.
    /// Absent for the v0 heuristic pipeline and for documents
    /// that were processed without an embedding slot. Phase 2
    /// populates this when `ModelRuntimeConfig::Local` has an
    /// `embedding` slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_index: Option<ModelIndex>,
}
