use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use objective_core::traits::{DocumentRepository, ExtractionRepository};
use objective_core::ObjectiveConfig;
use objective_correlation::EventRepository;
use objective_ingestion::SourceRegistry;
use objective_message_bus::InMemoryMessageBus;
use objective_model_runtime::local::LocalModelRuntime;
use objective_plugin_host::PluginHost;
use objective_store::{monitoring::MonitoringService, recovery::RecoveryService};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::routes::{
    auxiliary::AuxiliaryStores, broadcasts, claims, config, contradictions, derived_events, docs,
    documents, entities, events, export, extractions, health, model_runtime, monitoring,
    narratives, plugins, recovery, search, sources, stats,
};
use crate::ws::{ws_handler, WebSocketHub};

pub trait ApiRepository: DocumentRepository + ExtractionRepository {}

impl<T> ApiRepository for T where T: DocumentRepository + ExtractionRepository {}

/// Container state shared across every route handler in the API gateway.
///
/// The `event_repository` field is optional so the rest of the gateway can
/// be constructed without the correlation crate (e.g. for tests that only
/// exercise ingestion-visible routes).
#[derive(Clone)]
pub struct ApiState {
    pub store: Arc<dyn ApiRepository>,
    pub bus: Arc<InMemoryMessageBus>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub event_repository: Option<Arc<dyn EventRepository>>,
    pub monitoring: Option<Arc<MonitoringService>>,
    pub recovery: Option<Arc<RecoveryService>>,
    pub websocket_hub: Option<WebSocketHub>,
    pub source_registry: Option<Arc<SourceRegistry>>,
    pub plugin_host: Option<Arc<PluginHost<InMemoryMessageBus>>>,
    /// Live runtime handle (only populated for
    /// `ModelRuntimeConfig::Local`). Shared with the
    /// `RuntimeExtractionService` so `POST
    /// /api/v1/model-runtime/reload` can swap the inner
    /// `Arc<dyn ModelRuntime>` atomically.
    pub model_runtime: Option<Arc<ModelRuntimeHandle>>,
    pub auxiliary: AuxiliaryStores,
    pub config: Option<ObjectiveConfig>,
}

/// Reload-able model runtime handle shared by the
/// `/api/v1/model-runtime` routes and the live
/// `RuntimeExtractionService`. Lives here (not in the
/// `objective` binary) so the api-gateway does not have
/// to depend on a binary crate.
pub struct ModelRuntimeHandle {
    /// Live `Arc<dyn ModelRuntime>`. The
    /// `RuntimeExtractionService` clones the inner `Arc`
    /// on every chunk; `POST /reload` takes the write
    /// lock to perform the swap.
    pub swappable: Arc<tokio::sync::RwLock<Arc<dyn objective_core::traits::ModelRuntime>>>,
    /// Snapshot of the current `LocalModelRuntime` for the
    /// inventory + strategy endpoints. Refreshed by
    /// [`Self::reload`].
    pub view: Arc<tokio::sync::RwLock<LocalModelRuntime>>,
    /// Captured at startup; reload builds a new runtime
    /// from this.
    pub config: objective_core::LocalModelConfig,
}

impl ModelRuntimeHandle {
    /// Build a new `LocalModelRuntime` from the captured
    /// config, atomically swap it into the live slot, and
    /// refresh the view. Returns the new view snapshot.
    pub async fn reload(&self) -> objective_model_runtime::LocalModelRuntimeView {
        let new_runtime = LocalModelRuntime::from_config(self.config.clone());
        let new_arc: Arc<dyn objective_core::traits::ModelRuntime> = Arc::new(new_runtime.clone());
        {
            let mut current = self.swappable.write().await;
            *current = new_arc;
        }
        let view = new_runtime.view();
        *self.view.write().await = new_runtime;
        view
    }

    /// Read-only view snapshot.
    pub async fn snapshot(&self) -> objective_model_runtime::LocalModelRuntimeView {
        self.view.read().await.view()
    }
}

impl ApiState {
    pub fn new(store: Arc<dyn ApiRepository>, bus: Arc<InMemoryMessageBus>) -> Self {
        Self {
            store,
            bus,
            started_at: chrono::Utc::now(),
            event_repository: None,
            monitoring: None,
            recovery: None,
            websocket_hub: None,
            source_registry: None,
            plugin_host: None,
            model_runtime: None,
            auxiliary: AuxiliaryStores::new(),
            config: None,
        }
    }

    /// Attach the local model runtime handle so
    /// `/api/v1/model-runtime` becomes available. The
    /// `ModelRuntimeHandle` is shared with the
    /// `RuntimeExtractionService` so a
    /// `POST /api/v1/model-runtime/reload` can atomically
    /// swap the live `Arc<dyn ModelRuntime>` without
    /// rebuilding the API state. If unset, those routes
    /// respond with 503.
    pub fn with_model_runtime(mut self, handle: Arc<ModelRuntimeHandle>) -> Self {
        self.model_runtime = Some(handle);
        self
    }

    pub fn with_started_at(mut self, started_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.started_at = started_at;
        self
    }

    /// Attach an event repository so routes that surface derived events
    /// (the correlation engine output) can read from it.
    pub fn with_event_repository(mut self, repository: Arc<dyn EventRepository>) -> Self {
        self.event_repository = Some(repository);
        self
    }

    pub fn with_monitoring(mut self, monitoring: Arc<MonitoringService>) -> Self {
        self.monitoring = Some(monitoring);
        self
    }

    pub fn with_recovery(mut self, recovery: Arc<RecoveryService>) -> Self {
        self.recovery = Some(recovery);
        self
    }

    /// Attach a WebSocket hub so `/ws` becomes a live stream of bus
    /// events. If unset, the WebSocket route responds with 503.
    pub fn with_websocket_hub(mut self, hub: WebSocketHub) -> Self {
        self.websocket_hub = Some(hub);
        self
    }

    /// Attach a source registry so `/api/v1/source-registry` becomes
    /// available. If unset, those routes respond with 503.
    pub fn with_source_registry(mut self, registry: Arc<SourceRegistry>) -> Self {
        self.source_registry = Some(registry);
        self
    }

    /// Replace the bundled auxiliary stores (narratives, broadcasts,
    /// contradictions). Useful for tests that need to pre-seed data.
    pub fn with_auxiliary(mut self, auxiliary: AuxiliaryStores) -> Self {
        self.auxiliary = auxiliary;
        self
    }

    /// Attach the plugin host so `/api/v1/plugins` becomes available.
    /// If unset, those routes respond with 503.
    pub fn with_plugin_host(mut self, host: Arc<PluginHost<InMemoryMessageBus>>) -> Self {
        self.plugin_host = Some(host);
        self
    }

    /// Attach the active configuration so `/api/v1/config` can surface
    /// it. Without this the config endpoint returns 503.
    pub fn with_config(mut self, config: ObjectiveConfig) -> Self {
        self.config = Some(config);
        self
    }
}

pub fn build_router(state: ApiState) -> Router {
    let api_routes = Router::new()
        .route("/api/v1/health", get(health::get_health))
        .route("/api/v1/stats", get(stats::get_stats))
        .route("/api/v1/documents", get(documents::list_documents))
        .route("/api/v1/extractions", get(extractions::list_extractions))
        .route("/api/v1/events", get(events::list_events))
        .route(
            "/api/v1/derived-events",
            get(derived_events::list_derived_events),
        )
        .route(
            "/api/v1/derived-events/top",
            get(derived_events::list_top_derived_events),
        )
        .route(
            "/api/v1/derived-events/:id",
            get(derived_events::get_derived_event),
        )
        .route("/api/v1/entities", get(entities::list_entities))
        .route(
            "/api/v1/entities/summary",
            get(entities::list_entity_summary),
        )
        .route("/api/v1/entities/:name", get(entities::get_entity))
        .route("/api/v1/entities/merge", post(entities::merge_entities))
        .route("/api/v1/claims", get(claims::list_claims))
        .route("/api/v1/sources", get(sources::list_sources))
        .route(
            "/api/v1/source-registry",
            get(sources::list_registered_sources).post(sources::create_source),
        )
        .route(
            "/api/v1/source-registry/:name",
            get(sources::get_registered_source)
                .put(sources::update_source)
                .delete(sources::delete_source),
        )
        .route(
            "/api/v1/source-registry/:name/trigger",
            post(sources::trigger_source),
        )
        .route("/api/v1/narratives", get(narratives::list_narratives))
        .route("/api/v1/narratives/:id", get(narratives::get_narrative))
        .route(
            "/api/v1/contradictions",
            get(contradictions::list_contradictions),
        )
        .route(
            "/api/v1/contradictions/:id",
            get(contradictions::get_contradiction),
        )
        .route(
            "/api/v1/contradictions/:id/resolve",
            post(contradictions::resolve_contradiction),
        )
        .route("/api/v1/broadcasts", get(broadcasts::list_broadcasts))
        .route(
            "/api/v1/broadcasts/latest",
            get(broadcasts::latest_broadcast),
        )
        .route("/api/v1/broadcasts/:id", get(broadcasts::get_broadcast))
        .route(
            "/api/v1/broadcasts/generate",
            post(broadcasts::generate_broadcast),
        )
        .route("/api/v1/monitoring", get(monitoring::get_metrics))
        .route("/api/v1/recovery", get(recovery::get_recovery_state))
        .route(
            "/api/v1/recovery/check",
            post(recovery::post_recovery_check),
        )
        .route("/api/v1/export", get(export::export_data))
        .route("/api/v1/search", get(search::search))
        .route("/api/v1/config", get(config::get_config))
        .route("/api/v1/events/:id/resolve", post(events::resolve_event))
        .route("/api/v1/plugins", get(plugins::list_plugins))
        .route("/api/v1/plugins/:name", get(plugins::get_plugin))
        .route(
            "/api/v1/plugins/:name/restart",
            post(plugins::restart_plugin),
        )
        .route("/api/v1/plugins/reload", post(plugins::reload_plugins))
        .route(
            "/api/v1/model-runtime",
            get(model_runtime::get_model_runtime),
        )
        .route(
            "/api/v1/model-runtime/reload",
            post(model_runtime::post_model_runtime_reload),
        )
        .route("/api-docs/openapi.json", get(docs::get_openapi_spec))
        .with_state(state.clone());

    // The WebSocket route lives in its own sub-router so it can
    // carry a `WebSocketHub` as state without forcing every other
    // handler to type-erase around the hub.
    let ws_routes = if state.websocket_hub.is_some() {
        Router::new().route("/ws", get(ws_handler))
    } else {
        Router::new().route(
            "/ws",
            get(|| async {
                (
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    "websocket hub not configured",
                )
            }),
        )
    };

    api_routes
        .merge(ws_routes)
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
