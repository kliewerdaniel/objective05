//! Per-slot inference queue.
//!
//! Each model slot is fronted by a `tokio::sync::Semaphore`
//! with `max_concurrency` permits. Callers wait up to
//! `queue_timeout_ms` for a permit; if the wait times out,
//! the queue returns `Err(QueueTimeout)` so the
//! orchestrator can fall back to the heuristic on a
//! per-chunk basis.
//!
//! The queue also tracks the live `active` and `queued`
//! counts and pushes them into the
//! [`SlotStateMachine`](crate::state::SlotStateMachine) so
//! the API surface can render saturation per slot.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;
use tracing::warn;

use crate::state::SlotStateMachine;

/// Permit returned by [`SlotQueue::acquire`]. The permit is
/// bound to the queue and the slot state machine; dropping
/// it (or calling [`SlotGuard::release`]) decrements the
/// `active` counter and returns the slot to `Ready` (or
/// `Busy` if more permits are held).
#[derive(Debug)]
pub struct SlotGuard {
    _permit: Option<OwnedSemaphorePermit>,
    state: SlotStateMachine,
}

impl SlotGuard {
    /// Release the permit early. Equivalent to dropping
    /// the guard, but lets the caller observe the state
    /// machine before the next operation.
    pub fn release(mut self) {
        if let Some(permit) = self._permit.take() {
            drop(permit);
        }
        self.state.record_released();
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if self._permit.is_some() {
            self.state.record_released();
        }
    }
}

/// A timed-out queue acquisition. Surfaced as
/// `ModelError::Timeout(queue_timeout)` from the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueTimeout(pub Duration);

impl std::fmt::Display for QueueTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "queue acquire timed out after {:?}", self.0)
    }
}

impl std::error::Error for QueueTimeout {}

#[derive(Debug, Clone)]
pub struct SlotQueue {
    semaphore: Arc<Semaphore>,
    state: SlotStateMachine,
    max_concurrency: u32,
    queue_timeout: Duration,
}

impl SlotQueue {
    /// Build a queue. `max_concurrency` must be at least 1
    /// or the semaphore will panic on construction.
    pub fn new(max_concurrency: u32, queue_timeout: Duration) -> Self {
        let permits = max_concurrency.max(1) as usize;
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
            state: SlotStateMachine::new(),
            max_concurrency: max_concurrency.max(1),
            queue_timeout,
        }
    }

    /// Borrow the underlying state machine. The state
    /// machine is shared between the queue and the
    /// `LocalModelRuntime` so callers can render the
    /// `ModelSlotView` without going through the queue.
    pub fn state(&self) -> SlotStateMachine {
        self.state.clone()
    }

    pub fn max_concurrency(&self) -> u32 {
        self.max_concurrency
    }

    pub fn queue_timeout(&self) -> Duration {
        self.queue_timeout
    }

    /// Acquire a permit, waiting up to `queue_timeout` for
    /// one to become available. Returns
    /// `Err(QueueTimeout)` if the wait expires.
    pub async fn acquire(&self) -> Result<SlotGuard, QueueTimeout> {
        self.state.record_queued();
        let result = timeout(self.queue_timeout, self.semaphore.clone().acquire_owned()).await;
        self.state.record_unqueued();
        match result {
            Ok(Ok(permit)) => {
                self.state.record_acquired();
                Ok(SlotGuard {
                    _permit: Some(permit),
                    state: self.state.clone(),
                })
            }
            Ok(Err(_closed)) => {
                warn!("model slot semaphore closed");
                Err(QueueTimeout(self.queue_timeout))
            }
            Err(_elapsed) => Err(QueueTimeout(self.queue_timeout)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn acquire_returns_permit_immediately_when_uncontested() {
        let queue = SlotQueue::new(2, Duration::from_secs(1));
        let guard = queue.acquire().await.unwrap();
        assert_eq!(queue.state().active(), 1);
        drop(guard);
        assert_eq!(queue.state().active(), 0);
    }

    #[tokio::test]
    async fn acquire_blocks_until_permit_available() {
        let queue = SlotQueue::new(1, Duration::from_secs(1));
        let _g1 = queue.acquire().await.unwrap();
        let queue2 = queue.clone();
        let h = tokio::spawn(async move { queue2.acquire().await });
        // Give the spawned task a moment to queue.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(queue.state().queued(), 1);
        drop(_g1);
        let guard = h.await.unwrap().unwrap();
        assert_eq!(queue.state().active(), 1);
        guard.release();
    }

    #[tokio::test]
    async fn acquire_times_out_when_queue_is_saturated() {
        let queue = SlotQueue::new(1, Duration::from_millis(50));
        let _g1 = queue.acquire().await.unwrap();
        let err = queue.acquire().await.unwrap_err();
        assert_eq!(err, QueueTimeout(Duration::from_millis(50)));
        assert_eq!(queue.state().queued(), 0);
    }

    #[tokio::test]
    async fn zero_max_concurrency_is_normalised_to_one() {
        let queue = SlotQueue::new(0, Duration::from_secs(1));
        assert_eq!(queue.max_concurrency(), 1);
        let _g = queue.acquire().await.unwrap();
    }

    #[tokio::test]
    async fn release_returns_active_to_zero() {
        let queue = SlotQueue::new(2, Duration::from_secs(1));
        let g1 = queue.acquire().await.unwrap();
        let g2 = queue.acquire().await.unwrap();
        assert_eq!(queue.state().active(), 2);
        g1.release();
        assert_eq!(queue.state().active(), 1);
        g2.release();
        assert_eq!(queue.state().active(), 0);
        let (state, _, _, _) = queue.state().snapshot();
        assert_eq!(state, objective_core::traits::ModelSlotState::Ready);
    }
}
