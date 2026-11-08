//! Host-supplied limits for molecular perception.
//!
//! The default is unbounded. Hosts may inject their own reservation accounting;
//! dropping the returned guard releases a reservation. The kernel does not
//! detect capabilities or maintain a process-wide resource pool.
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub const MIB: usize = 1024 * 1024;
type Reservation = Box<dyn Send + Sync>;
type Reserve = dyn Fn(usize) -> Option<Reservation> + Send + Sync;

/// Resolved once by the host and immutable throughout an operation.
#[derive(Clone)]
pub struct Budget {
    heap_bytes: usize,
    work_units: usize,
    analysis_timeout: Option<Duration>,
    reservation_ceiling: usize,
    reserve: Arc<Reserve>,
}

impl fmt::Debug for Budget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Budget")
            .field("heap_bytes", &self.heap_bytes)
            .field("work_units", &self.work_units)
            .field("analysis_timeout", &self.analysis_timeout)
            .field("reservation_ceiling", &self.reservation_ceiling)
            .finish_non_exhaustive()
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(usize::MAX, usize::MAX, None, usize::MAX, |_| {
            Some(Box::new(()))
        })
    }
}

impl Budget {
    /// Inject limits and a reservation hook. `None` means no wall-clock limit.
    pub fn new(
        heap_bytes: usize,
        work_units: usize,
        analysis_timeout: Option<Duration>,
        reservation_ceiling: usize,
        reserve: impl Fn(usize) -> Option<Box<dyn Send + Sync>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            heap_bytes,
            work_units,
            analysis_timeout,
            reservation_ceiling,
            reserve: Arc::new(reserve),
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.heap_bytes
    }

    pub fn work_units(&self) -> usize {
        self.work_units
    }

    pub fn analysis_timeout(&self) -> Option<Duration> {
        self.analysis_timeout
    }

    pub fn reservation_ceiling(&self) -> usize {
        self.reservation_ceiling
    }

    /// Hold the returned guard until the operation finishes.
    pub fn try_reserve(&self, bytes: usize) -> Option<Box<dyn Send + Sync>> {
        (self.reserve)(bytes)
    }
}

/// Cooperative cancellation, copied from ReShiki's host policy support.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::Budget;
    use crate::{smiles, stereo};

    #[test]
    fn every_import_entry_point_uses_the_injected_budget() {
        let budget = Budget::new(0, usize::MAX, None, usize::MAX, |_| Some(Box::new(())));
        for error in [
            smiles::prepare("C", &budget).err().unwrap().to_string(),
            smiles::read("C", &budget).err().unwrap().to_string(),
            smiles::stereoisomers("C", &budget)
                .err()
                .unwrap()
                .to_string(),
            smiles::reaction_part(smiles::parse("C").unwrap(), Vec::new(), &budget)
                .err()
                .unwrap()
                .to_string(),
        ] {
            assert!(
                error.contains("estimated memory budget exceeded"),
                "{error}"
            );
        }

        let graph = smiles::parse("CC(F)C(Cl)C").unwrap().graph;
        let axis = graph
            .bonds
            .iter()
            .position(|bond| (bond.a == 1 && bond.b == 3) || (bond.a == 3 && bond.b == 1))
            .unwrap();
        let error = stereo::axis_ends_distinct(&graph, axis, &budget).unwrap_err();
        assert!(
            error.contains("estimated memory budget exceeded"),
            "{error}"
        );
    }
}
