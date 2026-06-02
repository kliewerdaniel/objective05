use async_trait::async_trait;
use objective_core::{traits::MessageBus, types::EventEnvelope, Result};
use tokio::sync::RwLock;

#[derive(Debug, Default)]
pub struct InMemoryMessageBus {
    events: RwLock<Vec<(String, EventEnvelope)>>,
}

impl InMemoryMessageBus {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MessageBus for InMemoryMessageBus {
    async fn publish(&self, subject: &str, event: EventEnvelope) -> Result<()> {
        self.events.write().await.push((subject.to_string(), event));
        Ok(())
    }

    async fn events(&self) -> Result<Vec<(String, EventEnvelope)>> {
        Ok(self.events.read().await.clone())
    }
}

#[cfg(test)]
mod tests {
    use objective_core::types::EventEnvelope;
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn test_publish_records_subject_and_event() {
        let bus = InMemoryMessageBus::new();
        bus.publish(
            "system.heartbeat",
            EventEnvelope::new("system.heartbeat", "test", json!({"ok": true})),
        )
        .await
        .unwrap();

        let events = bus.events().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "system.heartbeat");
        assert_eq!(events[0].1.event_type, "system.heartbeat");
    }
}
