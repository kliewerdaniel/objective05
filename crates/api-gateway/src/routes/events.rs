use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use objective_core::{traits::MessageBus, types::EventEnvelope};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct EventRecord {
    pub subject: String,
    pub event: EventEnvelope,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventsResponse {
    pub events: Vec<EventRecord>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventError {
    pub error: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EventResolveRequest {
    /// Optional resolution note recorded in metadata.
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventResolveResponse {
    pub event: serde_json::Value,
    pub message: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/events",
    responses((status = 200, description = "List recent internal events", body = EventsResponse))
)]
pub async fn list_events(State(state): State<ApiState>) -> Json<EventsResponse> {
    let events = state
        .bus
        .events()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(subject, event)| EventRecord { subject, event })
        .collect();

    Json(EventsResponse { events })
}

#[utoipa::path(
    post,
    path = "/api/v1/events/{id}/resolve",
    params(("id" = String, Path, description = "Derived event ID (ULID)")),
    request_body = EventResolveRequest,
    responses(
        (status = 200, description = "Event marked as resolved", body = EventResolveResponse),
        (status = 404, description = "Event not found", body = EventError),
        (status = 503, description = "Event repository not configured", body = EventError)
    )
)]
pub async fn resolve_event(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(request): Json<EventResolveRequest>,
) -> Result<Json<EventResolveResponse>, (StatusCode, Json<EventError>)> {
    use objective_core::types::EventStatus;

    let Some(repository) = state.event_repository.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(EventError {
                error: "event repository is not configured for this instance".to_string(),
            }),
        ));
    };

    let mut event = match repository.get_event(&id).await {
        Ok(Some(event)) => event,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(EventError {
                    error: format!("event not found: {id}"),
                }),
            ));
        }
        Err(error) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(EventError {
                    error: format!("failed to load event: {error}"),
                }),
            ));
        }
    };

    event.status = EventStatus::Resolved;
    event.last_updated_at = chrono::Utc::now();
    if let Some(note) = request.note.clone() {
        event.metadata.insert(
            "resolution_note".to_string(),
            serde_json::Value::String(note),
        );
    }
    event.metadata.insert(
        "resolved_at".to_string(),
        serde_json::Value::String(event.last_updated_at.to_rfc3339()),
    );

    if let Err(error) = repository.save_event(event.clone()).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(EventError {
                error: format!("failed to save event: {error}"),
            }),
        ));
    }

    let value = serde_json::to_value(&event).unwrap_or_default();
    Ok(Json(EventResolveResponse {
        event: value,
        message: "event marked as resolved".to_string(),
    }))
}
