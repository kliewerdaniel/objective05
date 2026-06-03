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
    <title>Objective Test Feed</title>
    <link>https://example.com</link>
    <description>Test feed for integration tests</description>
    <item>
      <title>Tesla announces new Gigafactory in Nevada</title>
      <link>https://example.com/tesla-nevada</link>
      <guid>tesla-nevada</guid>
      <pubDate>Tue, 02 Jun 2026 12:00:00 GMT</pubDate>
      <description><![CDATA[Tesla Inc announced a new $5 billion Gigafactory in Nevada with capacity for 500,000 vehicles per year. Local officials praised the economic impact.]]></description>
      <category>Business</category>
    </item>
    <item>
      <title>Apple expands Austin manufacturing</title>
      <link>https://example.com/apple-austin</link>
      <guid>apple-austin</guid>
      <pubDate>Tue, 02 Jun 2026 11:00:00 GMT</pubDate>
      <description><![CDATA[Apple Inc announced a 10% manufacturing expansion in Austin with details from local officials.]]></description>
      <category>Business</category>
    </item>
  </channel>
</rss>"#;

#[tokio::test]
async fn test_full_pipeline_ingestion_to_extraction_to_api() {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));

    // Step 1: Ingest from RSS
    let adapter =
        RssSourceAdapter::from_xml("integration_test", "https://example.com/rss.xml", RSS_FIXTURE);
    let ingested = ingestion.poll_source(&adapter).await.unwrap();
    assert_eq!(ingested, 2, "should ingest 2 documents from RSS feed");

    // Step 2: Verify documents are stored
    let documents = store.list_documents().await.unwrap();
    assert_eq!(documents.len(), 2);
    assert!(documents.iter().any(|d| d.title.as_deref() == Some("Tesla announces new Gigafactory in Nevada")));
    assert!(documents.iter().any(|d| d.title.as_deref() == Some("Apple expands Austin manufacturing")));

    // Step 3: Process documents through extraction
    let processor = HeuristicExtractionService;
    let mut extraction_count = 0;
    for document in &documents {
        let extraction = processor.process(document).await.unwrap();
        store.save_extraction(extraction).await.unwrap();
        extraction_count += 1;
    }
    assert_eq!(extraction_count, 2);

    // Step 4: Verify extractions contain expected entities and claims
    let extractions = store.list_extractions().await.unwrap();
    assert_eq!(extractions.len(), 2);

    let tesla_extraction = extractions
        .iter()
        .find(|e| e.document_id != extractions[0].document_id || e.entities.iter().any(|ent| ent.name == "Tesla Inc"))
        .unwrap();

    assert!(tesla_extraction.entities.iter().any(|e| e.name == "Tesla Inc"));
    assert!(tesla_extraction.claims.iter().any(|c| c.subject_name == "Tesla Inc"));

    // Step 5: Verify message bus has events
    let events = bus.events().await.unwrap();
    assert_eq!(events.len(), 2, "should have 2 ingestion events");
    assert!(events.iter().all(|e| e.1.event_type == "ingestion.document.received"));
}

#[tokio::test]
async fn test_ingestion_deduplicates_by_content_hash() {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));

    let adapter =
        RssSourceAdapter::from_xml("dedup_test", "https://example.com/rss.xml", RSS_FIXTURE);

    // Ingest the same feed twice
    ingestion.poll_source(&adapter).await.unwrap();
    ingestion.poll_source(&adapter).await.unwrap();

    // The store should deduplicate by content_hash
    let documents = store.list_documents().await.unwrap();
    assert_eq!(documents.len(), 2, "should still have only 2 documents after dedup");
}

#[tokio::test]
async fn test_extraction_produces_claims_with_confidence() {
    let store = Arc::new(InMemoryStore::new());
    let bus = Arc::new(InMemoryMessageBus::new());
    let ingestion = IngestionService::new(Arc::clone(&store), Arc::clone(&bus));

    let adapter =
        RssSourceAdapter::from_xml("confidence_test", "https://example.com/rss.xml", RSS_FIXTURE);
    ingestion.poll_source(&adapter).await.unwrap();

    let processor = HeuristicExtractionService;
    for document in store.list_documents().await.unwrap() {
        let extraction = processor.process(&document).await.unwrap();

        // All claims should have confidence > 0
        for claim in &extraction.claims {
            assert!(claim.confidence > 0.0, "claim should have positive confidence: {}", claim.claim_text);
            assert!(!claim.evidence_snippet.is_empty(), "claim should have evidence");
        }

        store.save_extraction(extraction).await.unwrap();
    }

    let extractions = store.list_extractions().await.unwrap();
    let total_claims: usize = extractions.iter().map(|e| e.claims.len()).sum();
    assert!(total_claims > 0, "should have extracted at least one claim");
}
