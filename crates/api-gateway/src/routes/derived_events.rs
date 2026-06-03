use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::server::ApiState;

/// Default maximum number of derived events returned when no limit is
/// supplied by the caller.
const DEFAULT_LIMIT: usize = 50;
/// Hard cap on the number of derived events returned in a single response.
const MAX_LIMIT: usize = 500;

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct DerivedEventsQuery {
    /// Optional cap on the number of events returned.
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DerivedEventsResponse {
    /// Events ordered by `last_updated_at` descending.
    pub events: Vec<serde_json::Value>,
    pub count: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DerivedEventError {
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DerivedEventDetailResponse {
    pub event: serde_json::Value,
}

fn clamp_limit(requested: Option<usize>) -> usize {
    requested.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

#[utoipa::path(
    get,
    path = "/api/v1/derived-events",
    params(DerivedEventsQuery),
    responses(
        (status = 200, description = "Derived events produced by the correlation engine", body = DerivedEventsResponse),
        (status = 503, description = "Event repository not configured for this instance", body = DerivedEventError),
    )
)]
pub async fn list_derived_events(
    State(state): State<ApiState>,
    Query(query): Query<DerivedEventsQuery>,
) -> Result<Json<DerivedEventsResponse>, (StatusCode, Json<DerivedEventError>)> {
    let Some(repository) = state.event_repository.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(DerivedEventError {
                error: "derived event repository is not configured for this instance".to_string(),
            }),
        ));
    };

    let limit = clamp_limit(query.limit);
    let events = repository.list_events().await.unwrap_or_default();
    let truncated: Vec<_> = events.into_iter().take(limit).collect();
    let body = DerivedEventsResponse {
        count: truncated.len(),
        events: truncated
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default(),
    };

    Ok(Json(body))
}

#[utoipa::path(
    get,
    path = "/api/v1/derived-events/top",
    params(DerivedEventsQuery),
    responses(
        (status = 200, description = "Top derived events sorted by importance", body = DerivedEventsResponse),
        (status = 503, description = "Event repository not configured for this instance", body = DerivedEventError),
    )
)]
pub async fn list_top_derived_events(
    State(state): State<ApiState>,
    Query(query): Query<DerivedEventsQuery>,
) -> Result<Json<DerivedEventsResponse>, (StatusCode, Json<DerivedEventError>)> {
    let Some(repository) = state.event_repository.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(DerivedEventError {
                error: "derived event repository is not configured for this instance".to_string(),
            }),
        ));
    };

    let limit = clamp_limit(query.limit);
    let events = repository.list_events().await.unwrap_or_default();
    let mut sorted = events;
    sorted.sort_by(|left, right| {
        right
            .importance
            .partial_cmp(&left.importance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let truncated: Vec<_> = sorted.into_iter().take(limit).collect();
    let body = DerivedEventsResponse {
        count: truncated.len(),
        events: truncated
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default(),
    };

    Ok(Json(body))
}

#[utoipa::path(
    get,
    path = "/api/v1/derived-events/{id}",
    params(
        ("id" = String, Path, description = "Event ID")
    ),
    responses(
        (status = 200, description = "Derived event detail", body = DerivedEventDetailResponse),
        (status = 404, description = "Event not found", body = DerivedEventError),
        (status = 503, description = "Event repository not configured for this instance", body = DerivedEventError),
    )
)]
pub async fn get_derived_event(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<DerivedEventDetailResponse>, (StatusCode, Json<DerivedEventError>)> {
    let Some(repository) = state.event_repository.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(DerivedEventError {
                error: "derived event repository is not configured for this instance".to_string(),
            }),
        ));
    };

    let events = repository.list_events().await.unwrap_or_default();
    let event = events.into_iter().find(|e| e.id.to_string() == id);

    match event {
        Some(event) => {
            let value = serde_json::to_value(&event).unwrap_or_default();
            Ok(Json(DerivedEventDetailResponse { event: value }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(DerivedEventError {
                error: format!("derived event not found: {id}"),
            }),
        )),
    }
}
