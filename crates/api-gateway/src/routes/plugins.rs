//! Plugin host routes.
//!
//! - `GET    /api/v1/plugins`         — list every registered plugin
//! - `GET    /api/v1/plugins/:name`   — read a single plugin
//! - `POST   /api/v1/plugins/:name/restart` — force restart a plugin
//! - `POST   /api/v1/plugins/reload`  — re-validate the registry
//!
//! All routes return 503 when no plugin host has been attached to
//! the `ApiState`. Per the documented contract, only manifest
//! `name` is exposed; events handled, restart count, and last
//! error are surfaced so the dashboard can flag unhealthy plugins.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use objective_message_bus::InMemoryMessageBus;
use objective_plugin_host::PluginHost;
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::ApiState;

#[derive(Debug, Serialize, ToSchema)]
pub struct PluginStatusResponse {
    pub name: String,
    pub version: String,
    pub plugin_type: String,
    pub description: String,
    pub state: String,
    pub events_handled: u64,
    pub restart_count: u32,
    pub last_event_at: Option<String>,
    pub last_error: Option<String>,
    pub started_at: Option<String>,
    pub subscriptions: PluginSubscriptions,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PluginSubscriptions {
    pub event_types: Vec<String>,
    pub entity_types: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PluginsListResponse {
    pub plugins: Vec<PluginStatusResponse>,
    pub total: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PluginErrorResponse {
    pub error: String,
    pub message: String,
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(PluginErrorResponse {
            error: "plugin_host_unavailable".to_string(),
            message: "plugin host is not configured for this daemon".to_string(),
        }),
    )
        .into_response()
}

fn host(state: &ApiState) -> Result<Arc<PluginHost<InMemoryMessageBus>>, Response> {
    match state.plugin_host.as_ref() {
        Some(host) => Ok(Arc::clone(host)),
        None => Err(unavailable()),
    }
}

fn status_to_response(s: &objective_plugin_host::PluginStatus) -> PluginStatusResponse {
    PluginStatusResponse {
        name: s.manifest.name.clone(),
        version: s.manifest.version.clone(),
        plugin_type: s.manifest.plugin_type.as_str().to_string(),
        description: s.manifest.description.clone(),
        state: s.state.as_str().to_string(),
        events_handled: s.events_handled,
        restart_count: s.restart_count,
        last_event_at: s.last_event_at.map(|t| t.to_rfc3339()),
        last_error: s.last_error.clone(),
        started_at: s.started_at.map(|t| t.to_rfc3339()),
        subscriptions: PluginSubscriptions {
            event_types: s.manifest.subscriptions.event_types.clone(),
            entity_types: s.manifest.subscriptions.entity_types.clone(),
        },
        capabilities: s.manifest.capabilities.clone(),
    }
}

/// `GET /api/v1/plugins`
#[utoipa::path(
    get,
    path = "/api/v1/plugins",
    tag = "plugins",
    responses(
        (status = 200, description = "List plugins", body = PluginsListResponse),
        (status = 503, description = "Plugin host not configured", body = PluginErrorResponse),
    )
)]
pub async fn list_plugins(State(state): State<ApiState>) -> Response {
    let host = match host(&state) {
        Ok(h) => h,
        Err(resp) => return resp,
    };
    let plugins = host.list().await;
    let total = plugins.len() as u32;
    let body = PluginsListResponse {
        plugins: plugins.iter().map(status_to_response).collect(),
        total,
    };
    Json(body).into_response()
}

/// `GET /api/v1/plugins/{name}`
#[utoipa::path(
    get,
    path = "/api/v1/plugins/{name}",
    tag = "plugins",
    params(
        ("name" = String, Path, description = "Plugin name"),
    ),
    responses(
        (status = 200, description = "Plugin status", body = PluginStatusResponse),
        (status = 404, description = "Plugin not found", body = PluginErrorResponse),
        (status = 503, description = "Plugin host not configured", body = PluginErrorResponse),
    )
)]
pub async fn get_plugin(State(state): State<ApiState>, Path(name): Path<String>) -> Response {
    let host = match host(&state) {
        Ok(h) => h,
        Err(resp) => return resp,
    };
    match host.get(&name).await {
        Some(status) => Json(status_to_response(&status)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(PluginErrorResponse {
                error: "plugin_not_found".to_string(),
                message: format!("plugin '{name}' is not registered"),
            }),
        )
            .into_response(),
    }
}

/// `POST /api/v1/plugins/{name}/restart`
#[utoipa::path(
    post,
    path = "/api/v1/plugins/{name}/restart",
    tag = "plugins",
    params(
        ("name" = String, Path, description = "Plugin name"),
    ),
    responses(
        (status = 200, description = "Plugin restarted", body = PluginStatusResponse),
        (status = 404, description = "Plugin not found", body = PluginErrorResponse),
        (status = 503, description = "Plugin host not configured", body = PluginErrorResponse),
    )
)]
pub async fn restart_plugin(State(state): State<ApiState>, Path(name): Path<String>) -> Response {
    let host = match host(&state) {
        Ok(h) => h,
        Err(resp) => return resp,
    };
    match host.restart(&name).await {
        Ok(status) => Json(status_to_response(&status)).into_response(),
        Err(objective_plugin_host::HostError::UnknownPlugin(_)) => (
            StatusCode::NOT_FOUND,
            Json(PluginErrorResponse {
                error: "plugin_not_found".to_string(),
                message: format!("plugin '{name}' is not registered"),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PluginErrorResponse {
                error: "plugin_restart_failed".to_string(),
                message: e.to_string(),
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PluginReloadResponse {
    pub reloaded: u32,
    pub plugin_names: Vec<String>,
}

/// `POST /api/v1/plugins/reload`
#[utoipa::path(
    post,
    path = "/api/v1/plugins/reload",
    tag = "plugins",
    responses(
        (status = 200, description = "Reload complete", body = PluginReloadResponse),
        (status = 503, description = "Plugin host not configured", body = PluginErrorResponse),
    )
)]
pub async fn reload_plugins(State(state): State<ApiState>) -> Response {
    let host = match host(&state) {
        Ok(h) => h,
        Err(resp) => return resp,
    };
    let path = std::path::PathBuf::from(".objective/plugins");
    let names = match host.reload(&path).await {
        Ok(names) => names,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PluginErrorResponse {
                    error: "plugin_reload_failed".to_string(),
                    message: e.to_string(),
                }),
            )
                .into_response();
        }
    };
    let reloaded = names.len() as u32;
    Json(PluginReloadResponse {
        reloaded,
        plugin_names: names,
    })
    .into_response()
}
