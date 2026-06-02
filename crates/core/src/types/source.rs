use std::time::Duration;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::RawDocument;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct SourceConfig {
    pub name: String,
    pub source_type: String,
    pub url: Option<String>,
    pub poll_interval_minutes: u64,
}

#[derive(Debug, Clone)]
pub struct PollResult {
    pub documents: Vec<RawDocument>,
    pub new_cursor: Option<String>,
    pub has_more: bool,
    pub poll_duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct HealthStatus {
    pub source_id: String,
    pub status: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst: u32,
}
