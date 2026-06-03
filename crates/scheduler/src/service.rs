use std::{path::PathBuf, sync::Arc};

use chrono::Utc;
use objective_core::{traits::MessageBus, types::EventEnvelope, Result};
use serde_json::json;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use crate::{
    job::{JobDefinition, JobState, JobTriggerPayload, SchedulerState},
};

/// Configuration for the scheduler service.
pub struct SchedulerConfig {
    /// Job definitions loaded from configuration.
    pub jobs: Vec<JobDefinition>,
    /// How often the scheduler checks for due jobs (in seconds).
    pub tick_interval_secs: u64,
    /// Path to persist scheduler state.
    pub state_path: Option<PathBuf>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            jobs: vec![
                JobDefinition {
                    name: "rss_poll".to_string(),
                    schedule: "0 * * * *".to_string(),
                    event: "ingestion.poll.rss".to_string(),
                    max_drift: 300,
                    enabled: true,
                },
                JobDefinition {
                    name: "extraction_batch".to_string(),
                    schedule: "*/5 * * * *".to_string(),
                    event: "extraction.process.pending".to_string(),
                    max_drift: 300,
                    enabled: true,
                },
                JobDefinition {
                    name: "broadcast_generation".to_string(),
                    schedule: "0 */2 * * *".to_string(),
                    event: "broadcast.generate".to_string(),
                    max_drift: 600,
                    enabled: true,
                },
                JobDefinition {
                    name: "maintenance".to_string(),
                    schedule: "0 3 * * *".to_string(),
                    event: "system.maintenance".to_string(),
                    max_drift: 3600,
                    enabled: true,
                },
                JobDefinition {
                    name: "snapshot".to_string(),
                    schedule: "0 4 * * *".to_string(),
                    event: "system.snapshot".to_string(),
                    max_drift: 3600,
                    enabled: true,
                },
            ],
            tick_interval_secs: 10,
            state_path: None,
        }
    }
}

/// The scheduler service that triggers periodic jobs via the message bus.
pub struct SchedulerService {
    config: SchedulerConfig,
    state: Arc<RwLock<SchedulerState>>,
    bus: Arc<dyn MessageBus>,
    running: Arc<RwLock<bool>>,
}

impl SchedulerService {
    /// Create a new scheduler service with the given configuration and message bus.
    pub fn new(config: SchedulerConfig, bus: Arc<dyn MessageBus>) -> Self {
        let now = Utc::now();

        // Load existing state or create new
        let mut state = match &config.state_path {
            Some(path) => SchedulerState::load_from_file(path),
            None => SchedulerState::new(),
        };

        // Initialize jobs that don't have state yet
        for job in &config.jobs {
            if let Ok(schedule) = job.parsed_schedule() {
                if !state.jobs.contains_key(&job.name) {
                    let next = schedule.next_trigger_after(now);
                    state.jobs.insert(
                        job.name.clone(),
                        JobState {
                            job_name: job.name.clone(),
                            enabled: job.enabled,
                            last_triggered_at: None,
                            next_scheduled_at: next,
                            execution_count: 0,
                            last_error: None,
                        },
                    );
                } else if let Some(job_state) = state.jobs.get_mut(&job.name) {
                    // Update enabled state from config
                    job_state.enabled = job.enabled;
                    // Recalculate next scheduled time if it's in the past
                    if job_state.next_scheduled_at <= now {
                        job_state.next_scheduled_at = schedule.next_trigger_after(now);
                    }
                }
            }
        }

        state.started_at = now;

        Self {
            config,
            state: Arc::new(RwLock::new(state)),
            bus,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the scheduler loop. This will run until `stop()` is called.
    pub async fn start(&self) -> Result<()> {
        {
            let mut running = self.running.write().await;
            if *running {
                warn!("scheduler is already running");
                return Ok(());
            }
            *running = true;
        }

        info!("scheduler starting with {} jobs", self.config.jobs.len());

        let event = EventEnvelope::new("system.service.started", "scheduler", json!({}));
        self.bus.publish("scheduler.system", event).await?;

        let tick_duration = Duration::from_secs(self.config.tick_interval_secs);
        let mut ticker = interval(tick_duration);

        loop {
            ticker.tick().await;

            if !*self.running.read().await {
                break;
            }

            self.tick().await;
        }

        info!("scheduler stopped");
        let event = EventEnvelope::new("system.service.stopped", "scheduler", json!({}));
        self.bus.publish("scheduler.system", event).await?;

        Ok(())
    }

    /// Stop the scheduler loop.
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("scheduler stop requested");
    }

    /// Check for due jobs and trigger them.
    async fn tick(&self) {
        let now = Utc::now();
        let mut state = self.state.write().await;

        for job_def in &self.config.jobs {
            let job_state = match state.jobs.get_mut(&job_def.name) {
                Some(s) => s,
                None => continue,
            };

            if !job_state.enabled {
                continue;
            }

            if now < job_state.next_scheduled_at {
                continue;
            }

            let schedule = match job_def.parsed_schedule() {
                Ok(s) => s,
                Err(e) => {
                    error!("job {} has invalid schedule: {}", job_def.name, e);
                    continue;
                }
            };

            let catch_up = job_state
                .last_triggered_at
                .map(|last| {
                    let drift = (now - last).num_seconds() as u64;
                    drift > job_def.max_drift
                })
                .unwrap_or(false);

            let scheduled_at = job_state.next_scheduled_at;
            job_state.next_scheduled_at = schedule.next_trigger_after(now);
            job_state.last_triggered_at = Some(now);
            job_state.execution_count += 1;

            let payload = JobTriggerPayload {
                job_name: job_def.name.clone(),
                triggered_at: now,
                scheduled_at,
                catch_up,
            };

            let event = EventEnvelope::new(
                &job_def.event,
                format!("scheduler.{}", job_def.name),
                serde_json::to_value(&payload).unwrap_or_default(),
            );

            if let Err(e) = self.bus.publish("scheduler.job.trigger", event).await {
                error!("failed to trigger job {}: {}", job_def.name, e);
                job_state.last_error = Some(e.to_string());
            } else {
                info!(
                    "triggered job {} (event: {}, execution #{})",
                    job_def.name, job_def.event, job_state.execution_count
                );
                job_state.last_error = None;
            }
        }

        // Persist state if configured
        if let Some(path) = &self.config.state_path {
            if let Err(e) = state.save_to_file(path) {
                error!("failed to persist scheduler state: {e}");
            }
        }
    }

    /// Get the current state of all jobs.
    pub async fn get_state(&self) -> SchedulerState {
        self.state.read().await.clone()
    }

    /// Enable or disable a specific job.
    pub async fn set_job_enabled(&self, job_name: &str, enabled: bool) -> Result<()> {
        let mut state = self.state.write().await;
        if let Some(job_state) = state.jobs.get_mut(job_name) {
            job_state.enabled = enabled;
            info!("job {} enabled={}", job_name, enabled);
        }
        Ok(())
    }

    /// Manually trigger a job by name, regardless of schedule.
    pub async fn trigger_job(&self, job_name: &str) -> Result<()> {
        let job_def = self
            .config
            .jobs
            .iter()
            .find(|j| j.name == job_name)
            .ok_or_else(|| objective_core::ObjectiveError::Validation(format!("job not found: {job_name}")))?
            .clone();

        let now = Utc::now();
        let payload = JobTriggerPayload {
            job_name: job_def.name.clone(),
            triggered_at: now,
            scheduled_at: now,
            catch_up: false,
        };

        // Update state
        {
            let mut state = self.state.write().await;
            if let Some(job_state) = state.jobs.get_mut(job_name) {
                job_state.last_triggered_at = Some(now);
                job_state.execution_count += 1;
                job_state.last_error = None;
            }
        }

        let event = EventEnvelope::new(
            &job_def.event,
            format!("scheduler.{}", job_def.name),
            serde_json::to_value(&payload).unwrap_or_default(),
        );

        self.bus
            .publish("scheduler.job.trigger", event)
            .await?;
        info!("manually triggered job {}", job_name);
        Ok(())
    }

    /// Save current state to disk.
    pub async fn save_state(&self) -> Result<()> {
        if let Some(path) = &self.config.state_path {
            let state = self.state.read().await;
            state
                .save_to_file(path)
                .map_err(|e| objective_core::ObjectiveError::Storage(e))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_message_bus::InMemoryMessageBus;

    #[tokio::test]
    async fn test_scheduler_creates_state_for_each_job() {
        let bus = Arc::new(InMemoryMessageBus::new());
        let config = SchedulerConfig::default();
        let scheduler = SchedulerService::new(config, bus);
        let state = scheduler.get_state().await;
        assert_eq!(state.jobs.len(), 5);
        assert!(state.jobs.contains_key("rss_poll"));
        assert!(state.jobs.contains_key("extraction_batch"));
    }

    #[tokio::test]
    async fn test_set_job_enabled() {
        let bus = Arc::new(InMemoryMessageBus::new());
        let config = SchedulerConfig::default();
        let scheduler = SchedulerService::new(config, bus);

        scheduler.set_job_enabled("rss_poll", false).await.unwrap();
        let state = scheduler.get_state().await;
        assert!(!state.jobs["rss_poll"].enabled);
    }

    #[tokio::test]
    async fn test_trigger_job_emits_event() {
        let bus = Arc::new(InMemoryMessageBus::new());
        let config = SchedulerConfig::default();
        let scheduler = SchedulerService::new(config, bus.clone());

        scheduler.trigger_job("rss_poll").await.unwrap();

        let events = bus.events().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].1.event_type,
            "ingestion.poll.rss"
        );
    }

    #[tokio::test]
    async fn test_trigger_unknown_job_returns_error() {
        let bus = Arc::new(InMemoryMessageBus::new());
        let config = SchedulerConfig::default();
        let scheduler = SchedulerService::new(config, bus);

        let result = scheduler.trigger_job("nonexistent").await;
        assert!(result.is_err());
    }
}
