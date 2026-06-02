use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ulid::Ulid;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BodyFormat {
    PlainText,
    Markdown,
    Html,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct RawDocument {
    pub id: Ulid,
    pub source_id: String,
    pub source_type: String,
    pub external_id: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub body: String,
    pub body_format: BodyFormat,
    pub author: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub language: String,
    pub content_hash: String,
    pub metadata: HashMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_bytes: Option<Vec<u8>>,
}
