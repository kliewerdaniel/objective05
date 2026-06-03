use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use chrono::Utc;
use objective_core::{ObjectiveError, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Snapshot of current pipeline metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineMetrics {
    pub uptime_secs: u64,
    pub documents_ingested: u64,
    pub extractions_completed: u64,
    pub events_created: u64,
    pub events_merged: u64,
    pub events_dropped: u64,
    pub pipeline_cycles: u64,
    pub retries_attempted: u64,
    pub dead_letters: u64,
    pub snapshots_created: u64,
    pub errors: u64,
    pub started_at: String,
    pub last_activity_at: String,
}

impl Default for PipelineMetrics {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
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
            errors: 0,
            started_at: now.clone(),
            last_activity_at: now,
        }
    }
}

/// Thread-safe pipeline monitoring service.
pub struct MonitoringService {
    metrics: RwLock<PipelineMetrics>,
    started_at: std::time::Instant,
    persist_path: PathBuf,
}

impl MonitoringService {
    pub fn new(data_root: &Path) -> Self {
        let persist_path = data_root.join("state").join("monitoring.json");
        let metrics = Self::load_from_file(&persist_path);
        let started_at = std::time::Instant::now();
        Self {
            metrics: RwLock::new(metrics),
            started_at,
            persist_path,
        }
    }

    fn load_from_file(path: &Path) -> PipelineMetrics {
        if !path.exists() {
            return PipelineMetrics::default();
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default()
    }

    fn persist(&self) {
        if let Ok(metrics) = self.metrics.read() {
            if let Ok(json) = serde_json::to_string_pretty(&*metrics) {
                let _ = std::fs::write(&self.persist_path, json);
            }
        }
    }

    pub fn record_document_ingested(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.documents_ingested += 1;
            m.last_activity_at = Utc::now().to_rfc3339();
        }
    }

    pub fn record_extraction_completed(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.extractions_completed += 1;
            m.last_activity_at = Utc::now().to_rfc3339();
        }
    }

    pub fn record_event_created(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.events_created += 1;
            m.last_activity_at = Utc::now().to_rfc3339();
        }
    }

    pub fn record_event_merged(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.events_merged += 1;
            m.last_activity_at = Utc::now().to_rfc3339();
        }
    }

    pub fn record_event_dropped(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.events_dropped += 1;
        }
    }

    pub fn record_pipeline_cycle(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.pipeline_cycles += 1;
            m.last_activity_at = Utc::now().to_rfc3339();
        }
    }

    pub fn record_retry(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.retries_attempted += 1;
        }
    }

    pub fn record_dead_letter(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.dead_letters += 1;
        }
    }

    pub fn record_snapshot(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.snapshots_created += 1;
            m.last_activity_at = Utc::now().to_rfc3339();
        }
    }

    pub fn record_error(&self) {
        if let Ok(mut m) = self.metrics.write() {
            m.errors += 1;
        }
    }

    pub fn get_metrics(&self) -> PipelineMetrics {
        let mut metrics = self.metrics.read().unwrap().clone();
        metrics.uptime_secs = self.started_at.elapsed().as_secs();
        metrics
    }

    pub fn persist_and_get(&self) -> PipelineMetrics {
        let metrics = self.get_metrics();
        self.persist();
        metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        tempfile::tempdir().unwrap().keep()
    }

    #[test]
    fn test_record_document_ingested() {
        let dir = temp_dir();
        let service = MonitoringService::new(&dir);
        service.record_document_ingested();
        service.record_document_ingested();
        let m = service.get_metrics();
        assert_eq!(m.documents_ingested, 2);
    }

    #[test]
    fn test_record_extraction_completed() {
        let dir = temp_dir();
        let service = MonitoringService::new(&dir);
        service.record_extraction_completed();
        let m = service.get_metrics();
        assert_eq!(m.extractions_completed, 1);
    }

    #[test]
    fn test_record_events() {
        let dir = temp_dir();
        let service = MonitoringService::new(&dir);
        service.record_event_created();
        service.record_event_created();
        service.record_event_merged();
        service.record_event_dropped();
        let m = service.get_metrics();
        assert_eq!(m.events_created, 2);
        assert_eq!(m.events_merged, 1);
        assert_eq!(m.events_dropped, 1);
    }

    #[test]
    fn test_record_pipeline_cycle() {
        let dir = temp_dir();
        let service = MonitoringService::new(&dir);
        service.record_pipeline_cycle();
        service.record_pipeline_cycle();
        let m = service.get_metrics();
        assert_eq!(m.pipeline_cycles, 2);
    }

    #[test]
    fn test_record_retry_and_dead_letter() {
        let dir = temp_dir();
        let service = MonitoringService::new(&dir);
        service.record_retry();
        service.record_retry();
        service.record_dead_letter();
        let m = service.get_metrics();
        assert_eq!(m.retries_attempted, 2);
        assert_eq!(m.dead_letters, 1);
    }

    #[test]
    fn test_persist_and_reload() {
        let dir = temp_dir();
        {
            let service = MonitoringService::new(&dir);
            service.record_document_ingested();
            service.record_document_ingested();
            service.record_document_ingested();
            service.persist_and_get();
        }

        let service2 = MonitoringService::new(&dir);
        let m = service2.get_metrics();
        assert_eq!(m.documents_ingested, 3);
    }

    #[test]
    fn test_uptime_increases() {
        let dir = temp_dir();
        let service = MonitoringService::new(&dir);
        let m1 = service.get_metrics();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let m2 = service.get_metrics();
        assert!(m2.uptime_secs >= m1.uptime_secs);
    }
}
