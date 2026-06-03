//! Scheduler service crate for the Objective platform.
//!
//! The scheduler manages periodic job definitions and emits trigger events
//! on the message bus. It supports cron-style scheduling with catch-up
//! semantics for missed executions.

pub mod cron;
pub mod job;
pub mod service;

pub use cron::{CronField, CronSchedule};
pub use job::{JobDefinition, JobState, JobTriggerPayload, SchedulerState};
pub use service::{SchedulerConfig, SchedulerService};
