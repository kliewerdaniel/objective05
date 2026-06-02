use async_trait::async_trait;

use crate::{types::EventEnvelope, Result};

#[async_trait]
pub trait MessageBus: Send + Sync {
    async fn publish(&self, subject: &str, event: EventEnvelope) -> Result<()>;
    async fn events(&self) -> Result<Vec<(String, EventEnvelope)>>;
}
