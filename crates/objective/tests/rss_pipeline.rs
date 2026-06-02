use std::sync::Arc;

use objective_core::traits::{
    DocumentProcessor, DocumentRepository, ExtractionRepository, MessageBus,
};
use objective_extraction::HeuristicExtractionService;
use objective_ingestion::{adapters::RssSourceAdapter, IngestionService};
use objective_message_bus::InMemoryMessageBus;
use objective_store::InMemoryStore;

const RSS_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
  <channel>
    <title>Objective Fixture Feed</title>
    <link>https://example.com</link>
    <description>Fixture feed</description>
    <item>
      <title>Apple expands Austin manufacturing</title>
      <link>https://example.com/apple-austin</link>
      <guid>apple-austin</guid>
      <pubDate>Tue, 02 Jun 2026 12:00:00 GMT</pubDate>
      <description><![CDATA[Apple Inc announced a 10% manufacturing expansion in Austin with details from local officials.]]></description>
      <category>Business</category>
    </item>
  </channel>
</rss>"#;

#[tokio::test]
async fn test_rss_ingestion_to_extraction_pipeline() {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));
    let adapter =
        RssSourceAdapter::from_xml("fixture_rss", "https://example.com/rss.xml", RSS_FIXTURE);

    let ingested = ingestion.poll_source(&adapter).await.unwrap();
    assert_eq!(ingested, 1);

    let processor = HeuristicExtractionService;
    for document in store.list_documents().await.unwrap() {
        let extraction = processor.process(&document).await.unwrap();
        store.save_extraction(extraction).await.unwrap();
    }

    let events = bus.events().await.unwrap();
    let extractions = store.list_extractions().await.unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1.event_type, "ingestion.document.received");
    assert_eq!(extractions.len(), 1);
    assert!(extractions[0]
        .entities
        .iter()
        .any(|entity| entity.name == "Apple Inc"));
    assert!(extractions[0]
        .claims
        .iter()
        .any(|claim| claim.object_value.as_deref() == Some("10%")));
}
