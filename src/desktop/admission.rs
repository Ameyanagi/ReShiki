//! The UI's one-shot answer to a desktop request; the listener waits on it.
use super::protocol::Status;
use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

#[derive(Debug)]
pub(crate) struct Admission {
    value: Mutex<Status>,
    changed: Condvar,
}
impl Admission {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            value: Mutex::new(Status::Pending),
            changed: Condvar::new(),
        })
    }
    pub(crate) fn set(&self, status: Status) {
        if let Ok(mut value) = self.value.lock() {
            // UI admission is a one-shot decision. A late callback must not
            // change a rejection/expiry into an accepted request.
            if *value == Status::Pending && status != Status::Pending {
                *value = status;
                self.changed.notify_all();
            }
        }
    }
    pub(crate) fn get(&self, timeout: Duration) -> Status {
        let Ok(value) = self.value.lock() else {
            return Status::Busy;
        };
        self.changed
            .wait_timeout_while(value, timeout, |value| *value == Status::Pending)
            .map_or(Status::Busy, |(value, _)| *value)
    }
}
