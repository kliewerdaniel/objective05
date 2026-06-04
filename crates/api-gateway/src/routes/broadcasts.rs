use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::routes::auxiliary::{new_id, BroadcastRecord, BroadcastStatus};
use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct Broadcast {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub body_markdown: String,
    pub status: BroadcastStatus,
    pub event_count: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<BroadcastRecord> for Broadcast {
    fn from(record: BroadcastRecord) -> Self {
        Self {
            id: record.id,
            title: record.title,
            summary: record.summary,
            body_markdown: record.body_markdown,
            status: record.status,
            event_count: record.event_count,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BroadcastsResponse {
    pub broadcasts: Vec<Broadcast>,
    pub total: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BroadcastDetailResponse {
    pub broadcast: Broadcast,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BroadcastLatestResponse {
    pub broadcast: Option<Broadcast>,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BroadcastError {
    pub error: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BroadcastGenerateRequest {
    /// Optional override for the generated broadcast title.
    pub title: Option<String>,
    /// Optional focus area — e.g. "supplier", "Austin", or a claim id.
    pub focus: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/broadcasts",
    responses((status = 200, description = "List recent broadcasts", body = BroadcastsResponse))
)]
pub async fn list_broadcasts(State(state): State<ApiState>) -> Json<BroadcastsResponse> {
    let records = state.auxiliary.broadcasts.list().await;
    let total = records.len();
    let broadcasts: Vec<Broadcast> = records.into_iter().map(Into::into).collect();
    Json(BroadcastsResponse { broadcasts, total })
}

#[utoipa::path(
    get,
    path = "/api/v1/broadcasts/latest",
    responses(
        (status = 200, description = "Most recent broadcast, if any", body = BroadcastLatestResponse)
    )
)]
pub async fn latest_broadcast(State(state): State<ApiState>) -> Json<BroadcastLatestResponse> {
    match state.auxiliary.broadcasts.latest().await {
        Some(broadcast) => Json(BroadcastLatestResponse {
            broadcast: Some(broadcast.into()),
            message: "ok".to_string(),
        }),
        None => Json(BroadcastLatestResponse {
            broadcast: None,
            message: "no broadcasts have been generated yet".to_string(),
        }),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/broadcasts/{id}",
    params(("id" = String, Path, description = "Broadcast ID")),
    responses(
        (status = 200, description = "Broadcast detail", body = BroadcastDetailResponse),
        (status = 404, description = "Broadcast not found", body = BroadcastError)
    )
)]
pub async fn get_broadcast(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<BroadcastDetailResponse>, (StatusCode, Json<BroadcastError>)> {
    match state.auxiliary.broadcasts.get(&id).await {
        Some(broadcast) => Ok(Json(BroadcastDetailResponse {
            broadcast: broadcast.into(),
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(BroadcastError {
                error: format!("broadcast not found: {id}"),
            }),
        )),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/broadcasts/generate",
    request_body = BroadcastGenerateRequest,
    responses(
        (status = 201, description = "Broadcast generated and stored", body = BroadcastDetailResponse)
    )
)]
pub async fn generate_broadcast(
    State(state): State<ApiState>,
    Json(request): Json<BroadcastGenerateRequest>,
) -> (StatusCode, Json<BroadcastDetailResponse>) {
    let now = chrono::Utc::now();
    let id = new_id();
    let title = request
        .title
        .clone()
        .unwrap_or_else(|| format!("Objective Pulse — {}", now.format("%Y-%m-%d %H:%M UTC")));
    let focus = request.focus.unwrap_or_else(|| "all".to_string());
    let body_markdown = format!(
        "# {title}\n\n_Generated on {ts} focusing on **{focus}**._\n\n\
         This is an on-demand broadcast. The full streaming pipeline will fill in \
         the body with detected events, narratives, and contradictions.\n",
        title = title,
        ts = now.to_rfc3339(),
        focus = focus
    );
    let summary = format!(
        "On-demand broadcast focused on {focus}. Events, narratives, and contradictions will be woven in as the pipeline matures."
    );
    let record = BroadcastRecord {
        id,
        title,
        summary,
        body_markdown,
        status: BroadcastStatus::Draft,
        event_count: 0,
        created_at: now,
        updated_at: now,
    };
    state.auxiliary.broadcasts.insert(record.clone()).await;
    (
        StatusCode::CREATED,
        Json(BroadcastDetailResponse {
            broadcast: record.into(),
        }),
    )
}
