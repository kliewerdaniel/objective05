//! Per-runtime metrics for the model runtime.
//!
//! Three layers:
//!
//! 1. `RuntimeMetrics` — the aggregate counters and
//!    per-kind/per-model histograms. Cheap to clone (uses
//!    `parking_lot::RwLock` for fast reads); the hot
//!    `observe` path takes a write lock for a few
//!    microseconds.
//! 2. `LatencyHistogram` (re-exported from
//!    `objective_core::traits`) — the per-bucket
//!    summary (count, sum, min, max, p50/p95/p99,
//!    timeouts, errors).
//! 3. `RuntimeMetrics::record_call` — the single entry
//!    point used by the runtime to record a completed
//!    call. Takes the elapsed time and the outcome so
//!    the histogram stays consistent.
//!
//! The metrics object is shared with the `MonitoringService`
//! and the `/api/v1/model-runtime` route so latency
//! flows end-to-end.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use objective_core::traits::{
    InferenceKind, LatencyHistogram, ModelId, ModelRuntimeMetrics,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Outcome of a single inference call. Drives which
/// histogram bucket the call lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallOutcome {
    Ok,
    Timeout,
    Error,
}

impl CallOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            CallOutcome::Ok => "ok",
            CallOutcome::Timeout => "timeout",
            CallOutcome::Error => "error",
        }
    }
}

/// Per-runtime aggregate metrics. The struct is the live
/// accumulator; the snapshot in
/// [`RuntimeMetricsSnapshot`] is the serialisable form.
#[derive(Debug, Default)]
pub struct RuntimeMetrics {
    inner: Arc<RwLock<RuntimeMetricsInner>>,
    /// Last observed timestamp (used to render `last_used_at`
    /// on the slot view). Stored as an atomic so the hot
    /// path can stamp it without taking the lock.
    last_observed_at_ms: Arc<AtomicU64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeMetricsSnapshot {
    pub total_calls: u64,
    pub total_timeouts: u64,
    pub total_errors: u64,
    pub total_fallbacks: u64,
    pub by_kind: BTreeMap<InferenceKind, LatencyHistogram>,
    pub by_model: BTreeMap<ModelId, LatencyHistogram>,
    pub last_observed_at: Option<DateTime<Utc>>,
}

impl From<&RuntimeMetricsSnapshot> for ModelRuntimeMetrics {
    fn from(snapshot: &RuntimeMetricsSnapshot) -> Self {
        Self {
            total_calls: snapshot.total_calls,
            total_fallbacks: snapshot.total_fallbacks,
            by_kind: snapshot.by_kind.clone(),
            by_model: snapshot.by_model.clone(),
        }
    }
}

#[derive(Debug, Default)]
struct RuntimeMetricsInner {
    total_calls: u64,
    total_timeouts: u64,
    total_errors: u64,
    total_fallbacks: u64,
    by_kind: BTreeMap<InferenceKind, LatencyHistogram>,
    by_model: BTreeMap<ModelId, LatencyHistogram>,
}

impl RuntimeMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one completed call. `elapsed_ms` is the
    /// measured wall-clock time; `outcome` is the resolved
    /// status. The histograms are updated for both the
    /// `kind` and the `model`.
    pub fn record_call(&self, kind: InferenceKind, model: &ModelId, elapsed_ms: u64, outcome: CallOutcome) {
        self.last_observed_at_ms
            .store(Utc::now().timestamp_millis() as u64, Ordering::Relaxed);
        let mut guard = self.inner.write().expect("runtime metrics poisoned");
        guard.total_calls = guard.total_calls.saturating_add(1);
        match outcome {
            CallOutcome::Ok => {
                let by_kind = guard.by_kind.entry(kind).or_default();
                by_kind.observe(elapsed_ms);
                let by_model = guard.by_model.entry(model.clone()).or_default();
                by_model.observe(elapsed_ms);
            }
            CallOutcome::Timeout => {
                guard.total_timeouts = guard.total_timeouts.saturating_add(1);
                guard.by_kind.entry(kind).or_default().observe_timeout();
                guard.by_model.entry(model.clone()).or_default().observe_timeout();
            }
            CallOutcome::Error => {
                guard.total_errors = guard.total_errors.saturating_add(1);
                guard.by_kind.entry(kind).or_default().observe_error();
                guard.by_model.entry(model.clone()).or_default().observe_error();
            }
        }
    }

    /// Record a single fallback event (the orchestrator
    /// decided to fall back to the heuristic on a per-chunk
    /// basis). Fallback is tracked separately from the
    /// per-kind histogram.
    pub fn record_fallback(&self) {
        let mut guard = self.inner.write().expect("runtime metrics poisoned");
        guard.total_fallbacks = guard.total_fallbacks.saturating_add(1);
    }

    /// Snapshot the metrics. Cheap; takes the read lock
    /// for a few microseconds.
    pub fn snapshot(&self) -> RuntimeMetricsSnapshot {
        let guard = self.inner.read().expect("runtime metrics poisoned");
        let last_observed_at_ms = self.last_observed_at_ms.load(Ordering::Relaxed);
        let last_observed_at = if last_observed_at_ms == 0 {
            None
        } else {
            chrono::DateTime::<Utc>::from_timestamp_millis(last_observed_at_ms as i64)
        };
        RuntimeMetricsSnapshot {
            total_calls: guard.total_calls,
            total_timeouts: guard.total_timeouts,
            total_errors: guard.total_errors,
            total_fallbacks: guard.total_fallbacks,
            by_kind: guard.by_kind.clone(),
            by_model: guard.by_model.clone(),
            last_observed_at,
        }
    }

    /// Reset all counters and histograms. Used by the
    /// `LocalModelRuntime::reload` path so a fresh build
    /// starts with an empty metrics table.
    pub fn reset(&self) {
        let mut guard = self.inner.write().expect("runtime metrics poisoned");
        guard.total_calls = 0;
        guard.total_timeouts = 0;
        guard.total_errors = 0;
        guard.total_fallbacks = 0;
        guard.by_kind.clear();
        guard.by_model.clear();
        self.last_observed_at_ms.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_metrics_are_zero() {
        let m = RuntimeMetrics::new();
        let snap = m.snapshot();
        assert_eq!(snap.total_calls, 0);
        assert_eq!(snap.total_timeouts, 0);
        assert_eq!(snap.total_errors, 0);
        assert_eq!(snap.total_fallbacks, 0);
        assert!(snap.by_kind.is_empty());
        assert!(snap.by_model.is_empty());
        assert!(snap.last_observed_at.is_none());
    }

    #[test]
    fn record_call_populates_both_histograms() {
        let m = RuntimeMetrics::new();
        m.record_call(InferenceKind::ClaimExtraction, &ModelId::Mistral7BInstruct, 120, CallOutcome::Ok);
        m.record_call(InferenceKind::ClaimExtraction, &ModelId::Mistral7BInstruct, 240, CallOutcome::Ok);
        let snap = m.snapshot();
        assert_eq!(snap.total_calls, 2);
        let kind = snap.by_kind.get(&InferenceKind::ClaimExtraction).unwrap();
        assert_eq!(kind.count, 2);
        assert_eq!(kind.sum_ms, 360);
        assert_eq!(kind.max_ms, 240);
        assert_eq!(kind.min_ms, 120);
        let model = snap.by_model.get(&ModelId::Mistral7BInstruct).unwrap();
        assert_eq!(model.count, 2);
    }

    #[test]
    fn timeouts_and_errors_increment_their_counters() {
        let m = RuntimeMetrics::new();
        m.record_call(InferenceKind::Embedding, &ModelId::BgeSmallEnV15, 0, CallOutcome::Timeout);
        m.record_call(InferenceKind::Embedding, &ModelId::BgeSmallEnV15, 0, CallOutcome::Error);
        let snap = m.snapshot();
        assert_eq!(snap.total_calls, 2);
        assert_eq!(snap.total_timeouts, 1);
        assert_eq!(snap.total_errors, 1);
        let kind = snap.by_kind.get(&InferenceKind::Embedding).unwrap();
        assert_eq!(kind.timeouts, 1);
        assert_eq!(kind.errors, 1);
    }

    #[test]
    fn fallback_counter_is_independent() {
        let m = RuntimeMetrics::new();
        m.record_call(InferenceKind::ClaimExtraction, &ModelId::Mistral7BInstruct, 10, CallOutcome::Ok);
        m.record_fallback();
        m.record_fallback();
        let snap = m.snapshot();
        assert_eq!(snap.total_calls, 1);
        assert_eq!(snap.total_fallbacks, 2);
    }

    #[test]
    fn reset_clears_everything() {
        let m = RuntimeMetrics::new();
        m.record_call(InferenceKind::ClaimExtraction, &ModelId::Mistral7BInstruct, 10, CallOutcome::Ok);
        m.record_fallback();
        m.reset();
        let snap = m.snapshot();
        assert_eq!(snap.total_calls, 0);
        assert_eq!(snap.total_fallbacks, 0);
        assert!(snap.by_kind.is_empty());
        assert!(snap.by_model.is_empty());
    }

    #[test]
    fn snapshot_converts_to_model_runtime_metrics() {
        let m = RuntimeMetrics::new();
        m.record_call(InferenceKind::ClaimExtraction, &ModelId::Mistral7BInstruct, 200, CallOutcome::Ok);
        let snap = m.snapshot();
        let model_metrics: ModelRuntimeMetrics = (&snap).into();
        assert_eq!(model_metrics.total_calls, 1);
        assert!(model_metrics
            .by_kind
            .contains_key(&InferenceKind::ClaimExtraction));
    }
}
