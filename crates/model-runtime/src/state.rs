//! Per-slot state machine for the model runtime.
//!
//! Each model slot (embedding, extraction_llm) carries a
//! single [`SlotStateMachine`] that records the current
//! state, the transition log, and the live `active` /
//! `queued` counts. The state machine is **not** behind a
//! mutex — every counter is its own atomic so the hot path
//! (`acquire`, `release`, `record_error`) does not block.
//!
//! State graph:
//!
//! ```text
//!   NotLoaded --warmup--> Ready
//!   Ready    --acquire--> Busy{active:1, queued:0}
//!   Busy     --acquire--> Busy{active:+1}
//!   Busy     --acquire(queued)--> Busy{queued:+1}
//!   Busy     --release--> Ready | Busy (depending on active)
//!   Busy     --error--> Error{message}
//!   Error    --clear--> Ready
//!   *        --drain--> Draining{active}
//!   Draining --drained--> Unloading
//!   Unloading --released--> NotLoaded
//! ```
//!
//! The state machine is shared with the API layer so
//! `GET /api/v1/model-runtime` can surface the live state
//! of every slot.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use objective_core::traits::{ModelSlotState, ModelSlotTransition};
use tracing::warn;

/// Maximum number of state transitions kept in the
/// transition log. The state machine overwrites the oldest
/// entry when this cap is hit.
const MAX_TRANSITIONS: usize = 32;

#[derive(Debug)]
struct StateInner {
    state: ModelSlotState,
    last_error: Option<String>,
    last_used_at: Option<DateTime<Utc>>,
    transitions: Vec<ModelSlotTransition>,
}

impl Default for StateInner {
    fn default() -> Self {
        Self {
            state: ModelSlotState::NotLoaded,
            last_error: None,
            last_used_at: None,
            transitions: Vec::new(),
        }
    }
}

/// Concrete state machine. Cheap to clone (`Arc` inside).
#[derive(Debug, Clone, Default)]
pub struct SlotStateMachine {
    inner: Arc<RwLock<StateInner>>,
    active: Arc<AtomicU32>,
    queued: Arc<AtomicU32>,
}

impl SlotStateMachine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the current state without mutating it. The
    /// returned snapshot is a deep copy of the state and
    /// the last `MAX_TRANSITIONS` transitions.
    pub fn snapshot(&self) -> (ModelSlotState, Option<String>, Option<DateTime<Utc>>, Vec<ModelSlotTransition>) {
        let guard = self.inner.read().expect("slot state machine poisoned");
        (
            guard.state.clone(),
            guard.last_error.clone(),
            guard.last_used_at,
            guard.transitions.clone(),
        )
    }

    /// Active and queued counts (mirrored in the
    /// `ModelSlotState::Busy` variant for convenience).
    pub fn active(&self) -> u32 {
        self.active.load(Ordering::Relaxed)
    }

    pub fn queued(&self) -> u32 {
        self.queued.load(Ordering::Relaxed)
    }

    /// Move the slot to `Ready`. Used after a successful
    /// warmup or after clearing an error.
    pub fn mark_ready(&self) {
        self.transition(ModelSlotState::Ready, None);
    }

    /// Mark the slot as currently loading.
    pub fn mark_loading(&self) {
        self.transition(ModelSlotState::Loading, None);
    }

    /// Record a successful call: bump `active` to at least
    /// 1, set the state to `Busy` (if not already draining
    /// or in error), and stamp the `last_used_at` clock.
    pub fn record_acquired(&self) {
        let prev = self.active.fetch_add(1, Ordering::Relaxed);
        self.record_used();
        if prev == 0 {
            self.transition(
                ModelSlotState::Busy {
                    active: 1,
                    queued: self.queued(),
                },
                None,
            );
        } else {
            // Active was already > 0; refresh the active
            // count in the existing `Busy` state so the
            // snapshot is consistent.
            self.refresh_busy();
        }
    }

    /// Record a queued caller: bump `queued` and refresh
    /// the `Busy` state's queued count.
    pub fn record_queued(&self) {
        self.queued.fetch_add(1, Ordering::Relaxed);
        self.refresh_busy();
    }

    /// Record that a queued caller gave up (queue-timeout).
    /// Decrements the queued count and refreshes the
    /// `Busy` state.
    pub fn record_unqueued(&self) {
        let prev = self.queued.load(Ordering::Relaxed);
        if prev > 0 {
            self.queued.fetch_sub(1, Ordering::Relaxed);
        }
        self.refresh_busy();
    }

    /// Record the completion of a call: decrement `active`
    /// and move the slot to `Ready` if `active` drops to
    /// zero. Does not clear `Error` if the slot is already
    /// in `Error`.
    pub fn record_released(&self) {
        let prev = self.active.load(Ordering::Relaxed);
        if prev > 0 {
            self.active.fetch_sub(1, Ordering::Relaxed);
        }
        let (state, _, _, _) = self.snapshot();
        if matches!(state, ModelSlotState::Error { .. }) {
            return;
        }
        if self.active() == 0 {
            self.transition(ModelSlotState::Ready, None);
        } else {
            self.refresh_busy();
        }
    }

    /// Record a call that errored out (non-timeout).
    pub fn record_error(&self, message: impl Into<String>) {
        let msg = message.into();
        warn!(state = ?self.snapshot().0, error = %msg, "model slot entered Error");
        self.transition(ModelSlotState::Error { message: msg }, None);
    }

    /// Begin draining. In-flight calls are allowed to
    /// finish; new acquisitions are rejected by the queue.
    pub fn mark_draining(&self) {
        let active = self.active();
        self.transition(ModelSlotState::Draining { active }, None);
    }

    /// Mark the slot as unloading (releasing the model).
    pub fn mark_unloading(&self) {
        self.transition(ModelSlotState::Unloading, None);
    }

    /// Reset to `NotLoaded`. Only used by the rebuild
    /// path so a fresh `LocalModelRuntime::from_config`
    /// starts clean.
    pub fn reset(&self) {
        let mut guard = self.inner.write().expect("slot state machine poisoned");
        guard.state = ModelSlotState::NotLoaded;
        guard.last_error = None;
        guard.last_used_at = None;
        guard.transitions.clear();
        self.active.store(0, Ordering::Relaxed);
        self.queued.store(0, Ordering::Relaxed);
    }

    fn record_used(&self) {
        let mut guard = self.inner.write().expect("slot state machine poisoned");
        guard.last_used_at = Some(Utc::now());
    }

    fn refresh_busy(&self) {
        let active = self.active();
        let queued = self.queued();
        let (state, _, _, _) = self.snapshot();
        if matches!(
            state,
            ModelSlotState::Busy { .. } | ModelSlotState::Draining { .. }
        ) {
            let next = if matches!(state, ModelSlotState::Draining { .. }) {
                ModelSlotState::Draining { active }
            } else {
                ModelSlotState::Busy { active, queued }
            };
            self.transition(next, None);
        }
    }

    fn transition(&self, to: ModelSlotState, reason: Option<String>) {
        let mut guard = self.inner.write().expect("slot state machine poisoned");
        if guard.state == to {
            return;
        }
        let from = std::mem::replace(&mut guard.state, to.clone());
        if let ModelSlotState::Error { message } = &to {
            guard.last_error = Some(message.clone());
        } else if !matches!(from, ModelSlotState::Error { .. }) {
            guard.last_error = None;
        }
        let transition = ModelSlotTransition {
            from,
            to,
            at: Utc::now(),
            reason,
        };
        guard.transitions.push(transition);
        if guard.transitions.len() > MAX_TRANSITIONS {
            let drop = guard.transitions.len() - MAX_TRANSITIONS;
            guard.transitions.drain(0..drop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_machine_is_not_loaded() {
        let m = SlotStateMachine::new();
        let (state, _, _, transitions) = m.snapshot();
        assert_eq!(state, ModelSlotState::NotLoaded);
        assert!(transitions.is_empty());
        assert_eq!(m.active(), 0);
        assert_eq!(m.queued(), 0);
    }

    #[test]
    fn mark_ready_transitions_from_not_loaded() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        let (state, _, _, transitions) = m.snapshot();
        assert_eq!(state, ModelSlotState::Ready);
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].from, ModelSlotState::NotLoaded);
        assert_eq!(transitions[0].to, ModelSlotState::Ready);
    }

    #[test]
    fn acquire_and_release_round_trip() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        m.record_acquired();
        m.record_acquired();
        assert_eq!(m.active(), 2);
        let (state, _, _, _) = m.snapshot();
        assert!(matches!(state, ModelSlotState::Busy { active: 2, queued: 0 }));

        m.record_released();
        assert_eq!(m.active(), 1);
        m.record_released();
        assert_eq!(m.active(), 0);
        let (state, _, _, _) = m.snapshot();
        assert_eq!(state, ModelSlotState::Ready);
    }

    #[test]
    fn queued_counter_round_trips() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        m.record_acquired();
        m.record_queued();
        m.record_queued();
        assert_eq!(m.queued(), 2);
        let (state, _, _, _) = m.snapshot();
        assert!(matches!(state, ModelSlotState::Busy { active: 1, queued: 2 }));

        m.record_unqueued();
        assert_eq!(m.queued(), 1);
        m.record_unqueued();
        assert_eq!(m.queued(), 0);
    }

    #[test]
    fn record_error_moves_to_error_state() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        m.record_error("onnx session died");
        let (state, last_error, _, _) = m.snapshot();
        assert!(matches!(state, ModelSlotState::Error { .. }));
        assert_eq!(last_error.as_deref(), Some("onnx session died"));
    }

    #[test]
    fn record_released_does_not_clear_error_state() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        m.record_acquired();
        m.record_error("boom");
        m.record_released();
        let (state, _, _, _) = m.snapshot();
        assert!(matches!(state, ModelSlotState::Error { .. }));
        assert_eq!(m.active(), 0);
    }

    #[test]
    fn mark_draining_carries_active_count() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        m.record_acquired();
        m.record_acquired();
        m.mark_draining();
        let (state, _, _, _) = m.snapshot();
        assert!(matches!(state, ModelSlotState::Draining { active: 2 }));
    }

    #[test]
    fn reset_returns_to_not_loaded_and_clears_counts() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        m.record_acquired();
        m.record_queued();
        m.record_error("x");
        m.reset();
        let (state, last_error, _, transitions) = m.snapshot();
        assert_eq!(state, ModelSlotState::NotLoaded);
        assert!(last_error.is_none());
        assert!(transitions.is_empty());
        assert_eq!(m.active(), 0);
        assert_eq!(m.queued(), 0);
    }

    #[test]
    fn transition_log_is_capped() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        for _ in 0..(MAX_TRANSITIONS * 2) {
            m.record_acquired();
            m.record_released();
        }
        let (_, _, _, transitions) = m.snapshot();
        assert_eq!(transitions.len(), MAX_TRANSITIONS);
    }

    #[test]
    fn snapshot_is_clone_safe() {
        let m = SlotStateMachine::new();
        m.mark_ready();
        let a = m.snapshot();
        let b = m.snapshot();
        assert_eq!(a.0, b.0);
    }
}
