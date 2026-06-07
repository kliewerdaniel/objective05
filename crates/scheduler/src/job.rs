use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;
use utoipa::ToSchema;

use crate::cron::CronSchedule;

/// Configuration for a single scheduled job.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobDefinition {
    /// Unique job name (e.g., "rss_poll").
    pub name: String,
    /// Cron expression defining when the job runs.
    pub schedule: String,
    /// Event type to emit when the job triggers (e.g., "ingestion.poll.rss").
    pub event: String,
    /// Maximum drift in seconds before skipping catch-up on restart.
    #[serde(default = "default_max_drift")]
    pub max_drift: u64,
    /// Whether this job is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_max_drift() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

impl JobDefinition {
    /// Parse the cron schedule string into a CronSchedule.
    pub fn parsed_schedule(&self) -> Result<CronSchedule, String> {
        CronSchedule::parse(&self.schedule)
    }
}

/// Runtime state for a single job, persisted across restarts.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobState {
    /// Name of the job this state tracks.
    pub job_name: String,
    /// Whether the job is enabled (can be toggled at runtime).
    pub enabled: bool,
    /// When this job was last triggered.
    pub last_triggered_at: Option<DateTime<Utc>>,
    /// When this job is next scheduled to run.
    pub next_scheduled_at: DateTime<Utc>,
    /// Total number of times this job has been triggered.
    pub execution_count: u64,
    /// Last error message if the job failed.
    pub last_error: Option<String>,
}

/// The full scheduler state, persisted as a single file.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SchedulerState {
    /// Per-job state keyed by job name.
    pub jobs: std::collections::BTreeMap<String, JobState>,
    /// When the scheduler was last started.
    pub started_at: DateTime<Utc>,
}

impl Default for SchedulerState {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            jobs: std::collections::BTreeMap::new(),
            started_at: Utc::now(),
        }
    }

    /// Load scheduler state from a JSON file. Returns a new state if the file doesn't exist.
    pub fn load_from_file(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                info!("failed to parse scheduler state, starting fresh: {e}");
                Self::new()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("no scheduler state file found, starting fresh");
                Self::new()
            }
            Err(e) => {
                info!("failed to read scheduler state file, starting fresh: {e}");
                Self::new()
            }
        }
    }

    /// Save scheduler state to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> std::result::Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create state directory: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize state: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("failed to write state file: {e}"))?;
        Ok(())
    }
}

/// Event payload emitted when a job triggers.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobTriggerPayload {
    /// Name of the job that triggered.
    pub job_name: String,
    /// When the job was triggered.
    pub triggered_at: DateTime<Utc>,
    /// When the job was originally scheduled.
    pub scheduled_at: DateTime<Utc>,
    /// Whether this was a catch-up run.
    pub catch_up: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_definition_parse_schedule() {
        let job = JobDefinition {
            name: "test".to_string(),
            schedule: "0 * * * *".to_string(),
            event: "test.event".to_string(),
            max_drift: 300,
            enabled: true,
        };
        let schedule = job.parsed_schedule().unwrap();
        assert_eq!(schedule.minute, crate::cron::CronField::Fixed(0));
        assert_eq!(schedule.hour, crate::cron::CronField::Any);
    }

    #[test]
    fn test_scheduler_state_defaults() {
        let state = SchedulerState::new();
        assert!(state.jobs.is_empty());
    }

    #[test]
    fn test_job_trigger_payload_serialization() {
        let payload = JobTriggerPayload {
            job_name: "rss_poll".to_string(),
            triggered_at: Utc::now(),
            scheduled_at: Utc::now(),
            catch_up: false,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("rss_poll"));
    }
}
