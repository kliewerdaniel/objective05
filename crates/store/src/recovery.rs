//! Recovery service for pipeline health monitoring and automatic recovery.
//!
//! The recovery service observes pipeline activity through the
//! [`MonitoringService`], detects stalled or erroring pipelines, and
//! publishes the system events documented in `docs/api/internal-api.md`:
//!
//! - `system.heartbeat` — periodic health signal
//! - `system.service.crash` — emitted when a stalled and erroring
//!   pipeline is detected
//! - `system.service.recovered` — emitted when a previously crashed
//!   pipeline returns to a healthy state
//!
//! The service is intentionally conservative: it only flags a pipeline
//! as crashed when both pipeline activity has stalled and the error
//! count has climbed in the same observation window, so transient gaps
//! in activity (e.g. between scheduler ticks) do not produce false
//! positives.

use std::{path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use objective_core::{traits::MessageBus, types::EventEnvelope, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use utoipa::ToSchema;

use crate::monitoring::{MonitoringService, PipelineMetrics};

/// Coarse status of the watched pipeline.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    /// Pipeline is making forward progress and errors are within bounds.
    Healthy,
    /// Pipeline produced no activity for longer than the stall threshold.
    Stalled,
    /// Pipeline is producing activity but the error rate is elevated.
    Degraded,
    /// A crash was detected and the recovery procedure is in progress.
    Recovering,
    /// A previously crashed pipeline returned to a healthy state.
    Recovered,
}

impl ServiceStatus {
    fn as_str(self) -> &'static str {
        match self {
            ServiceStatus::Healthy => "healthy",
            ServiceStatus::Stalled => "stalled",
            ServiceStatus::Degraded => "degraded",
            ServiceStatus::Recovering => "recovering",
            ServiceStatus::Recovered => "recovered",
        }
    }
}

/// A single transition or event captured by the recovery service.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecoveryEvent {
    /// Type of recovery event (`crash_detected`, `recovered`,
    /// `stalled`, `degraded`, `heartbeat`).
    pub event_type: String,
    /// When the event was recorded.
    pub timestamp: DateTime<Utc>,
    /// Short human-readable reason.
    pub reason: String,
    /// Status captured at this event.
    pub status: ServiceStatus,
    /// Status captured immediately before this event, when applicable.
    pub previous_status: Option<ServiceStatus>,
}

/// Aggregate state of the recovery service, exposed through the API.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecoveryState {
    /// Name of the watched service (e.g. "pipeline").
    pub service_name: String,
    /// Current observed status.
    pub current_status: ServiceStatus,
    /// When the last health check ran.
    pub last_check_at: DateTime<Utc>,
    /// When the last `system.heartbeat` was published.
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    /// When the most recent crash was detected, if any.
    pub last_crash_at: Option<DateTime<Utc>>,
    /// When the most recent recovery completed, if any.
    pub last_recovery_at: Option<DateTime<Utc>>,
    /// Number of health checks performed since startup.
    pub checks_performed: u64,
    /// Number of crashes detected since startup.
    pub crashes_detected: u64,
    /// Number of recoveries performed since startup.
    pub recoveries_performed: u64,
    /// Recent recovery events, newest first.
    pub history: Vec<RecoveryEvent>,
}

impl Default for RecoveryState {
    fn default() -> Self {
        Self {
            service_name: "pipeline".to_string(),
            current_status: ServiceStatus::Healthy,
            last_check_at: Utc::now(),
            last_heartbeat_at: None,
            last_crash_at: None,
            last_recovery_at: None,
            checks_performed: 0,
            crashes_detected: 0,
            recoveries_performed: 0,
            history: Vec::new(),
        }
    }
}

/// Configuration for the recovery service.
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Name of the watched service (used in events and as the
    /// `source` field of emitted envelopes).
    pub service_name: String,
    /// How often the recovery loop runs, in seconds.
    pub check_interval_secs: u64,
    /// Maximum time without pipeline activity before the pipeline is
    /// considered stalled, in seconds.
    pub stall_threshold_secs: i64,
    /// Maximum allowed number of errors within `error_window_secs`
    /// before the pipeline is treated as degraded.
    pub max_errors_in_window: u64,
    /// Rolling window used for error counting, in seconds.
    pub error_window_secs: i64,
    /// How often the `system.heartbeat` event is published, in
    /// seconds. Set to `0` to disable heartbeats.
    pub heartbeat_interval_secs: u64,
    /// Optional path used to persist recovery state across restarts.
    pub state_path: Option<PathBuf>,
    /// Maximum number of history entries to retain in memory and on
    /// disk.
    pub max_history: usize,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            service_name: "pipeline".to_string(),
            check_interval_secs: 15,
            stall_threshold_secs: 300,
            max_errors_in_window: 10,
            error_window_secs: 60,
            heartbeat_interval_secs: 60,
            state_path: None,
            max_history: 64,
        }
    }
}

/// Result of a single health check, useful for tests and diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecoveryCheck {
    pub status: ServiceStatus,
    pub reason: String,
    pub stalled: bool,
    pub errors_in_window: u64,
    pub seconds_since_activity: i64,
    pub published_crash: bool,
    pub published_recovered: bool,
    pub published_heartbeat: bool,
    pub timestamp: DateTime<Utc>,
}

/// Action emitted by the state transition step. The state is updated
/// first; publishing happens after the write lock has been released to
/// avoid re-entering tokio's RwLock.
enum TransitionAction {
    None,
    Crash,
    Recovered(ServiceStatus),
}

/// The recovery service.
pub struct RecoveryService {
    config: RecoveryConfig,
    monitoring: Arc<MonitoringService>,
    bus: Arc<dyn MessageBus>,
    state: Arc<RwLock<RecoveryState>>,
    running: Arc<RwLock<bool>>,
}

impl RecoveryService {
    /// Construct a new recovery service.
    pub fn new(
        config: RecoveryConfig,
        monitoring: Arc<MonitoringService>,
        bus: Arc<dyn MessageBus>,
    ) -> Self {
        let state = match &config.state_path {
            Some(path) => Self::load_state(path, &config),
            None => RecoveryState {
                service_name: config.service_name.clone(),
                ..RecoveryState::default()
            },
        };

        Self {
            config,
            monitoring,
            bus,
            state: Arc::new(RwLock::new(state)),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Return the recovery configuration in use.
    pub fn config(&self) -> &RecoveryConfig {
        &self.config
    }

    /// Return the message bus used for recovery events.
    pub fn bus(&self) -> &Arc<dyn MessageBus> {
        &self.bus
    }

    /// Return the monitoring service this recovery service is observing.
    /// Provided so callers and tests can drive metrics changes through
    /// the same handle the recovery service is reading.
    pub fn monitoring(&self) -> &Arc<MonitoringService> {
        &self.monitoring
    }

    fn load_state(path: &PathBuf, config: &RecoveryConfig) -> RecoveryState {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str::<RecoveryState>(&content).unwrap_or_else(|e| {
                warn!("failed to parse recovery state, starting fresh: {e}");
                RecoveryState {
                    service_name: config.service_name.clone(),
                    ..RecoveryState::default()
                }
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => RecoveryState {
                service_name: config.service_name.clone(),
                ..RecoveryState::default()
            },
            Err(e) => {
                warn!("failed to read recovery state file, starting fresh: {e}");
                RecoveryState {
                    service_name: config.service_name.clone(),
                    ..RecoveryState::default()
                }
            }
        }
    }

    fn persist_state(&self, state: &RecoveryState) {
        if let Some(path) = &self.config.state_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(state) {
                if let Err(e) = std::fs::write(path, json) {
                    warn!("failed to persist recovery state: {e}");
                }
            }
        }
    }

    /// Start the recovery loop. The loop runs until
    /// [`RecoveryService::stop`] is called.
    pub async fn start(&self) -> Result<()> {
        {
            let mut running = self.running.write().await;
            if *running {
                warn!("recovery service is already running");
                return Ok(());
            }
            *running = true;
        }

        info!(
            service = %self.config.service_name,
            check_interval_secs = self.config.check_interval_secs,
            "recovery service starting"
        );

        let tick = Duration::from_secs(self.config.check_interval_secs);
        let mut ticker = interval(tick);

        loop {
            ticker.tick().await;

            if !*self.running.read().await {
                break;
            }

            if let Err(e) = self.force_check().await {
                error!(error = %e, "recovery check failed");
            }
        }

        info!(service = %self.config.service_name, "recovery service stopped");
        Ok(())
    }

    /// Request the recovery loop to stop at the next tick.
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("recovery service stop requested");
    }

    /// Snapshot of the recovery state.
    pub async fn get_state(&self) -> RecoveryState {
        self.state.read().await.clone()
    }

    /// Run a single health check, publishing any transition events.
    pub async fn force_check(&self) -> Result<RecoveryCheck> {
        let metrics = self.monitoring.get_metrics();
        let mut check = self.evaluate(&metrics).await;
        let (action, heartbeat) = self.apply_check(&check).await?;
        check.published_heartbeat = heartbeat;
        match action {
            TransitionAction::Crash => check.published_crash = true,
            TransitionAction::Recovered(_) => check.published_recovered = true,
            TransitionAction::None => {}
        }
        Ok(check)
    }

    async fn evaluate(&self, metrics: &PipelineMetrics) -> RecoveryCheck {
        let now = Utc::now();
        let seconds_since_activity = seconds_since(&metrics.last_activity_at, now);
        let errors_in_window = if seconds_since_activity <= self.config.error_window_secs {
            metrics.errors
        } else {
            0
        };

        let stalled = seconds_since_activity > self.config.stall_threshold_secs;
        let erroring = errors_in_window > self.config.max_errors_in_window;

        let (status, reason) = if stalled && erroring {
            (
                ServiceStatus::Recovering,
                format!(
                    "pipeline stalled for {seconds_since_activity}s and recorded {errors_in_window} errors in the last {win}s",
                    win = self.config.error_window_secs
                ),
            )
        } else if stalled {
            (
                ServiceStatus::Stalled,
                format!("no pipeline activity for {seconds_since_activity}s"),
            )
        } else if erroring {
            (
                ServiceStatus::Degraded,
                format!(
                    "{errors_in_window} errors in the last {win}s",
                    win = self.config.error_window_secs
                ),
            )
        } else {
            (
                ServiceStatus::Healthy,
                "pipeline is active and within error bounds".to_string(),
            )
        };

        RecoveryCheck {
            status,
            reason,
            stalled,
            errors_in_window,
            seconds_since_activity,
            published_crash: false,
            published_recovered: false,
            published_heartbeat: false,
            timestamp: now,
        }
    }

    async fn apply_check(&self, check: &RecoveryCheck) -> Result<(TransitionAction, bool)> {
        let now = Utc::now();

        // Build the transition under the write lock, then release the
        // lock before publishing events. tokio's RwLock does not allow
        // a read lock to be acquired from the same task that holds
        // the write lock, so we must drop the guard first.
        let transition: (TransitionAction, bool) = {
            let mut state = self.state.write().await;
            let previous = state.current_status;
            let should_heartbeat = self.config.heartbeat_interval_secs > 0
                && state
                    .last_heartbeat_at
                    .map(|last| {
                        (now - last).num_seconds() as u64 >= self.config.heartbeat_interval_secs
                    })
                    .unwrap_or(true);

            state.last_check_at = check.timestamp;
            state.checks_performed += 1;

            let mut action = TransitionAction::None;
            match (previous, check.status) {
                (
                    ServiceStatus::Healthy | ServiceStatus::Stalled | ServiceStatus::Degraded,
                    ServiceStatus::Recovering,
                ) => {
                    state.crashes_detected += 1;
                    state.last_crash_at = Some(check.timestamp);
                    state.current_status = ServiceStatus::Recovering;
                    self.record_event(
                        &mut state,
                        "crash_detected",
                        check.reason.clone(),
                        ServiceStatus::Recovering,
                        Some(previous),
                    );
                    action = TransitionAction::Crash;
                }
                (
                    ServiceStatus::Recovering | ServiceStatus::Stalled | ServiceStatus::Degraded,
                    ServiceStatus::Healthy,
                ) => {
                    state.recoveries_performed += 1;
                    state.last_recovery_at = Some(check.timestamp);
                    state.current_status = ServiceStatus::Healthy;
                    self.record_event(
                        &mut state,
                        "recovered",
                        "pipeline returned to healthy state".to_string(),
                        ServiceStatus::Healthy,
                        Some(previous),
                    );
                    action = TransitionAction::Recovered(previous);
                }
                (_, status) => {
                    if previous != status {
                        self.record_event(
                            &mut state,
                            status.as_str(),
                            check.reason.clone(),
                            status,
                            Some(previous),
                        );
                    }
                    state.current_status = status;
                }
            }

            if should_heartbeat {
                state.last_heartbeat_at = Some(check.timestamp);
            }

            self.persist_state(&state);
            (action, should_heartbeat)
        };

        // Now the write lock has been dropped; safe to publish.
        match transition.0 {
            TransitionAction::Crash => self.publish_crash(check).await?,
            TransitionAction::Recovered(prev) => self.publish_recovered(check, prev).await?,
            TransitionAction::None => {}
        }
        if transition.1 {
            self.publish_heartbeat(check).await?;
        }
        Ok(transition)
    }

    fn record_event(
        &self,
        state: &mut RecoveryState,
        event_type: &str,
        reason: String,
        status: ServiceStatus,
        previous: Option<ServiceStatus>,
    ) {
        state.history.insert(
            0,
            RecoveryEvent {
                event_type: event_type.to_string(),
                timestamp: Utc::now(),
                reason,
                status,
                previous_status: previous,
            },
        );
        if state.history.len() > self.config.max_history {
            state.history.truncate(self.config.max_history);
        }
    }

    async fn publish_crash(&self, check: &RecoveryCheck) -> Result<()> {
        let payload = json!({
            "service_name": self.config.service_name,
            "reason": check.reason,
            "status": check.status.as_str(),
            "stalled": check.stalled,
            "errors_in_window": check.errors_in_window,
            "seconds_since_activity": check.seconds_since_activity,
        });
        let event = EventEnvelope::new(
            "system.service.crash",
            format!("recovery.{}", self.config.service_name),
            payload,
        );
        if let Err(e) = self.bus.publish("recovery.system", event).await {
            warn!(error = %e, "failed to publish system.service.crash");
        }
        Ok(())
    }

    async fn publish_recovered(
        &self,
        check: &RecoveryCheck,
        previous: ServiceStatus,
    ) -> Result<()> {
        let payload = json!({
            "service_name": self.config.service_name,
            "previous_status": previous.as_str(),
            "status": check.status.as_str(),
            "seconds_since_activity": check.seconds_since_activity,
        });
        let event = EventEnvelope::new(
            "system.service.recovered",
            format!("recovery.{}", self.config.service_name),
            payload,
        );
        if let Err(e) = self.bus.publish("recovery.system", event).await {
            warn!(error = %e, "failed to publish system.service.recovered");
        }
        Ok(())
    }

    async fn publish_heartbeat(&self, check: &RecoveryCheck) -> Result<()> {
        let checks = self.state.read().await.checks_performed;
        let crashes = self.state.read().await.crashes_detected;
        let payload = json!({
            "service_name": self.config.service_name,
            "status": check.status.as_str(),
            "checks_performed": checks,
            "crashes_detected": crashes,
            "seconds_since_activity": check.seconds_since_activity,
        });
        let event = EventEnvelope::new(
            "system.heartbeat",
            format!("recovery.{}", self.config.service_name),
            payload,
        );
        if let Err(e) = self.bus.publish("recovery.system", event).await {
            warn!(error = %e, "failed to publish system.heartbeat");
        }
        Ok(())
    }
}

fn seconds_since(last_activity_at: &str, now: DateTime<Utc>) -> i64 {
    match DateTime::parse_from_rfc3339(last_activity_at) {
        Ok(parsed) => (now - parsed.with_timezone(&Utc)).num_seconds(),
        Err(_) => i64::MAX / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_message_bus::InMemoryMessageBus;

    fn stale_metrics(errors: u64, seconds_ago: i64) -> PipelineMetrics {
        let old = (Utc::now() - chrono::Duration::seconds(seconds_ago)).to_rfc3339();
        PipelineMetrics {
            uptime_secs: 0,
            documents_ingested: 0,
            extractions_completed: 0,
            events_created: 0,
            events_merged: 0,
            events_dropped: 0,
            pipeline_cycles: 0,
            retries_attempted: 0,
            dead_letters: 0,
            snapshots_created: 0,
            errors,
            started_at: old.clone(),
            last_activity_at: old,
        }
    }

    fn make_monitoring() -> Arc<MonitoringService> {
        let dir = tempfile::tempdir().unwrap().keep();
        Arc::new(MonitoringService::new(&dir))
    }

    fn make_bus() -> Arc<InMemoryMessageBus> {
        Arc::new(InMemoryMessageBus::new())
    }

    #[tokio::test]
    async fn test_initial_state_is_healthy() {
        let monitoring = make_monitoring();
        let bus = make_bus();
        let service = RecoveryService::new(RecoveryConfig::default(), monitoring, bus);
        let state = service.get_state().await;
        assert_eq!(state.current_status, ServiceStatus::Healthy);
        assert_eq!(state.checks_performed, 0);
        assert!(state.history.is_empty());
    }

    #[tokio::test]
    async fn test_first_check_publishes_heartbeat() {
        let monitoring = make_monitoring();
        let bus = make_bus();
        let service = RecoveryService::new(RecoveryConfig::default(), monitoring, bus.clone());
        let check = service.force_check().await.unwrap();
        assert_eq!(check.status, ServiceStatus::Healthy);
        assert!(check.published_heartbeat);
        assert!(!check.published_crash);

        let events = bus.events().await.unwrap();
        assert!(events
            .iter()
            .any(|(_, e)| e.event_type == "system.heartbeat"));

        let state = service.get_state().await;
        assert_eq!(state.checks_performed, 1);
        assert!(state.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn test_subsequent_checks_within_heartbeat_window_do_not_republish() {
        let monitoring = make_monitoring();
        let bus = make_bus();
        let config = RecoveryConfig {
            heartbeat_interval_secs: 3600,
            ..RecoveryConfig::default()
        };
        let service = RecoveryService::new(config, monitoring, bus.clone());

        service.force_check().await.unwrap();
        let check = service.force_check().await.unwrap();
        assert!(!check.published_heartbeat);

        let events = bus.events().await.unwrap();
        let heartbeats = events
            .iter()
            .filter(|(_, e)| e.event_type == "system.heartbeat")
            .count();
        assert_eq!(heartbeats, 1);
    }

    #[tokio::test]
    async fn test_stalled_pipeline_publishes_crash_event() {
        let monitoring = make_monitoring();
        monitoring.replace_metrics(stale_metrics(25, 3600));
        let bus = make_bus();
        let config = RecoveryConfig {
            heartbeat_interval_secs: 0,
            stall_threshold_secs: 60,
            error_window_secs: 7200,
            ..RecoveryConfig::default()
        };
        let service = RecoveryService::new(config, monitoring, bus.clone());

        let check = service.force_check().await.unwrap();
        assert_eq!(check.status, ServiceStatus::Recovering);
        assert!(check.published_crash);

        let state = service.get_state().await;
        assert_eq!(state.crashes_detected, 1);
        assert!(state.last_crash_at.is_some());

        let events = bus.events().await.unwrap();
        let crash = events
            .iter()
            .find(|(_, e)| e.event_type == "system.service.crash")
            .expect("crash event must be published");
        assert_eq!(crash.1.data["status"], "recovering");
    }
    #[tokio::test]
    async fn test_recovery_from_recovering_emits_recovered_event() {
        let monitoring = make_monitoring();
        let bus = make_bus();
        let config = RecoveryConfig {
            heartbeat_interval_secs: 0,
            stall_threshold_secs: 60,
            error_window_secs: 7200,
            ..RecoveryConfig::default()
        };
        let service = RecoveryService::new(config, monitoring.clone(), bus.clone());

        monitoring.replace_metrics(stale_metrics(25, 3600));
        service.force_check().await.unwrap();

        // Recovery: fresh activity and zero errors
        monitoring.replace_metrics(stale_metrics(0, 0));
        let check = service.force_check().await.unwrap();
        assert_eq!(check.status, ServiceStatus::Healthy);
        assert!(check.published_recovered);

        let state = service.get_state().await;
        assert_eq!(state.crashes_detected, 1);
        assert_eq!(state.recoveries_performed, 1);
        assert!(state.last_recovery_at.is_some());
        assert!(state.history.iter().any(|e| e.event_type == "recovered"));

        let events = bus.events().await.unwrap();
        assert!(events
            .iter()
            .any(|(_, e)| e.event_type == "system.service.recovered"));
    }
    #[tokio::test]
    async fn test_persistent_state_survives_reload() {
        let dir = tempfile::tempdir().unwrap().keep();
        let monitoring_dir = dir.join("monitoring");
        std::fs::create_dir_all(&monitoring_dir).unwrap();
        let monitoring = Arc::new(MonitoringService::new(&monitoring_dir));
        let bus = make_bus();
        let config = RecoveryConfig {
            heartbeat_interval_secs: 0,
            state_path: Some(dir.join("state").join("recovery.json")),
            ..RecoveryConfig::default()
        };
        let service = RecoveryService::new(config.clone(), monitoring.clone(), bus.clone());
        service.force_check().await.unwrap();
        service.force_check().await.unwrap();
        let first = service.get_state().await;
        assert_eq!(first.checks_performed, 2);

        let service2 = RecoveryService::new(config, monitoring, bus);
        let second = service2.get_state().await;
        assert_eq!(second.checks_performed, 2);
    }

    #[tokio::test]
    async fn test_degraded_status_when_only_errors_high() {
        let monitoring = make_monitoring();
        let bus = make_bus();
        let config = RecoveryConfig {
            heartbeat_interval_secs: 0,
            stall_threshold_secs: i64::MAX,
            ..RecoveryConfig::default()
        };
        let service = RecoveryService::new(config, monitoring, bus.clone());

        for _ in 0..25 {
            service.monitoring().record_error();
        }

        let check = service.force_check().await.unwrap();
        assert_eq!(check.status, ServiceStatus::Degraded);
        assert!(!check.published_crash);
    }

    #[tokio::test]
    async fn test_history_is_capped_at_max_history() {
        let monitoring = make_monitoring();
        let bus = make_bus();
        let config = RecoveryConfig {
            heartbeat_interval_secs: 0,
            max_history: 3,
            ..RecoveryConfig::default()
        };
        let service = RecoveryService::new(config, monitoring.clone(), bus);

        for _ in 0..3 {
            monitoring.replace_metrics(stale_metrics(25, 3600));
            service.force_check().await.unwrap();
            monitoring.record_document_ingested();
            service.force_check().await.unwrap();
        }

        let state = service.get_state().await;
        assert!(state.history.len() <= 3);
    }
}
