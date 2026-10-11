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
