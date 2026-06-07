use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Snapshot of the model-runtime metrics table, mirrored
/// from the live `LocalModelRuntime` so the monitoring
/// service can persist + surface a historical view of the
/// latency histograms. Phase 4 of the model-runtime
/// design.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ModelRuntimeMetricsSnapshot {
    pub total_calls: u64,
    pub total_timeouts: u64,
    pub total_errors: u64,
    pub total_fallbacks: u64,
    /// Serialised as a flat list of `(kind, histogram)` rows
    /// so the JSON shape stays small even when the
    /// histograms grow.
    pub by_kind: Vec<ModelMetricsRow>,
    pub by_model: Vec<ModelMetricsRow>,
    pub last_observed_at: Option<chrono::DateTime<Utc>>,
}

/// One row of the model-runtime metrics table.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct ModelMetricsRow {
    pub key: String,
    pub count: u64,
    pub sum_ms: u64,
    pub min_ms: u64,
    pub max_ms: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
    pub timeouts: u64,
    pub errors: u64,
}

impl From<(String, objective_core::traits::LatencyHistogram)> for ModelMetricsRow {
    fn from((key, h): (String, objective_core::traits::LatencyHistogram)) -> Self {
        Self {
            key,
            count: h.count,
            sum_ms: h.sum_ms,
            min_ms: h.min_ms,
            max_ms: h.max_ms,
            p50_ms: h.p50_ms,
            p95_ms: h.p95_ms,
            p99_ms: h.p99_ms,
            timeouts: h.timeouts,
            errors: h.errors,
        }
    }
}

impl From<&(String, objective_core::traits::LatencyHistogram)> for ModelMetricsRow {
    fn from(pair: &(String, objective_core::traits::LatencyHistogram)) -> Self {
        Self {
            key: pair.0.clone(),
            count: pair.1.count,
            sum_ms: pair.1.sum_ms,
            min_ms: pair.1.min_ms,
            max_ms: pair.1.max_ms,
            p50_ms: pair.1.p50_ms,
            p95_ms: pair.1.p95_ms,
            p99_ms: pair.1.p99_ms,
            timeouts: pair.1.timeouts,
            errors: pair.1.errors,
        }
    }
}

/// Snapshot of current pipeline metrics.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    /// Optional model-runtime metrics snapshot. `Some`
    /// when the daemon was started with a `Local`
    /// `ModelRuntimeConfig`; `None` when the model runtime
    /// is `Disabled` or `Heuristic`. Phase 4.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model_runtime: Option<ModelRuntimeMetricsSnapshot>,
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
            model_runtime: None,
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
    /// Replace the cached model-runtime metrics snapshot.
    /// Called by the model runtime on every infer call (or
    /// on a polling cadence) so the `/api/v1/monitoring`
    /// route and the persisted JSON on disk both reflect
    /// the live histograms. Phase 4.
    pub fn update_model_runtime(&self, snapshot: ModelRuntimeMetricsSnapshot) {
        if let Ok(mut m) = self.metrics.write() {
            m.model_runtime = Some(snapshot);
        }
    }

    /// Read the cached model-runtime metrics snapshot.
    /// Used by the recovery service and tests.
    pub fn model_runtime_metrics(&self) -> Option<ModelRuntimeMetricsSnapshot> {
        self.metrics.read().ok().and_then(|m| m.model_runtime.clone())
    }

    pub fn new(data_root: &Path) -> Self {
        let state_dir = data_root.join("state");
        let _ = std::fs::create_dir_all(&state_dir);
        let persist_path = state_dir.join("monitoring.json");
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

    /// Return the on-disk path used to persist metrics. Used by the
    /// recovery service and tests.
    pub fn persist_path(&self) -> &std::path::Path {
        &self.persist_path
    }

    /// Replace the in-memory metrics with the supplied snapshot. Intended
    /// for tests and recovery flows that need to seed a known state.
    pub fn replace_metrics(&self, mut metrics: PipelineMetrics) {
        metrics.uptime_secs = self.started_at.elapsed().as_secs();
        if let Ok(mut guard) = self.metrics.write() {
            *guard = metrics;
        }
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

    #[test]
    fn test_model_runtime_metrics_round_trip() {
        let dir = temp_dir();
        let service = MonitoringService::new(&dir);
        // Initially absent.
        assert!(service.model_runtime_metrics().is_none());

        let snap = ModelRuntimeMetricsSnapshot {
            total_calls: 4,
            total_timeouts: 1,
            total_errors: 0,
            total_fallbacks: 0,
            by_kind: vec![ModelMetricsRow {
                key: "claim_extraction".to_string(),
                count: 4,
                sum_ms: 800,
                min_ms: 100,
                max_ms: 300,
                p50_ms: 200,
                p95_ms: 300,
                p99_ms: 300,
                timeouts: 1,
                errors: 0,
            }],
            by_model: vec![ModelMetricsRow {
                key: "mistral-7b-instruct".to_string(),
                count: 4,
                sum_ms: 800,
                min_ms: 100,
                max_ms: 300,
                p50_ms: 200,
                p95_ms: 300,
                p99_ms: 300,
                timeouts: 1,
                errors: 0,
            }],
            last_observed_at: None,
        };
        service.update_model_runtime(snap.clone());
        let stored = service
            .model_runtime_metrics()
            .expect("snapshot should be present after update");
        assert_eq!(stored, snap);
        let m = service.get_metrics();
        assert_eq!(m.model_runtime.as_ref().map(|s| s.total_calls), Some(4));
    }
}
