use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigResponse {
    pub data_root: String,
    pub rest_port: u16,
    pub websocket_port: u16,
    pub cors_allowed_origins: Vec<String>,
    pub auth_enabled: bool,
    pub database_path: String,
    pub document_path: String,
    pub vector_path: String,
    pub queue_path: String,
    pub graph_path: String,
    pub embedding_path: String,
    pub nats_url: String,
    pub use_embedded_nats: bool,
    pub log_level: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigError {
    pub error: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/config",
    responses(
        (status = 200, description = "Active configuration", body = ConfigResponse),
        (status = 503, description = "Configuration is not exposed by this gateway", body = ConfigError)
    )
)]
pub async fn get_config(
    State(state): State<ApiState>,
) -> Result<Json<ConfigResponse>, (StatusCode, Json<ConfigError>)> {
    let Some(config) = state.config.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ConfigError {
                error: "configuration is not exposed by this gateway".to_string(),
            }),
        ));
    };

    Ok(Json(ConfigResponse {
        data_root: config.data_root.display().to_string(),
        rest_port: config.api.rest_port,
        websocket_port: config.api.websocket_port,
        cors_allowed_origins: config.api.cors_allowed_origins.clone(),
        auth_enabled: config.api.auth_enabled,
        database_path: config.storage.database_path.display().to_string(),
        document_path: config.storage.document_path.display().to_string(),
        vector_path: config.storage.vector_path.display().to_string(),
        queue_path: config.storage.queue_path.display().to_string(),
        graph_path: config.storage.graph_path.display().to_string(),
        embedding_path: config.storage.embedding_path.display().to_string(),
        nats_url: config.message_bus.nats_url.clone(),
        use_embedded_nats: config.message_bus.use_embedded,
        log_level: config.logging.level.clone(),
    }))
}
