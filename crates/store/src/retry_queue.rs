use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use objective_core::{ObjectiveError, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

/// A job that failed and is waiting to be retried.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryJob {
    pub id: String,
    pub event_type: String,
    pub source_subject: String,
    pub created_at: String,
    pub next_retry_at: String,
    pub retry_count: u32,
    pub max_retries: u32,
    pub last_error: String,
}

/// Durable retry queue with dead-letter support.
///
/// Failed jobs are persisted as individual JSON files in a queue directory.
/// After `max_retries` attempts, jobs are moved to a dead-letter directory.
pub struct RetryQueue {
    queue_dir: PathBuf,
    dead_letter_dir: PathBuf,
    max_retries: u32,
    base_backoff_secs: i64,
}

impl RetryQueue {
    pub fn new(data_root: &Path, max_retries: u32, base_backoff_secs: i64) -> Self {
        let queue_dir = data_root.join("queue").join("retry");
        let dead_letter_dir = data_root.join("queue").join("dead-letter");
        std::fs::create_dir_all(&queue_dir).ok();
        std::fs::create_dir_all(&dead_letter_dir).ok();
        Self {
            queue_dir,
            dead_letter_dir,
            max_retries,
            base_backoff_secs,
        }
    }

    /// Enqueue a failed job for retry.
    pub fn enqueue(
        &self,
        event_type: &str,
        source_subject: &str,
        error: &str,
        attempt: u32,
    ) -> Result<RetryJob> {
        let id = format!("{}-{}", chrono::Utc::now().timestamp_millis(), attempt);
        let backoff = self.base_backoff_secs * 2_i64.pow(attempt.min(6));
        let next_retry = Utc::now() + Duration::seconds(backoff);

        let job = RetryJob {
            id: id.clone(),
            event_type: event_type.to_string(),
            source_subject: source_subject.to_string(),
            created_at: Utc::now().to_rfc3339(),
            next_retry_at: next_retry.to_rfc3339(),
            retry_count: attempt,
            max_retries: self.max_retries,
            last_error: error.to_string(),
        };

        let path = self.queue_dir.join(format!("{id}.json"));
        let json = serde_json::to_string_pretty(&job)
            .map_err(|e| ObjectiveError::Storage(format!("failed to serialize retry job: {e}")))?;
        std::fs::write(&path, json).map_err(|e| {
            ObjectiveError::Storage(format!("failed to write retry job: {e}"))
        })?;

        Ok(job)
    }

    /// Check if a job should be retried based on its retry count and backoff.
    pub fn should_retry(&self, job: &RetryJob) -> bool {
        if job.retry_count >= self.max_retries {
            return false;
        }
        if let Ok(next_retry) = DateTime::parse_from_rfc3339(&job.next_retry_at) {
            Utc::now() >= next_retry.with_timezone(&Utc)
        } else {
            false
        }
    }

    /// Move a job to dead-letter (exceeded max retries).
    pub fn move_to_dead_letter(&self, job: &RetryJob) -> Result<()> {
        let src = self.queue_dir.join(format!("{}.json", job.id));
        let dest = self.dead_letter_dir.join(format!("{}.json", job.id));
        if src.exists() {
            std::fs::rename(&src, &dest).map_err(|e| {
                ObjectiveError::Storage(format!("failed to move job to dead letter: {e}"))
            })?;
            info!(job_id = %job.id, event_type = %job.event_type, "job moved to dead letter");
        }
        Ok(())
    }

    /// Remove a successfully completed job from the queue.
    pub fn complete(&self, job_id: &str) -> Result<()> {
        let path = self.queue_dir.join(format!("{job_id}.json"));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                ObjectiveError::Storage(format!("failed to remove completed job: {e}"))
            })?;
        }
        Ok(())
    }

    /// Get all jobs ready for retry.
    pub fn ready_jobs(&self) -> Result<Vec<RetryJob>> {
        if !self.queue_dir.exists() {
            return Ok(Vec::new());
        }

        let mut jobs = Vec::new();
        for entry in std::fs::read_dir(&self.queue_dir).map_err(storage_error)? {
            let entry = entry.map_err(storage_error)?;
            if !entry.file_type().map_err(storage_error)?.is_file() {
                continue;
            }
            if !entry.file_name().to_string_lossy().ends_with(".json") {
                continue;
            }

            let data = std::fs::read_to_string(entry.path()).map_err(storage_error)?;
            if let Ok(job) = serde_json::from_str::<RetryJob>(&data) {
                if self.should_retry(&job) {
                    jobs.push(job);
                }
            }
        }

        jobs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(jobs)
    }

    /// List all jobs in the dead-letter directory.
    pub fn dead_letters(&self) -> Result<Vec<RetryJob>> {
        if !self.dead_letter_dir.exists() {
            return Ok(Vec::new());
        }

        let mut jobs = Vec::new();
        for entry in std::fs::read_dir(&self.dead_letter_dir).map_err(storage_error)? {
            let entry = entry.map_err(storage_error)?;
            if !entry.file_type().map_err(storage_error)?.is_file() {
                continue;
            }
            let data = std::fs::read_to_string(entry.path()).map_err(storage_error)?;
            if let Ok(job) = serde_json::from_str::<RetryJob>(&data) {
                jobs.push(job);
            }
        }

        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(jobs)
    }

    /// Re-enqueue a job with incremented retry count.
    pub fn re_enqueue(&self, job: &RetryJob) -> Result<RetryJob> {
        if job.retry_count + 1 >= self.max_retries {
            self.move_to_dead_letter(job)?;
            return Ok(RetryJob {
                id: job.id.clone(),
                event_type: job.event_type.clone(),
                source_subject: job.source_subject.clone(),
                created_at: job.created_at.clone(),
                next_retry_at: job.next_retry_at.clone(),
                retry_count: job.retry_count + 1,
                max_retries: job.max_retries,
                last_error: job.last_error.clone(),
            });
        }

        self.complete(&job.id)?;
        self.enqueue(
            &job.event_type,
            &job.source_subject,
            &job.last_error,
            job.retry_count + 1,
        )
    }
}

fn storage_error(e: std::io::Error) -> ObjectiveError {
    ObjectiveError::Storage(format!("storage error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        tempfile::tempdir().unwrap().keep()
    }

    #[test]
    fn test_enqueue_creates_file() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 60);

        let job = queue
            .enqueue("ingestion.poll.rss", "rss_source", "timeout", 0)
            .unwrap();

        assert!(!job.id.is_empty());
        assert_eq!(job.retry_count, 0);
        assert_eq!(job.max_retries, 3);

        let path = queue.queue_dir.join(format!("{}.json", job.id));
        assert!(path.exists());
    }

    #[test]
    fn test_should_retry_within_limit() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 0);

        let job = queue
            .enqueue("test.event", "source", "error", 0)
            .unwrap();
        assert!(queue.should_retry(&job));

        let job2 = queue
            .enqueue("test.event", "source", "error", 2)
            .unwrap();
        assert!(queue.should_retry(&job2));
    }

    #[test]
    fn test_should_not_retry_exceeded_limit() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 60);

        let job = queue
            .enqueue("test.event", "source", "error", 3)
            .unwrap();
        assert!(!queue.should_retry(&job));
    }

    #[test]
    fn test_move_to_dead_letter() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 60);

        let job = queue
            .enqueue("test.event", "source", "error", 0)
            .unwrap();
        queue.move_to_dead_letter(&job).unwrap();

        assert!(!queue.queue_dir.join(format!("{}.json", job.id)).exists());
        assert!(queue
            .dead_letter_dir
            .join(format!("{}.json", job.id))
            .exists());
    }

    #[test]
    fn test_complete_removes_job() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 60);

        let job = queue
            .enqueue("test.event", "source", "error", 0)
            .unwrap();
        queue.complete(&job.id).unwrap();

        assert!(!queue.queue_dir.join(format!("{}.json", job.id)).exists());
    }

    #[test]
    fn test_ready_jobs_returns_only_ready() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 0);

        let job = queue
            .enqueue("test.event", "source", "error", 0)
            .unwrap();

        let ready = queue.ready_jobs().unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, job.id);
    }

    #[test]
    fn test_re_enqueue_increments_count() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 60);

        let job = queue
            .enqueue("test.event", "source", "error", 0)
            .unwrap();
        let job2 = queue.re_enqueue(&job).unwrap();

        assert_eq!(job2.retry_count, 1);
        assert!(!queue.queue_dir.join(format!("{}.json", job.id)).exists());
        assert!(queue.queue_dir.join(format!("{}.json", job2.id)).exists());
    }

    #[test]
    fn test_re_enqueue_at_limit_moves_to_dead_letter() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 60);

        let job = queue
            .enqueue("test.event", "source", "error", 2)
            .unwrap();
        let job2 = queue.re_enqueue(&job).unwrap();

        assert_eq!(job2.retry_count, 3);
        assert!(!queue.queue_dir.join(format!("{}.json", job2.id)).exists());
        assert!(queue
            .dead_letter_dir
            .join(format!("{}.json", job2.id))
            .exists());
    }

    #[test]
    fn test_dead_letters_lists_dead_letter_jobs() {
        let dir = temp_dir();
        let queue = RetryQueue::new(&dir, 3, 60);

        let job = queue
            .enqueue("test.event", "source", "error", 0)
            .unwrap();
        queue.move_to_dead_letter(&job).unwrap();

        let dead = queue.dead_letters().unwrap();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].id, job.id);
    }
}
