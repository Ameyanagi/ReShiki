//! Operational budgets, independent of chemistry and structural admission rules.
//! Requests resolve from a memory snapshot refreshed at most once per second.
//! A small, bounded lookup probe
//! measures this executable's effective throughput; processor count is not used.
use std::{
    collections::BTreeMap,
    hint::black_box,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

mod platform;

pub const MIB: usize = 1024 * 1024;
pub const MIN_HEAP_BYTES: usize = 64 * MIB;
pub const MAX_HEAP_BYTES: usize = 512 * MIB;
pub const MIN_WORK_UNITS: usize = 5_000_000;
pub const MAX_WORK_UNITS: usize = 200_000_000;
const FALLBACK_MEMORY: u64 = 1024 * MIB as u64;
const REFERENCE_RATE: u64 = 5_000_000;
const MAX_POOL_BYTES: usize = 1280 * MIB;

/// Injectable machine observations. Missing observations use the historical
/// 256 MiB / 60 s / 50 million unit defaults. Observed exhausted memory denies
/// reservations; a missing or zero throughput sample uses the reference rate.
#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    pub memory_headroom_bytes: Option<u64>,
    pub lookup_operations_per_second: Option<u64>,
}
impl Capabilities {
    pub fn detect() -> Self {
        static RATE: OnceLock<Option<u64>> = OnceLock::new();
        static MEMORY: OnceLock<Mutex<(Instant, Option<u64>)>> = OnceLock::new();
        let observed =
            MEMORY.get_or_init(|| Mutex::new((Instant::now(), platform::memory_headroom())));
        let memory_headroom_bytes = observed.lock().ok().and_then(|mut snapshot| {
            if snapshot.0.elapsed() >= Duration::from_secs(1) {
                *snapshot = (Instant::now(), platform::memory_headroom());
            }
            snapshot.1
        });
        Self {
            memory_headroom_bytes,
            lookup_operations_per_second: *RATE.get_or_init(calibrate),
        }
    }
    pub fn resolve(self) -> Budget {
        let memory = self.memory_headroom_bytes.unwrap_or(FALLBACK_MEMORY);
        let rate = self
            .lookup_operations_per_second
            .filter(|&n| n > 0)
            .unwrap_or(REFERENCE_RATE);
        let heap_bytes = usize::try_from(memory / 4)
            .unwrap_or(MAX_HEAP_BYTES)
            .clamp(MIN_HEAP_BYTES, MAX_HEAP_BYTES);
        // The probe is a throughput proxy, not a claim about a molecule's solve
        // time. Independent wall-clock and work ceilings remain mandatory.
        let seconds = REFERENCE_RATE
            .saturating_mul(60)
            .checked_div(rate)
            .unwrap_or(60)
            .clamp(60, 120);
        let work_units = usize::try_from(rate.saturating_mul(10))
            .unwrap_or(MAX_WORK_UNITS)
            .clamp(MIN_WORK_UNITS, MAX_WORK_UNITS);
        let pool_bytes = usize::try_from(memory / 2)
            .unwrap_or(MAX_POOL_BYTES)
            .min(MAX_POOL_BYTES);
        Budget {
            heap_bytes,
            geometry_timeout: Duration::from_secs(seconds),
            analysis_timeout: Duration::from_secs(10),
            work_units,
            pool_bytes,
        }
    }
}

/// Resolved once and immutable throughout an operation.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    heap_bytes: usize,
    geometry_timeout: Duration,
    analysis_timeout: Duration,
    work_units: usize,
    pool_bytes: usize,
}
impl Budget {
    pub fn heap_bytes(self) -> usize {
        self.heap_bytes
    }
    pub fn geometry_timeout(self) -> Duration {
        self.geometry_timeout
    }
    pub fn analysis_timeout(self) -> Duration {
        self.analysis_timeout
    }
    pub fn work_units(self) -> usize {
        self.work_units
    }
    pub fn reservation_ceiling(self) -> usize {
        self.pool_bytes
    }
    /// Reserve calculation memory plus overhead. This is admission accounting,
    /// not an allocator or a total-process memory measurement.
    pub fn try_reserve(self, bytes: usize) -> Option<Reservation> {
        static POOL: OnceLock<Arc<Pool>> = OnceLock::new();
        POOL.get_or_init(|| Arc::new(Pool::default()))
            .reserve(bytes, self.pool_bytes)
    }
}

#[derive(Default)]
struct Pool {
    used: AtomicUsize,
}
impl Pool {
    fn reserve(self: &Arc<Self>, bytes: usize, ceiling: usize) -> Option<Reservation> {
        if bytes == 0 || bytes > ceiling {
            return None;
        }
        if self
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|&n| n <= ceiling)
            })
            .is_err()
        {
            return None;
        }
        Some(Reservation {
            pool: Arc::clone(self),
            bytes,
        })
    }
}
pub struct Reservation {
    pool: Arc<Pool>,
    bytes: usize,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.pool.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

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

fn calibrate() -> Option<u64> {
    let table: BTreeMap<_, _> = (0u64..256).map(|n| (n * 17, n.rotate_left(7))).collect();
    let start = Instant::now();
    let mut operations = 0u64;
    let mut value = 0u64;
    for _ in 0..2048 {
        for n in 0u64..256 {
            value ^= table.get(&black_box(n * 17)).copied().unwrap_or(0);
        }
        operations += 256;
        black_box(value);
        if start.elapsed() >= Duration::from_millis(10) {
            break;
        }
    }
    let nanos = start.elapsed().as_nanos();
    if nanos < 1_000_000 || nanos > 100_000_000 {
        return None;
    }
    u64::try_from(u128::from(operations) * 1_000_000_000 / nanos).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_clamp_and_fall_back_without_using_processor_count() {
        let fallback = Capabilities {
            memory_headroom_bytes: None,
            lookup_operations_per_second: None,
        }
        .resolve();
        assert_eq!(fallback.heap_bytes(), 256 * MIB);
        assert_eq!(fallback.geometry_timeout(), Duration::from_secs(60));
        assert_eq!(fallback.work_units(), 50_000_000);
        let low = Capabilities {
            memory_headroom_bytes: Some(256 * MIB as u64),
            lookup_operations_per_second: Some(100_000),
        }
        .resolve();
        assert_eq!(low.heap_bytes(), MIN_HEAP_BYTES);
        assert_eq!(low.geometry_timeout(), Duration::from_secs(120));
        assert_eq!(low.work_units(), MIN_WORK_UNITS);
        let high = Capabilities {
            memory_headroom_bytes: Some(u64::MAX),
            lookup_operations_per_second: Some(u64::MAX),
        }
        .resolve();
        assert_eq!(high.heap_bytes(), MAX_HEAP_BYTES);
        assert_eq!(high.geometry_timeout(), Duration::from_secs(60));
        assert_eq!(high.work_units(), MAX_WORK_UNITS);
        let exhausted = Capabilities {
            memory_headroom_bytes: Some(0),
            lookup_operations_per_second: None,
        }
        .resolve();
        assert_eq!(exhausted.reservation_ceiling(), 0);
        assert!(exhausted.try_reserve(1).is_none());
    }
    #[test]
    fn reservations_cover_aggregate_bytes_and_are_released_on_drop() {
        let pool = Arc::new(Pool::default());
        let first = pool.reserve(80, 100).unwrap();
        assert!(pool.reserve(21, 100).is_none());
        let second = pool.reserve(20, 100).unwrap();
        assert!(pool.reserve(1, 100).is_none());
        drop(first);
        assert!(pool.reserve(81, 100).is_none());
        drop(second);
        assert!(pool.reserve(100, 100).is_some());
        assert!(pool.reserve(usize::MAX, 100).is_none());
    }
    #[test]
    fn cancellation_is_shared() {
        let token = Cancellation::default();
        let other = token.clone();
        other.cancel();
        assert!(token.is_cancelled());
    }
}
