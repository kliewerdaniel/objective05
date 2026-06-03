use std::sync::Arc;

use objective_core::traits::MessageBus;
use objective_message_bus::InMemoryMessageBus;
use objective_scheduler::{SchedulerConfig, SchedulerService};

#[tokio::test]
async fn test_scheduler_triggers_jobs_and_emits_events() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let config = SchedulerConfig::default();
    let scheduler = SchedulerService::new(config, Arc::clone(&bus) as Arc<_>);

    // Manually trigger a job
    scheduler.trigger_job("rss_poll").await.unwrap();

    let events = bus.events().await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1.event_type, "ingestion.poll.rss");

    // Verify the payload contains job metadata
    let payload = &events[0].1.data;
    assert_eq!(payload["job_name"], "rss_poll");
    assert!(payload["triggered_at"].is_string());
    assert_eq!(payload["catch_up"], false);
}

#[tokio::test]
async fn test_scheduler_state_persistence() {
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let state_path = temp_dir.path().join("scheduler.jobstate");

    let bus = Arc::new(InMemoryMessageBus::new());
    let config = SchedulerConfig {
        state_path: Some(state_path.clone()),
        ..Default::default()
    };

    // Create scheduler, trigger a job, and explicitly save state
    {
        let scheduler = SchedulerService::new(config, Arc::clone(&bus) as Arc<_>);
        scheduler.trigger_job("rss_poll").await.unwrap();
        scheduler.save_state().await.unwrap();
    }

    // Verify state file was created
    assert!(state_path.exists());

    // Load state and verify it persisted
    let saved_state = objective_scheduler::SchedulerState::load_from_file(&state_path);
    let rss_job = saved_state.jobs.get("rss_poll").unwrap();
    assert_eq!(rss_job.execution_count, 1);
    assert!(rss_job.last_triggered_at.is_some());
}

#[tokio::test]
async fn test_scheduler_enable_disable() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let config = SchedulerConfig::default();
    let scheduler = SchedulerService::new(config, Arc::clone(&bus) as Arc<_>);

    // Disable the job
    scheduler.set_job_enabled("rss_poll", false).await.unwrap();
    let state = scheduler.get_state().await;
    assert!(!state.jobs["rss_poll"].enabled);

    // Re-enable the job
    scheduler.set_job_enabled("rss_poll", true).await.unwrap();
    let state = scheduler.get_state().await;
    assert!(state.jobs["rss_poll"].enabled);
}

#[tokio::test]
async fn test_scheduler_multiple_jobs() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let config = SchedulerConfig::default();
    let scheduler = SchedulerService::new(config, Arc::clone(&bus) as Arc<_>);

    // Trigger multiple jobs
    scheduler.trigger_job("rss_poll").await.unwrap();
    scheduler.trigger_job("extraction_batch").await.unwrap();
    scheduler.trigger_job("broadcast_generation").await.unwrap();

    let events = bus.events().await.unwrap();
    assert_eq!(events.len(), 3);

    let event_types: Vec<&str> = events.iter().map(|e| e.1.event_type.as_str()).collect();
    assert!(event_types.contains(&"ingestion.poll.rss"));
    assert!(event_types.contains(&"extraction.process.pending"));
    assert!(event_types.contains(&"broadcast.generate"));
}

#[tokio::test]
async fn test_scheduler_returns_error_for_unknown_job() {
    let bus = Arc::new(InMemoryMessageBus::new());
    let config = SchedulerConfig::default();
    let scheduler = SchedulerService::new(config, bus);

    let result = scheduler.trigger_job("nonexistent").await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("job not found"));
}
