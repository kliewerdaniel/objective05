use async_trait::async_trait;

use crate::{
    types::{HealthStatus, PollResult, RateLimitConfig, RawDocument},
    Result,
};

#[async_trait]
pub trait SourceAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn source_type(&self) -> &str;
    fn validate(&self) -> Result<()>;
    async fn poll(&self, cursor: Option<String>) -> Result<PollResult>;
    async fn fetch_one(&self, external_id: &str) -> Result<RawDocument>;
    async fn health(&self) -> Result<HealthStatus>;
    fn rate_limit_config(&self) -> RateLimitConfig;
}
