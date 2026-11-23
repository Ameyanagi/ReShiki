//! Process-wide admission for batch PNGs' raw RGBA storage, not total RSS.
//! Documents, SVG trees, rendering/encoder scratch space and encoded outputs are separate.
use std::sync::{Condvar, Mutex};

const BATCH_BITMAP_BYTES: usize = 128 * 1024 * 1024;
static BATCH: BitmapBudget = BitmapBudget::new(BATCH_BITMAP_BYTES);

pub(crate) fn batch() -> &'static BitmapBudget {
    &BATCH
}

pub(crate) struct BitmapBudget {
    capacity: usize,
    state: Mutex<State>,
    available: Condvar,
}

struct State {
    used: usize,
    #[cfg(test)]
    waiting: usize,
}

impl BitmapBudget {
    const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            state: Mutex::new(State {
                used: 0,
                #[cfg(test)]
                waiting: 0,
            }),
            available: Condvar::new(),
        }
    }

    pub(crate) fn acquire(&self, bytes: usize) -> Result<BitmapPermit<'_>, String> {
        // A request that can never fit must fail before entering the wait.
        if bytes > self.capacity {
            return Err("PNG exceeds the batch raw bitmap memory budget".into());
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        while bytes > self.capacity - state.used {
            #[cfg(test)]
            {
                state.waiting += 1;
            }
            state = self
                .available
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
            #[cfg(test)]
            {
                state.waiting -= 1;
            }
        }
        state.used += bytes;
        Ok(BitmapPermit {
            budget: self,
            bytes,
        })
    }
}

pub(crate) struct BitmapPermit<'a> {
    budget: &'a BitmapBudget,
    bytes: usize,
}

impl Drop for BitmapPermit<'_> {
    fn drop(&mut self) {
        let mut state = self
            .budget
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.used -= self.bytes;
        drop(state);
        self.budget.available.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    fn used(budget: &BitmapBudget) -> usize {
        budget.state.lock().unwrap().used
    }

    fn wait_for_blocked(budget: &BitmapBudget) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if budget.state.lock().unwrap().waiting > 0 {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "No bitmap request entered the wait"
            );
            thread::yield_now();
        }
    }

    #[test]
    fn weighted_requests_wait_until_enough_bytes_are_released() {
        let budget = BitmapBudget::new(10);
        let six = budget.acquire(6).unwrap();
        let four = budget.acquire(4).unwrap();
        assert_eq!(used(&budget), 10);
        thread::scope(|scope| {
            let budget = &budget;
            let (acquired, receipt) = mpsc::channel();
            let (release, released) = mpsc::channel();
            scope.spawn(move || {
                let _seven = budget.acquire(7).unwrap();
                acquired.send(()).unwrap();
                released.recv().unwrap();
            });
            wait_for_blocked(budget);
            assert!(receipt.try_recv().is_err());
            drop(four);
            assert_eq!(used(budget), 6);
            assert!(receipt.try_recv().is_err());
            drop(six);
            receipt.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(used(budget), 7);
            release.send(()).unwrap();
        });
        assert_eq!(used(&budget), 0);
        assert!(budget.acquire(10).is_ok());
    }

    #[test]
    fn overlapping_callers_share_capacity_and_release_their_own_weight() {
        let budget = BitmapBudget::new(16);
        thread::scope(|scope| {
            let budget = &budget;
            let (acquired, receipt) = mpsc::channel();
            let (release_first, first_released) = mpsc::channel();
            let (release_second, second_released) = mpsc::channel();
            let first_acquired = acquired.clone();
            scope.spawn(move || {
                let _permit = budget.acquire(4).unwrap();
                first_acquired.send(()).unwrap();
                first_released.recv().unwrap();
            });
            scope.spawn(move || {
                let _permit = budget.acquire(4).unwrap();
                acquired.send(()).unwrap();
                second_released.recv().unwrap();
            });
            receipt.recv_timeout(Duration::from_secs(5)).unwrap();
            receipt.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(used(budget), 8);
            let (large_acquired, large_receipt) = mpsc::channel();
            let (release_large, large_released) = mpsc::channel();
            scope.spawn(move || {
                let _permit = budget.acquire(12).unwrap();
                large_acquired.send(()).unwrap();
                large_released.recv().unwrap();
            });
            wait_for_blocked(budget);
            assert!(large_receipt.try_recv().is_err());
            release_first.send(()).unwrap();
            large_receipt.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(used(budget), 16);
            release_second.send(()).unwrap();
            release_large.send(()).unwrap();
        });
        assert_eq!(used(&budget), 0);
    }

    #[test]
    fn oversized_requests_fail_without_waiting_or_reserving_bytes() {
        let budget = BitmapBudget::new(10);
        let _full = budget.acquire(10).unwrap();
        assert!(budget.acquire(11).is_err());
        assert_eq!(used(&budget), 10);
        assert_eq!(budget.state.lock().unwrap().waiting, 0);
    }

    #[test]
    fn errors_and_panics_return_the_reservation() {
        let budget = BitmapBudget::new(10);
        let failed = || -> Result<(), &str> {
            let _permit = budget.acquire(10).unwrap();
            Err("Image allocation or encoding failed")
        };
        assert!(failed().is_err());
        assert_eq!(used(&budget), 0);
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _permit = budget.acquire(10).unwrap();
                panic!("Image encoding panicked");
            }))
            .is_err()
        );
        assert_eq!(used(&budget), 0);
        assert!(budget.acquire(10).is_ok());
    }

    #[test]
    fn batches_use_one_process_wide_default_budget() {
        assert!(std::ptr::eq(batch(), batch()));
        assert_eq!(batch().capacity, 128 * 1024 * 1024);
    }
}
