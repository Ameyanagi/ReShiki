//! Admission and cancellation for the requests a connection has in flight.
use super::{Key, Limits};
use std::{
    collections::HashMap,
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
};

/// The outcome of [`Tracker::admit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admit {
    /// Admitted: the request now holds a slot until [`Tracker::complete`].
    Ok,
    /// A request with the same id is still outstanding.
    Duplicate,
    /// The connection is closing; nothing more is admitted.
    Closed,
}

struct Entry {
    bytes: usize,
    cancelled: bool,
}

#[derive(Default)]
struct State {
    outstanding: HashMap<Key, Entry>,
    retained: usize,
    high_water: usize,
    closed: bool,
}

/// Requests read but not yet answered. The reader admits each request
/// before forwarding it, and the writer completes it once its response is
/// written or suppressed, so every slot is held for exactly one response.
pub struct Tracker {
    state: Mutex<State>,
    /// Notified when a slot frees or the tracker closes.
    space: Condvar,
    max_outstanding: usize,
    max_retained_bytes: usize,
}

impl Tracker {
    /// A tracker bounded by `limits.max_outstanding` (at least one) and
    /// `limits.max_retained_bytes`.
    pub fn new(limits: &Limits) -> Self {
        Self {
            state: Mutex::default(),
            space: Condvar::new(),
            max_outstanding: limits.max_outstanding.max(1),
            max_retained_bytes: limits.max_retained_bytes,
        }
    }

    /// Recovers from poisoning, so completion and close always run.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Admits a request of `bytes` bytes, blocking until a slot and enough
    /// retained-byte budget are free. A single request is always admissible
    /// when nothing is outstanding, whatever its size.
    pub fn admit(&self, key: &Key, bytes: usize) -> Admit {
        let mut state = self.state();
        loop {
            if state.closed {
                return Admit::Closed;
            }
            if state.outstanding.contains_key(key) {
                return Admit::Duplicate;
            }
            let full = state.outstanding.len() >= self.max_outstanding
                || (!state.outstanding.is_empty()
                    && state.retained.saturating_add(bytes) > self.max_retained_bytes);
            if !full {
                break;
            }
            state = self
                .space
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.outstanding.insert(
            key.clone(),
            Entry {
                bytes,
                cancelled: false,
            },
        );
        state.retained = state.retained.saturating_add(bytes);
        state.high_water = state.high_water.max(state.outstanding.len());
        Admit::Ok
    }

    /// Marks an outstanding request cancelled. True only when `key` is
    /// outstanding and was not already marked. The mark stays until
    /// [`Tracker::complete`], so each request is newly marked at most once.
    pub fn cancel(&self, key: &Key) -> bool {
        match self.state().outstanding.get_mut(key) {
            Some(entry) if !entry.cancelled => {
                entry.cancelled = true;
                true
            }
            _ => false,
        }
    }

    /// Whether `key` is outstanding and cancelled. The writer calls this to
    /// suppress the response.
    pub fn is_cancelled(&self, key: &Key) -> bool {
        self.state()
            .outstanding
            .get(key)
            .is_some_and(|entry| entry.cancelled)
    }

    /// Frees the slot of `key`, if it holds one.
    pub fn complete(&self, key: &Key) {
        let mut state = self.state();
        if let Some(entry) = state.outstanding.remove(key) {
            state.retained = state.retained.saturating_sub(entry.bytes);
            self.space.notify_all();
        }
    }

    /// Refuses every later admission and wakes a blocked one.
    pub fn close(&self) {
        self.state().closed = true;
        self.space.notify_all();
    }

    /// Requests admitted and not yet completed.
    pub fn outstanding(&self) -> usize {
        self.state().outstanding.len()
    }

    /// Outstanding requests that were not cancelled: responses the client
    /// still expects. Once the writer ended, none of them can be answered.
    pub fn unanswered(&self) -> usize {
        self.state()
            .outstanding
            .values()
            .filter(|entry| !entry.cancelled)
            .count()
    }

    /// The most requests ever outstanding at once.
    pub fn high_water(&self) -> usize {
        self.state().high_water
    }
}
