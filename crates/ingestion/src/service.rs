use std::sync::Arc;

use objective_core::{
    traits::{DocumentRepository, MessageBus, SourceAdapter},
    types::EventEnvelope,
    Result,
};
use serde_json::json;
use tracing::info;

pub struct IngestionService<R, B>
where
    R: DocumentRepository,
    B: MessageBus,
{
    repository: Arc<R>,
    bus: Arc<B>,
}

impl<R, B> IngestionService<R, B>
where
    R: DocumentRepository,
    B: MessageBus,
{
    pub fn new(repository: Arc<R>, bus: Arc<B>) -> Self {
        Self { repository, bus }
    }

    pub async fn poll_source<A>(&self, adapter: &A) -> Result<usize>
    where
        A: SourceAdapter,
    {
        adapter.validate()?;
        let result = adapter.poll(None).await?;
        let count = result.documents.len();

        for document in result.documents {
            let data = json!({
                "document_id": document.id.to_string(),
                "source_id": document.source_id,
                "source_type": document.source_type,
                "url": document.url,
                "title": document.title,
                "published_at": document.published_at,
            });
            self.repository.save_document(document).await?;
            self.bus
                .publish(
                    "ingestion.document.received",
                    EventEnvelope::new("ingestion.document.received", adapter.name(), data),
                )
                .await?;
        }

        info!(source = adapter.name(), documents = count, "poll complete");
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use objective_core::traits::{DocumentRepository, MessageBus};
    use objective_message_bus::InMemoryMessageBus;
    use objective_store::InMemoryStore;

    use crate::adapters::StaticSourceAdapter;

    use super::*;

    #[tokio::test]
    async fn test_poll_source_saves_document_and_publishes_event() {
        let store = Arc::new(InMemoryStore::new());
        let bus = Arc::new(InMemoryMessageBus::new());
        let service = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));
        let adapter = StaticSourceAdapter::from_plain_text(
            "fixture",
            "Austin update",
            "Apple Inc announced a new manufacturing plan in Austin with public details.",
        );

        let count = service.poll_source(&adapter).await.unwrap();

        assert_eq!(count, 1);
        assert_eq!(store.list_documents().await.unwrap().len(), 1);
        assert_eq!(bus.events().await.unwrap().len(), 1);
    }
}
