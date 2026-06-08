use std::{net::SocketAddr, sync::Arc, time::Duration};

use objective_api_gateway::{build_router, ApiState, WebSocketHub};
use objective_broadcast::{
    BroadcastCollector, BroadcastGenerator, BroadcastService, FileBroadcastRepository,
};
use objective_core::{
    traits::{DocumentProcessor, DocumentRepository, ExtractionRepository, GraphRepository},
    ModelRuntimeConfig, ObjectiveConfig,
};
use objective_correlation::{store::FileEventRepository, EventEngine};
use objective_extraction::{
    HeuristicExtractionService, RuntimeExtractionConfig, RuntimeExtractionService,
};
use objective_ingestion::{
    adapters::{RssSourceAdapter, StaticSourceAdapter},
    IngestionService, SourceDefinition, SourceRegistry, SourceType,
};
use objective_message_bus::InMemoryMessageBus;
use objective_model_runtime::local::LocalModelRuntime;
use objective_model_runtime::runtime_for;
use objective_core::traits::ModelRuntime;
use objective_api_gateway::server::ModelRuntimeHandle;
use objective_plugin_host::{
    audit_log_plugin, re_emitter_plugin, BuiltinPlugin, HostConfig, PluginHost,
};
use objective_scheduler::{SchedulerConfig, SchedulerService};
use objective_store::{
    kuzu::KuzuGraphStore,
    monitoring::{ModelMetricsRow, ModelRuntimeMetricsSnapshot, MonitoringService},
    recovery::{RecoveryConfig, RecoveryService},
    retry_queue::RetryQueue,
    snapshot::SnapshotService,
    vectordb::LanceVectorStore,
    RuntimeStore,
};
use tracing::{info, warn};

use crate::pipeline::{first_claim_from, PipelineWorker};

/// Bundles every stateful component owned by the running daemon.
pub struct AppState {
    pub store: Arc<RuntimeStore>,
    pub bus: Arc<InMemoryMessageBus>,
    pub graph: Arc<KuzuGraphStore>,
    pub vectors: Arc<LanceVectorStore>,
    pub event_repository: Arc<FileEventRepository>,
    pub event_engine: Arc<EventEngine<FileEventRepository>>,
    pub scheduler: Arc<SchedulerService>,
    pub monitoring: Arc<MonitoringService>,
    pub recovery: Arc<RecoveryService>,
    pub websocket_hub: WebSocketHub,
    pub source_registry: Arc<SourceRegistry>,
    pub plugin_host: Arc<PluginHost<InMemoryMessageBus>>,
    pub processor: Arc<dyn DocumentProcessor>,
    pub model_runtime_config: ModelRuntimeConfig,
    /// File-backed broadcast repository with auto-persistence.
    pub broadcast_repository: Arc<FileBroadcastRepository>,
    /// Handle that backs the `/api/v1/model-runtime` routes.
    /// Populated only for `ModelRuntimeConfig::Local`. The
    /// `swappable` inner `Arc<RwLock<Arc<dyn ModelRuntime>>>`
    /// is shared with `RuntimeExtractionService` so a
    /// `POST /api/v1/model-runtime/reload` rebuilds and
    /// atomically swaps the live runtime — the next
    /// `process` call picks up the new instance without a
    /// daemon restart.
    pub model_runtime_handle: Option<Arc<ModelRuntimeHandle>>,
}

impl AppState {
    pub async fn build(config: &ObjectiveConfig) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&config.storage.document_path)?;
        std::fs::create_dir_all(&config.storage.database_path)?;
        std::fs::create_dir_all(&config.storage.vector_path)?;
        std::fs::create_dir_all(&config.storage.queue_path)?;
        std::fs::create_dir_all(&config.storage.graph_path)?;

        let store = Arc::new(RuntimeStore::new(config.storage.document_path.clone()));
        let bus = Arc::new(InMemoryMessageBus::new());

        let graph = Arc::new(KuzuGraphStore::new(&config.storage.graph_path)?);
        let vectors =
            Arc::new(LanceVectorStore::new(&config.storage.vector_path, "document_vectors").await?);

        let broadcasts_path = config.data_root.join("state").join("broadcasts.json");
        let broadcast_repository = Arc::new(FileBroadcastRepository::new(broadcasts_path));

        let event_path = config.data_root.join("state").join("events.json");
        let event_repository = Arc::new(FileEventRepository::new(&event_path));
        let event_engine = Arc::new(EventEngine::with_defaults(Arc::clone(&event_repository)));

        // Initialize scheduler with persistent state
        let state_dir = config.data_root.join("state");
        std::fs::create_dir_all(&state_dir)?;
        let scheduler_config = SchedulerConfig {
            state_path: Some(state_dir.join("scheduler.jobstate")),
            ..Default::default()
        };
        let scheduler = Arc::new(SchedulerService::new(
            scheduler_config,
            Arc::clone(&bus) as Arc<_>,
        ));

        let monitoring = Arc::new(MonitoringService::new(&config.data_root));

        let recovery = Arc::new(RecoveryService::new(
            RecoveryConfig {
                state_path: Some(state_dir.join("recovery.json")),
                ..Default::default()
            },
            Arc::clone(&monitoring),
            Arc::clone(&bus) as Arc<_>,
        ));

        let websocket_hub = WebSocketHub::spawn(Arc::clone(&bus));

        let source_registry = Arc::new(SourceRegistry::load(&state_dir));
        if let Err(err) = source_registry
            .add(SourceDefinition {
                name: "hackernews_front".to_string(),
                source_type: SourceType::Rss,
                url: Some("https://hnrss.org/frontpage".to_string()),
                schedule: None,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .await
        {
            if !matches!(err, objective_ingestion::RegistryError::AlreadyExists(_)) {
                warn!(?err, "failed to seed hackernews_front source");
            }
        }
        if let Err(err) = source_registry
            .add(SourceDefinition {
                name: "lobsters".to_string(),
                source_type: SourceType::Rss,
                url: Some("https://lobste.rs/rss".to_string()),
                schedule: None,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .await
        {
            if !matches!(err, objective_ingestion::RegistryError::AlreadyExists(_)) {
                warn!(?err, "failed to seed lobsters source");
            }
        }

        // Plugin host: registers two built-in plugins (audit log and
        // re-emitter) so the local-first runtime has a working plugin
        // pipeline out of the box. External plugins are discovered from
        // `.objective/plugins/<name>/plugin.json` if any are present.
        let plugin_host = Arc::new(PluginHost::new(
            Arc::clone(&bus),
            state_dir.join("plugins.json"),
            HostConfig::default(),
        )?);
        if let Err(err) = plugin_host
            .register_builtin(BuiltinPlugin::new(
                objective_plugin_host::builtin_manifest(
                    "audit-log",
                    objective_plugin_host::PluginType::Filter,
                    "0.1.0",
                    vec![
                        "ingestion.document.received".to_string(),
                        "extraction.document.processed".to_string(),
                        "correlation.event.detected".to_string(),
                        "broadcast.generated".to_string(),
                    ],
                ),
                audit_log_plugin(vec![
                    "ingestion.document.received".to_string(),
                    "extraction.document.processed".to_string(),
                    "correlation.event.detected".to_string(),
                    "broadcast.generated".to_string(),
                ]),
            ))
            .await
        {
            warn!(?err, "failed to register audit-log plugin");
        }
        if let Err(err) = plugin_host
            .register_builtin(BuiltinPlugin::new(
                objective_plugin_host::builtin_manifest(
                    "re-emitter",
                    objective_plugin_host::PluginType::Processor,
                    "0.1.0",
                    vec!["extraction.document.processed".to_string()],
                ),
                re_emitter_plugin(
                    vec!["extraction.document.processed".to_string()],
                    "plugin.re_emitted",
                ),
            ))
            .await
        {
            warn!(?err, "failed to register re-emitter plugin");
        }

        let model_runtime_handle =
            Self::build_model_runtime_handle(&config.model_runtime)?;
        let processor = Self::build_processor(&config.model_runtime, model_runtime_handle.as_ref())?;
        let model_runtime_config = config.model_runtime.clone();

        Ok(Self {
            store,
            bus,
            graph,
            vectors,
            event_repository,
            event_engine,
            scheduler,
            monitoring,
            recovery,
            websocket_hub,
            source_registry,
            plugin_host,
            processor,
            model_runtime_config,
            model_runtime_handle,
            broadcast_repository,
        })
    }

    /// Build the document processor that the pipeline worker
    /// will use. Driven by `config.model_runtime`:
    ///
    /// * `Disabled` -> `HeuristicExtractionService` (v0 default;
    ///   no runtime is constructed).
    /// * `Heuristic` / `Local` -> `RuntimeExtractionService`
    ///   backed by the provider returned by
    ///   `objective_model_runtime::runtime_for`. The `Local`
    ///   branch shares a swappable handle with the API
    ///   route so `POST /api/v1/model-runtime/reload`
    ///   rebuilds the live runtime atomically.
    fn build_processor(
        config: &ModelRuntimeConfig,
        model_runtime_handle: Option<&Arc<ModelRuntimeHandle>>,
    ) -> anyhow::Result<Arc<dyn DocumentProcessor>> {
        match config {
            ModelRuntimeConfig::Disabled => Ok(Arc::new(HeuristicExtractionService)),
            ModelRuntimeConfig::Heuristic => {
                let runtime = runtime_for(config)?;
                Ok(Arc::new(RuntimeExtractionService::new(runtime)))
            }
            ModelRuntimeConfig::Local(local) => {
                let handle = model_runtime_handle
                    .as_ref()
                    .expect("build_processor(Local) requires a ModelRuntimeHandle");
                let swappable = Arc::clone(&handle.swappable);
                let chunk_timeout =
                    Duration::from_millis(local.chunk_timeout_ms.max(1_000));
                Ok(Arc::new(
                    RuntimeExtractionService::with_swappable_handle(swappable).with_config(
                        RuntimeExtractionConfig {
                            chunk_timeout,
                            ..RuntimeExtractionConfig::default()
                        },
                    ),
                ))
            }
        }
    }

    /// Build the reload-able handle that backs the
    /// `POST /api/v1/model-runtime/reload` route. Only
    /// `ModelRuntimeConfig::Local` produces a handle; the
    /// heuristic and disabled modes return `None` so the
    /// route responds with 503.
    fn build_model_runtime_handle(
        config: &ModelRuntimeConfig,
    ) -> anyhow::Result<Option<Arc<ModelRuntimeHandle>>> {
        match config {
            ModelRuntimeConfig::Local(local) => {
                let runtime = LocalModelRuntime::from_config(local.clone());
                let view = Arc::new(tokio::sync::RwLock::new(runtime));
                let swappable = Arc::new(tokio::sync::RwLock::new(Arc::new(
                    LocalModelRuntime::from_config(local.clone()),
                )
                    as Arc<dyn ModelRuntime>));
                Ok(Some(Arc::new(ModelRuntimeHandle {
                    swappable,
                    view,
                    config: local.clone(),
                })))
            }
            _ => Ok(None),
        }
    }

    /// Run the first vertical slice through the full pipeline so the API has
    /// visible data immediately on startup.
    pub async fn seed_first_vertical_slice(&self) -> anyhow::Result<()> {
        let ingestion = IngestionService::new(Arc::clone(&self.store), Arc::clone(&self.bus));
        let adapter = StaticSourceAdapter::from_plain_text(
            "local_fixture",
            "Austin Manufacturing Update",
            "Apple Inc announced a 10% manufacturing expansion in Austin. Analysts reported Apple Inc hired workers for the new facility.",
        );
        ingestion.poll_source(&adapter).await?;

        let processor: Arc<dyn DocumentProcessor> = Arc::clone(&self.processor);
        for document in self.store.list_documents().await? {
            let extraction = processor.process(&document).await?;
            self.store.save_extraction(extraction.clone()).await?;

            // Mirror the extractions into the graph and vector stores so the
            // documented Kuzu and LanceDB paths stay warm even while their
            // persistent backends remain in-progress.
            for entity in &extraction.entities {
                if let Err(error) = self.graph.save_entity(entity.clone()).await {
                    warn!(?error, "graph.save_entity failed");
                }
            }
            for claim in &extraction.claims {
                if let Err(error) = self.graph.save_claim(claim.clone()).await {
                    warn!(?error, "graph.save_claim failed");
                }
            }
            for relationship in &extraction.relationships {
                if let Err(error) = self.graph.save_relationship(relationship.clone()).await {
                    warn!(?error, "graph.save_relationship failed");
                }
            }

            // Feed the extracted claims into the correlation event engine so
            // the running daemon has a live set of derived events available
            // for the API and any future broadcast layer.
            for (index, claim) in extraction.claims.iter().enumerate() {
                let first_claim = first_claim_from(&document, claim, index);
                if let Err(error) = self.event_engine.ingest(first_claim).await {
                    warn!(?error, "event_engine.ingest failed");
                }
            }
        }

        Ok(())
    }
}

pub async fn serve(config: ObjectiveConfig) -> anyhow::Result<()> {
    let state = AppState::build(&config).await?;
    state.seed_first_vertical_slice().await?;

    let api_state = ApiState::new(Arc::clone(&state.store) as Arc<_>, Arc::clone(&state.bus))
        .with_started_at(chrono::Utc::now())
        .with_event_repository(
            Arc::clone(&state.event_repository) as Arc<dyn objective_correlation::EventRepository>
        )
        .with_monitoring(Arc::clone(&state.monitoring))
        .with_recovery(Arc::clone(&state.recovery))
        .with_websocket_hub(state.websocket_hub.clone())
        .with_source_registry(Arc::clone(&state.source_registry))
        .with_plugin_host(Arc::clone(&state.plugin_host) as Arc<_>);

    let api_state = if let Some(handle) = state.model_runtime_handle.as_ref() {
        api_state.with_model_runtime(Arc::clone(handle))
    } else {
        api_state
    };

    let app = build_router(api_state);
    let addr = SocketAddr::from(([127, 0, 0, 1], config.api.rest_port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(
        %addr,
        graph = state.graph.is_stub(),
        vectors = state.vectors.is_stub(),
        model_runtime = %state.model_runtime_config,
        "objective api listening"
    );

    // Start scheduler in background
    let scheduler = Arc::clone(&state.scheduler);
    tokio::spawn(async move {
        if let Err(e) = scheduler.start().await {
            tracing::error!("scheduler error: {e}");
        }
    });

    // Start recovery service in background
    let recovery = Arc::clone(&state.recovery);
    tokio::spawn(async move {
        if let Err(e) = recovery.start().await {
            tracing::error!("recovery service error: {e}");
        }
    });

    // Start plugin host in background
    let plugin_host = Arc::clone(&state.plugin_host);
    tokio::spawn(async move {
        if let Err(e) = plugin_host.run().await {
            tracing::error!("plugin host error: {e}");
        }
    });

    // Start pipeline worker in background
    let ingestion = IngestionService::new(Arc::clone(&state.store), Arc::clone(&state.bus));
    let processor: Arc<dyn DocumentProcessor> = Arc::clone(&state.processor);
    let default_sources: Vec<Arc<dyn objective_core::traits::SourceAdapter>> = vec![
        Arc::new(RssSourceAdapter::new(
            "hackernews_front",
            "https://hnrss.org/frontpage",
        )),
        Arc::new(RssSourceAdapter::new("lobsters", "https://lobste.rs/rss")),
    ];
    let pipeline = PipelineWorker::new(
        ingestion,
        processor,
        Arc::clone(&state.store),
        Arc::clone(&state.bus),
        default_sources,
        Arc::clone(&state.event_engine),
        Arc::new(SnapshotService::new(
            &config.data_root,
            &config.data_root.join("state"),
            &config.storage.document_path,
        )),
        Arc::new(RetryQueue::new(&config.data_root, 3, 60)),
        Arc::clone(&state.monitoring),
    );
    tokio::spawn(async move {
        if let Err(e) = pipeline.run().await {
            tracing::error!("pipeline worker error: {e}");
        }
    });

    // Start broadcast service in background
    let broadcast_runtime = runtime_for(&config.model_runtime).ok();
    let broadcast_service = BroadcastService::new(
        BroadcastCollector::new(Arc::clone(&state.event_repository) as Arc<dyn objective_correlation::EventRepository>),
        BroadcastGenerator::new(broadcast_runtime),
        Arc::clone(&state.broadcast_repository) as Arc<_>,
        Arc::clone(&state.bus) as Arc<_>,
    );
    tokio::spawn(async move {
        if let Err(e) = broadcast_service.run().await {
            tracing::error!("broadcast service error: {e}");
        }
    });

    // Start the model-runtime metrics publisher. Phase 4:
    // a small background task that polls the live
    // `LocalModelRuntime` for its latency histograms and
    // pushes the snapshot into the `MonitoringService` so
    // `/api/v1/monitoring` and the persisted JSON file
    // both reflect the live metrics. Only runs when a
    // handle is attached (i.e. `ModelRuntimeConfig::Local`).
    if let Some(handle) = state.model_runtime_handle.as_ref() {
        let handle = Arc::clone(handle);
        let monitoring = Arc::clone(&state.monitoring);
        tokio::spawn(async move {
            use std::time::Duration;
            let mut ticker = tokio::time::interval(Duration::from_secs(1));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                let snap = handle.view.read().await.metrics_snapshot();
                let rows_by_kind: Vec<ModelMetricsRow> = snap
                    .by_kind
                    .iter()
                    .map(|(k, h)| ModelMetricsRow {
                        key: k.to_string(),
                        count: h.count,
                        sum_ms: h.sum_ms,
                        min_ms: h.min_ms,
                        max_ms: h.max_ms,
                        p50_ms: h.p50_ms,
                        p95_ms: h.p95_ms,
                        p99_ms: h.p99_ms,
                        timeouts: h.timeouts,
                        errors: h.errors,
                    })
                    .collect();
                let rows_by_model: Vec<ModelMetricsRow> = snap
                    .by_model
                    .iter()
                    .map(|(k, h)| ModelMetricsRow {
                        key: k.as_str().to_string(),
                        count: h.count,
                        sum_ms: h.sum_ms,
                        min_ms: h.min_ms,
                        max_ms: h.max_ms,
                        p50_ms: h.p50_ms,
                        p95_ms: h.p95_ms,
                        p99_ms: h.p99_ms,
                        timeouts: h.timeouts,
                        errors: h.errors,
                    })
                    .collect();
                let snapshot = ModelRuntimeMetricsSnapshot {
                    total_calls: snap.total_calls,
                    total_timeouts: snap.total_timeouts,
                    total_errors: snap.total_errors,
                    total_fallbacks: snap.total_fallbacks,
                    by_kind: rows_by_kind,
                    by_model: rows_by_model,
                    last_observed_at: snap.last_observed_at,
                };
                monitoring.update_model_runtime(snapshot);
            }
        });
    }

    axum::serve(listener, app).await?;
    Ok(())
}

pub async fn setup(config: ObjectiveConfig) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.data_root)?;
    std::fs::create_dir_all(&config.storage.document_path)?;
    std::fs::create_dir_all(&config.storage.database_path)?;
    std::fs::create_dir_all(&config.storage.vector_path)?;
    std::fs::create_dir_all(&config.storage.queue_path)?;
    std::fs::create_dir_all(&config.storage.graph_path)?;
    println!(
        "Objective data directories initialized at {}",
        config.data_root.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::pipeline::first_claim_from;
    use chrono::Utc;
    use objective_core::types::{BodyFormat, ClaimType, ExtractedClaim, RawDocument};
    use serde_json::json;
    use std::collections::HashMap;
    use ulid::Ulid;

    fn make_document(
        title: Option<&str>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> RawDocument {
        RawDocument {
            id: Ulid::new(),
            source_id: "fixture".to_string(),
            source_type: "fixture".to_string(),
            external_id: "1".to_string(),
            url: None,
            title: title.map(|s| s.to_string()),
            body: "Apple Inc announced a 10% manufacturing expansion in Austin.".to_string(),
            body_format: BodyFormat::PlainText,
            author: None,
            published_at: Some(Utc::now()),
            fetched_at: Utc::now(),
            language: "en".to_string(),
            content_hash: "hash".to_string(),
            metadata,
            raw_bytes: None,
        }
    }

    fn make_claim(subject: &str, object: Option<&str>, text: &str) -> ExtractedClaim {
        ExtractedClaim {
            claim_text: text.to_string(),
            subject_name: subject.to_string(),
            predicate: "announced".to_string(),
            object_name: object.map(|s| s.to_string()),
            object_value: None,
            claim_type: ClaimType::Relation,
            sentiment: None,
            confidence: 0.6,
            evidence_snippet: text.to_string(),
            attributed_to: None,
        }
    }

    #[test]
    fn first_claim_from_includes_document_id_and_index() {
        let document = make_document(Some("Austin"), HashMap::new());
        let claim = make_claim("Apple Inc", Some("Austin"), "Apple Inc announced.");
        let first = first_claim_from(&document, &claim, 2);
        assert_eq!(first.claim_id, format!("{}#2", document.id));
        assert_eq!(first.subject_name, "Apple Inc");
        assert_eq!(first.object_name.as_deref(), Some("Austin"));
        assert_eq!(
            first.document_id.as_deref(),
            Some(document.id.to_string().as_str())
        );
        assert_eq!(first.location.as_deref(), Some("Austin"));
        assert!((first.confidence - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn first_claim_from_prefers_metadata_location() {
        let mut metadata = HashMap::new();
        metadata.insert("location".to_string(), json!("Berlin"));
        let document = make_document(Some("Austin"), metadata);
        let claim = make_claim("Apple Inc", Some("Austin"), "Apple Inc announced.");
        let first = first_claim_from(&document, &claim, 0);
        assert_eq!(first.location.as_deref(), Some("Berlin"));
    }

    #[test]
    fn first_claim_from_returns_none_location_when_absent() {
        let document = make_document(None, HashMap::new());
        let claim = make_claim("Apple Inc", None, "Apple Inc announced.");
        let first = first_claim_from(&document, &claim, 0);
        assert!(first.location.is_none());
    }
}
