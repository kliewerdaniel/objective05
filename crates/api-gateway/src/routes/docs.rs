use axum::{http::header, response::IntoResponse, Json};
use utoipa::OpenApi;

use crate::routes::{
    claims::ClaimsResponse,
    derived_events::{DerivedEventError, DerivedEventsQuery, DerivedEventsResponse},
    documents::DocumentsResponse,
    entities::{EntitiesResponse, EntitySummary, EntitySummaryResponse},
    events::EventsResponse,
    extractions::ExtractionsResponse,
    health::{HealthResponse, ServiceHealth},
    monitoring::MonitoringResponse,
    stats::StatsResponse,
};

/// OpenAPI specification for the Objective API gateway.
///
/// Aggregates the in-place `#[utoipa::path]` annotations defined on every
/// route handler in this crate so a single JSON document can be served to
/// clients and external tooling.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Objective API",
        version = "0.1.0",
        description = "Internal API for the local-first Objective intelligence platform.",
        contact(name = "Objective", url = "https://github.com/kliewerdaniel/objective05")
    ),
    paths(
        crate::routes::health::get_health,
        crate::routes::stats::get_stats,
        crate::routes::monitoring::get_metrics,
        crate::routes::documents::list_documents,
        crate::routes::extractions::list_extractions,
        crate::routes::events::list_events,
        crate::routes::derived_events::list_derived_events,
        crate::routes::derived_events::list_top_derived_events,
        crate::routes::entities::list_entities,
        crate::routes::entities::list_entity_summary,
        crate::routes::claims::list_claims,
    ),
    components(
        schemas(
            HealthResponse,
            ServiceHealth,
            StatsResponse,
            MonitoringResponse,
            objective_store::monitoring::PipelineMetrics,
            DocumentsResponse,
            ExtractionsResponse,
            EventsResponse,
            DerivedEventsQuery,
            DerivedEventsResponse,
            DerivedEventError,
            EntitiesResponse,
            EntitySummary,
            EntitySummaryResponse,
            ClaimsResponse,
            objective_core::types::BodyFormat,
            objective_core::types::RawDocument,
            objective_core::types::EntityType,
            objective_core::types::ExtractedEntity,
            objective_core::types::ClaimType,
            objective_core::types::ExtractedClaim,
            objective_core::types::ExtractedRelationship,
            objective_core::types::ExtractionResult,
            objective_core::types::EventEnvelope,
            objective_core::types::EventMetadata,
            objective_core::types::Event,
            objective_core::types::EventStatus,
            objective_core::types::EventType,
        )
    ),
    tags(
        (name = "health", description = "Service health and uptime."),
        (name = "stats", description = "Aggregate system statistics."),
        (name = "monitoring", description = "Pipeline monitoring metrics and operational visibility."),
        (name = "documents", description = "Normalized documents ingested from sources."),
        (name = "extractions", description = "Entities, claims, and relationships extracted from documents."),
        (name = "events", description = "Internal event bus traffic."),
        (name = "derived-events", description = "Events produced by the correlation engine from extracted claims."),
        (name = "entities", description = "Entities aggregated across all extractions."),
        (name = "claims", description = "Claims extracted from documents."),
    )
)]
pub struct ApiDoc;

pub async fn get_openapi_spec() -> impl IntoResponse {
    let spec = ApiDoc::openapi();
    let body = Json(spec);
    ([(header::CONTENT_TYPE, "application/json")], body)
}
