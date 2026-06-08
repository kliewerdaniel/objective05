use std::sync::Arc;

use objective_core::traits::MessageBus;
use objective_core::types::EventEnvelope;
use objective_core::Result;
use serde_json::json;
use tokio::time::{interval, Duration};
use tracing::{error, info};

use crate::collector::BroadcastCollector;
use crate::generator::BroadcastGenerator;
use crate::store::BroadcastRepository;

pub struct BroadcastService {
    collector: BroadcastCollector,
    generator: BroadcastGenerator,
    repository: Arc<dyn BroadcastRepository>,
    bus: Arc<dyn MessageBus>,
    poll_interval_secs: u64,
}

impl BroadcastService {
    pub fn new(
        collector: BroadcastCollector,
        generator: BroadcastGenerator,
        repository: Arc<dyn BroadcastRepository>,
        bus: Arc<dyn MessageBus>,
    ) -> Self {
        Self {
            collector,
            generator,
            repository,
            bus,
            poll_interval_secs: 10,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("broadcast service starting");
        let mut last_index: usize = 0;
        let mut ticker = interval(Duration::from_secs(self.poll_interval_secs));

        loop {
            ticker.tick().await;

            let events = self.bus.events().await?;
            if events.len() <= last_index {
                continue;
            }

            let new_events: Vec<EventEnvelope> = events[last_index..]
                .iter()
                .map(|(_, e)| e.clone())
                .collect();
            last_index = events.len();

            for event in new_events {
                if event.event_type == "broadcast.generate" || event.event_type == "broadcast.generate_immediate" {
                    if let Err(e) = self.generate_and_deliver().await {
                        error!(error = %e, "broadcast generation failed");
                    }
                }
            }
        }
    }

    pub async fn generate_on_demand(&self) -> Result<crate::types::BroadcastRecord> {
        self.generate_and_deliver().await
    }

    async fn generate_and_deliver(&self) -> Result<crate::types::BroadcastRecord> {
        let collection = self.collector.collect().await?;
        info!(
            events = collection.top_events.len(),
            narratives = collection.narratives.len(),
            contradictions = collection.contradictions.len(),
            "collected data for broadcast"
        );

        let record = self.generator.generate(&collection).await?;
        self.repository.insert(record.clone()).await?;

        let _ = self
            .bus
            .publish(
                "broadcast.generated",
                EventEnvelope::new("broadcast.generated", "broadcast", json!({
                    "broadcast_id": record.id,
                    "title": record.title,
                    "status": "ready",
                    "event_count": record.event_count,
                })),
            )
            .await;

        info!(
            broadcast_id = %record.id,
            title = %record.title,
            event_count = record.event_count,
            "broadcast generated and delivered"
        );

        Ok(record)
    }
}
