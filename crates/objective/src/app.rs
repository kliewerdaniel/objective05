use std::{net::SocketAddr, sync::Arc};

use objective_api_gateway::{build_router, ApiState};
use objective_core::{
    traits::{DocumentProcessor, DocumentRepository, ExtractionRepository},
    ObjectiveConfig,
};
use objective_extraction::HeuristicExtractionService;
use objective_ingestion::{adapters::StaticSourceAdapter, IngestionService};
use objective_message_bus::InMemoryMessageBus;
use objective_store::RuntimeStore;
use tracing::info;

pub async fn serve(config: ObjectiveConfig) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.storage.document_path)?;
    std::fs::create_dir_all(&config.storage.database_path)?;
    std::fs::create_dir_all(&config.storage.vector_path)?;
    std::fs::create_dir_all(&config.storage.queue_path)?;

    let store = Arc::new(RuntimeStore::new(config.storage.document_path.clone()));
    let bus = Arc::new(InMemoryMessageBus::new());
    seed_first_vertical_slice(Arc::clone(&store), Arc::clone(&bus)).await?;

    let app = build_router(ApiState::new(store, bus));
    let addr = SocketAddr::from(([127, 0, 0, 1], config.api.rest_port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "objective api listening");

    axum::serve(listener, app).await?;
    Ok(())
}

pub async fn setup(config: ObjectiveConfig) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.data_root)?;
    std::fs::create_dir_all(&config.storage.document_path)?;
    std::fs::create_dir_all(&config.storage.database_path)?;
    std::fs::create_dir_all(&config.storage.vector_path)?;
    std::fs::create_dir_all(&config.storage.queue_path)?;
    println!(
        "Objective data directories initialized at {}",
        config.data_root.display()
    );
    Ok(())
}

async fn seed_first_vertical_slice(
    store: Arc<RuntimeStore>,
    bus: Arc<InMemoryMessageBus>,
) -> anyhow::Result<()> {
    let ingestion = IngestionService::new(Arc::clone(&store), bus);
    let adapter = StaticSourceAdapter::from_plain_text(
        "local_fixture",
        "Austin Manufacturing Update",
        "Apple Inc announced a 10% manufacturing expansion in Austin. Analysts reported Apple Inc hired workers for the new facility.",
    );
    ingestion.poll_source(&adapter).await?;

    let processor = HeuristicExtractionService;
    for document in store.list_documents().await? {
        let extraction = processor.process(&document).await?;
        store.save_extraction(extraction).await?;
    }

    Ok(())
}
