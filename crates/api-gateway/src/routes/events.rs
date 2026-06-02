use axum::{extract::State, Json};
use objective_core::{traits::MessageBus, types::EventEnvelope};
use serde::Serialize;
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
