use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Concept,
    EventTopic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: EntityType,
    pub aliases: Vec<String>,
    pub description: Option<String>,
    pub metadata: HashMap<String, Value>,
    pub confidence: f32,
    pub evidence_snippet: String,
}
