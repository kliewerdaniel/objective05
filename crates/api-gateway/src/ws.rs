//! Real-time WebSocket bridge for the Objective API.
//!
//! The dashboard expects push notifications for new events as the
//! ingestion, extraction, and correlation pipelines produce them. The
//! [`MessageBus`](objective_core::traits::MessageBus) abstraction in
//! this MVP is a polling-style bus (the in-memory implementation
//! records published events and exposes them via `events()`); there
//! is no native subscription. This module bridges that gap by
//! repeatedly polling the bus on a short interval and re-broadcasting
//! the new events through a tokio broadcast channel that connected
//! WebSocket clients listen on.
//!
//! ## Wire protocol
//!
//! Clients connect to `/ws` and receive a `welcome` envelope on
//! connect, then push notifications for every event the bus records.
//! The notification shape is the same as the documented internal
//! WebSocket protocol (`docs/api/internal-api.md`):
//!
//! ```json
//! { "type": "event", "data": { "subject": "ingestion.document.received", "envelope": {...} } }
//! { "type": "system", "data": { "event_type": "system.heartbeat", "payload": {...} } }
//! { "type": "broadcast", "data": { "broadcast_id": "...", "title": "...", "status": "..." } }
//! ```
//!
//! Clients can send short control messages to refine the stream:
//!
//! - `{ "type": "ping" }` — server responds with `{ "type": "pong" }`.
//! - `{ "type": "subscribe", "channels": ["events", "system"] }` —
//!   restrict the stream to the given logical channels. Channels map
//!   to the `event_type` prefix: `events` covers `ingestion.*`,
//!   `extraction.*`, and `correlation.*`; `broadcast` covers
//!   `broadcast.*`; `system` covers `system.*` and
//!   `recovery.*`/`scheduler.*`/`monitoring.*`. An empty list resets
//!   the filter to "all".
//! - `{ "type": "unsubscribe", "channels": ["system"] }` — drop the
//!   listed channels from the filter.
//!
//! The server is intentionally conservative: malformed messages and
//! network errors are logged and the client is dropped rather than
//! tearing the hub down for everyone else.

use std::{collections::HashSet, sync::Arc, time::Duration};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use objective_core::traits::MessageBus;
use objective_message_bus::InMemoryMessageBus;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::server::ApiState;

/// Polling interval used to read newly published events from the
/// underlying message bus.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Maximum number of in-flight broadcast messages kept in the
/// per-hub channel. Late subscribers miss older events; this is
/// the standard tokio broadcast trade-off and is sized to absorb
/// short bursts without stalling publishers.
const HUB_CHANNEL_CAPACITY: usize = 1024;

/// What the hub pushes to connected WebSocket clients. Mirrors the
/// documented internal WebSocket envelope shape so the dashboard
/// code does not need to special-case server-pushed events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HubMessage {
    /// Initial envelope sent right after the WebSocket handshake.
    Welcome { data: WelcomeData },
    /// Real-time bus event. The dashboard decodes the embedded
    /// envelope and routes it to the relevant page.
    Event { data: EventData },
    /// Reply to a client ping, used as a keep-alive.
    Pong { data: PongData },
    /// Server-side error notification (e.g. failure to serialize a
    /// particular envelope). Non-fatal.
    Error { data: ErrorData },
}

/// Body of a `welcome` envelope. Carries the server startup time
/// so clients can compute their own offset against the canonical
/// timeline.
#[derive(Debug, Clone, Serialize)]
pub struct WelcomeData {
    pub server_started_at: chrono::DateTime<chrono::Utc>,
    pub message: String,
}

/// Body of an `event` envelope. The `envelope` field is the full
/// [`EventEnvelope`](objective_core::types::EventEnvelope) as
/// serialized JSON; `event_type` and `subject` are hoisted up so
/// clients can route on them without deserializing the envelope.
#[derive(Debug, Clone, Serialize)]
pub struct EventData {
    pub subject: String,
    pub event_type: String,
    pub envelope: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct PongData {
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorData {
    pub message: String,
}

/// Logical channel categories a client can subscribe to. The values
/// are stable strings the dashboard can switch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Events,
    Broadcast,
    System,
}

impl Channel {
    fn matches(self, event_type: &str) -> bool {
        let prefix = match self {
            Channel::Events => {
                if event_type.starts_with("ingestion.")
                    || event_type.starts_with("extraction.")
                    || event_type.starts_with("correlation.")
                {
                    return true;
                }
                "events"
            }
            Channel::Broadcast => "broadcast",
            Channel::System => "system",
        };
        event_type.starts_with(prefix)
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Channel::Events => "events",
            Channel::Broadcast => "broadcast",
            Channel::System => "system",
        };
        f.write_str(s)
    }
}

/// Control messages the dashboard (or any client) can send to the
/// server. Unknown variants are ignored, not rejected.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Ping,
    Subscribe { channels: Vec<Channel> },
    Unsubscribe { channels: Vec<Channel> },
}

/// Central fan-out for the bus. A single hub is shared by every
/// connected client; the hub itself runs a background task that
/// polls the bus.
#[derive(Clone)]
pub struct WebSocketHub {
    sender: broadcast::Sender<HubMessage>,
}

impl WebSocketHub {
    /// Build a new hub and spawn the background poller. The returned
    /// hub is cheap to clone (an `Arc` to a broadcast sender) and is
    /// what the WebSocket handler will use to subscribe new
    /// connections.
    pub fn spawn(bus: Arc<InMemoryMessageBus>) -> Self {
        let (sender, _) = broadcast::channel(HUB_CHANNEL_CAPACITY);
        let hub = Self {
            sender: sender.clone(),
        };
        tokio::spawn(poll_bus(bus, sender));
        hub
    }

    /// Build a hub that uses the supplied broadcast sender. Useful
    /// for tests that want to inject a deterministic bus stand-in.
    pub fn with_sender(sender: broadcast::Sender<HubMessage>) -> Self {
        Self { sender }
    }

    /// Subscribe a new client. Each subscriber gets its own receiver
    /// so a slow client does not back up the rest of the hub.
    pub fn subscribe(&self) -> broadcast::Receiver<HubMessage> {
        self.sender.subscribe()
    }

    /// Send a hub message to every connected client. Returns the
    /// number of receivers that received the message.
    pub fn broadcast(&self, message: HubMessage) -> usize {
        self.sender.send(message).unwrap_or(0)
    }

    /// Number of currently registered receivers.
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

/// Background task that drains the bus on a short interval and
/// republishes new events through the broadcast channel.
async fn poll_bus(bus: Arc<InMemoryMessageBus>, sender: broadcast::Sender<HubMessage>) {
    let mut last_index: usize = 0;
    let mut ticker = tokio::time::interval(POLL_INTERVAL);
    info!(
        poll_interval_ms = POLL_INTERVAL.as_millis() as u64,
        "websocket hub poller started"
    );
    loop {
        ticker.tick().await;
        let events = match bus.events().await {
            Ok(events) => events,
            Err(e) => {
                warn!(error = %e, "bus poll failed; will retry");
                continue;
            }
        };
        if events.len() <= last_index {
            continue;
        }
        for (subject, envelope) in &events[last_index..] {
            let message = HubMessage::Event {
                data: EventData {
                    subject: subject.clone(),
                    event_type: envelope.event_type.clone(),
                    envelope: serde_json::to_value(envelope).unwrap_or(Value::Null),
                },
            };
            // If no receivers are connected, drop on the floor.
            let _ = sender.send(message);
        }
        last_index = events.len();
    }
}

/// Axum WebSocket handler mounted on the gateway router. Pulls the
/// [`WebSocketHub`] from the shared [`ApiState`] and runs the
/// connection loop until the client disconnects.
pub async fn ws_handler(
    axum::extract::State(state): axum::extract::State<ApiState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let Some(hub) = state.websocket_hub.clone() else {
        debug!("ws_handler invoked without a configured hub; refusing upgrade");
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "websocket hub not configured",
        )
            .into_response();
    };
    ws.on_upgrade(move |socket| serve(hub, socket))
}

/// Run a single WebSocket connection until the client disconnects.
/// Exposed separately from the axum handler so callers can wire it
/// to a closure-based route when they do not want to mount a
/// dedicated sub-router for the WebSocket state.
pub async fn serve(hub: WebSocketHub, socket: WebSocket) {
    let started_at = chrono::Utc::now();
    info!("websocket client connected");

    let mut receiver = hub.subscribe();
    let _ = hub.broadcast(HubMessage::Welcome {
        data: WelcomeData {
            server_started_at: started_at,
            message: "connected to objective realtime stream".to_string(),
        },
    });

    let (mut sender, mut client) = socket.split();

    // Per-connection filter so a dashboard view of "events only"
    // does not get flooded by system heartbeats.
    let active_channels: Arc<RwLock<Option<HashSet<Channel>>>> = Arc::new(RwLock::new(None));
    let hub_for_writes = hub.clone();

    // Task: read from client and apply control messages.
    let channels_for_read = Arc::clone(&active_channels);
    let read_task = tokio::spawn(async move {
        while let Some(message) = client.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    if let Ok(parsed) = serde_json::from_str::<ClientMessage>(&text) {
                        apply_control(&channels_for_read, parsed).await;
                    } else {
                        debug!(payload = %text, "ignoring malformed client message");
                    }
                }
                Ok(Message::Binary(_)) => {
                    debug!("ignoring binary websocket frame");
                }
                Ok(Message::Ping(payload)) => {
                    let _ = hub_for_writes.broadcast(HubMessage::Pong {
                        data: PongData {
                            timestamp: chrono::Utc::now(),
                        },
                    });
                    let _ = payload; // axum auto-replies to pings
                }
                Ok(Message::Pong(_)) => {}
                Ok(Message::Close(reason)) => {
                    info!(?reason, "websocket client closed connection");
                    break;
                }
                Err(e) => {
                    warn!(error = %e, "websocket read error");
                    break;
                }
            }
        }
    });

    // Loop: forward hub events to the client, filtered by their
    // current subscription.
    loop {
        let message = match receiver.recv().await {
            Ok(message) => message,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                warn!(skipped, "websocket client lagged; dropping messages");
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => {
                debug!("hub channel closed");
                break;
            }
        };
        if !passes_filter(&active_channels, &message).await {
            continue;
        }
        let payload = match serde_json::to_string(&message) {
            Ok(payload) => payload,
            Err(e) => {
                error!(error = %e, "failed to serialize hub message");
                continue;
            }
        };
        if let Err(e) = sender.send(Message::Text(payload)).await {
            debug!(error = %e, "websocket write failed; closing");
            break;
        }
    }

    read_task.abort();
    info!("websocket client disconnected");
}

async fn apply_control(channels: &Arc<RwLock<Option<HashSet<Channel>>>>, message: ClientMessage) {
    match message {
        ClientMessage::Ping => {
            // No-op here; pong is emitted when we receive a Ping
            // frame. Text-based pings are ignored so clients do not
            // accidentally trigger the pong path.
        }
        ClientMessage::Subscribe { channels: wanted } => {
            let mut guard = channels.write().await;
            if wanted.is_empty() {
                *guard = None;
            } else {
                let entry = guard.get_or_insert_with(HashSet::new);
                for channel in wanted {
                    entry.insert(channel);
                }
            }
        }
        ClientMessage::Unsubscribe { channels: dropped } => {
            let mut guard = channels.write().await;
            if let Some(active) = guard.as_mut() {
                for channel in dropped {
                    active.remove(&channel);
                }
                if active.is_empty() {
                    *guard = None;
                }
            }
        }
    }
}

async fn passes_filter(
    channels: &Arc<RwLock<Option<HashSet<Channel>>>>,
    message: &HubMessage,
) -> bool {
    let active = channels.read().await;
    let Some(active) = active.as_ref() else {
        return true;
    };
    if active.is_empty() {
        return true;
    }
    match message {
        HubMessage::Welcome { .. } | HubMessage::Pong { .. } | HubMessage::Error { .. } => true,
        HubMessage::Event { data } => active
            .iter()
            .any(|channel| channel.matches(&data.event_type)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_core::types::EventEnvelope;
    use serde_json::json;
    use std::time::Duration as StdDuration;
    use tokio::time::timeout;

    fn make_bus() -> Arc<InMemoryMessageBus> {
        Arc::new(InMemoryMessageBus::new())
    }

    #[test]
    fn test_channel_matches_event_prefixes() {
        assert!(Channel::Events.matches("ingestion.document.received"));
        assert!(Channel::Events.matches("extraction.document.processed"));
        assert!(Channel::Events.matches("correlation.contradiction.detected"));
        assert!(Channel::Broadcast.matches("broadcast.generated"));
        assert!(Channel::System.matches("system.heartbeat"));
        assert!(Channel::System.matches("system.service.crash"));
        assert!(!Channel::System.matches("ingestion.document.received"));
        assert!(!Channel::Broadcast.matches("system.heartbeat"));
    }

    #[tokio::test]
    async fn test_hub_subscribe_returns_independent_receivers() {
        let (sender, _) = broadcast::channel(8);
        let hub = WebSocketHub::with_sender(sender);

        let mut a = hub.subscribe();
        let mut b = hub.subscribe();

        hub.broadcast(HubMessage::Pong {
            data: PongData {
                timestamp: chrono::Utc::now(),
            },
        });

        let recv_a = timeout(StdDuration::from_millis(50), a.recv())
            .await
            .expect("a should receive")
            .expect("a should not error");
        let recv_b = timeout(StdDuration::from_millis(50), b.recv())
            .await
            .expect("b should receive")
            .expect("b should not error");

        assert!(matches!(recv_a, HubMessage::Pong { .. }));
        assert!(matches!(recv_b, HubMessage::Pong { .. }));
    }

    #[tokio::test]
    async fn test_hub_poll_task_picks_up_new_events() {
        let bus = make_bus();
        let hub = WebSocketHub::spawn(bus.clone());
        let mut receiver = hub.subscribe();

        bus.publish(
            "ingestion.document.received",
            EventEnvelope::new(
                "ingestion.document.received",
                "test.source",
                json!({"document_id": "abc"}),
            ),
        )
        .await
        .unwrap();

        let event = wait_for_event(&mut receiver, "ingestion.document.received").await;
        match event {
            HubMessage::Event { data } => {
                assert_eq!(data.event_type, "ingestion.document.received");
            }
            other => panic!("expected Event message, got {other:?}"),
        }
    }

    async fn wait_for_event(
        receiver: &mut broadcast::Receiver<HubMessage>,
        expected: &str,
    ) -> HubMessage {
        let deadline = StdDuration::from_secs(2);
        timeout(deadline, async {
            loop {
                match receiver.recv().await {
                    Ok(message) => {
                        if let HubMessage::Event { data } = &message {
                            if data.event_type == expected {
                                return message;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(e) => panic!("receiver error: {e}"),
                }
            }
        })
        .await
        .expect("hub should broadcast the published event")
    }

    #[tokio::test]
    async fn test_filter_blocks_non_matching_events() {
        let channels: Arc<RwLock<Option<HashSet<Channel>>>> =
            Arc::new(RwLock::new(Some(HashSet::from([Channel::System]))));

        let event = HubMessage::Event {
            data: EventData {
                subject: "ingestion.document.received".to_string(),
                event_type: "ingestion.document.received".to_string(),
                envelope: json!({}),
            },
        };
        assert!(!passes_filter(&channels, &event).await);

        let system = HubMessage::Event {
            data: EventData {
                subject: "system.heartbeat".to_string(),
                event_type: "system.heartbeat".to_string(),
                envelope: json!({}),
            },
        };
        assert!(passes_filter(&channels, &system).await);

        let welcome = HubMessage::Welcome {
            data: WelcomeData {
                server_started_at: chrono::Utc::now(),
                message: "hi".to_string(),
            },
        };
        assert!(passes_filter(&channels, &welcome).await);
    }

    #[tokio::test]
    async fn test_apply_control_subscribe_and_unsubscribe() {
        let channels: Arc<RwLock<Option<HashSet<Channel>>>> = Arc::new(RwLock::new(None));

        apply_control(
            &channels,
            ClientMessage::Subscribe {
                channels: vec![Channel::Events],
            },
        )
        .await;
        assert_eq!(channels.read().await.as_ref().map(|s| s.len()), Some(1));

        apply_control(
            &channels,
            ClientMessage::Unsubscribe {
                channels: vec![Channel::Events],
            },
        )
        .await;
        assert!(channels.read().await.is_none());
    }
}
