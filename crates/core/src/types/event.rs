use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ulid::Ulid;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct EventMetadata {
    pub producer: String,
    pub producer_version: String,
    pub retry_count: u32,
    pub produced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct EventEnvelope {
    pub id: Ulid,
    #[schema(value_type = String)]
    pub event_type: String,
    pub version: u32,
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub correlation_id: Ulid,
    pub causation_id: Ulid,
    pub data: Value,
    pub metadata: EventMetadata,
}

impl EventEnvelope {
    pub fn new(event_type: impl Into<String>, source: impl Into<String>, data: Value) -> Self {
        let id = Ulid::new();
        let now = Utc::now();

        Self {
            id,
            event_type: event_type.into(),
            version: 1,
            timestamp: now,
            source: source.into(),
            correlation_id: id,
            causation_id: id,
            data,
            metadata: EventMetadata {
                producer: "objective".to_string(),
                producer_version: env!("CARGO_PKG_VERSION").to_string(),
                retry_count: 0,
                produced_at: now,
            },
        }
    }
}
