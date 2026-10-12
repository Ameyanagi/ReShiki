use super::*;

#[test]
fn resolved_budgets_match_literal_policy_goldens() {
    // Keep these expected values independent of the policy constants so changes
    // to the implementation cannot silently update the historical behavior.
    for (memory, heap, pool) in [
        (None, 256 * MIB, 512 * MIB),
        (Some(0), 64 * MIB, 0),
        (Some(256 * MIB as u64 - 1), 64 * MIB, 128 * MIB - 1),
        (Some(256 * MIB as u64), 64 * MIB, 128 * MIB),
        (Some(256 * MIB as u64 + 1), 64 * MIB, 128 * MIB),
        (Some(256 * MIB as u64 + 4), 64 * MIB + 1, 128 * MIB + 2),
        (Some(2048 * MIB as u64 - 1), 512 * MIB - 1, 1024 * MIB - 1),
        (Some(2048 * MIB as u64), 512 * MIB, 1024 * MIB),
        (Some(2048 * MIB as u64 + 1), 512 * MIB, 1024 * MIB),
        (Some(2048 * MIB as u64 + 4), 512 * MIB, 1024 * MIB + 2),
        (Some(2560 * MIB as u64 - 1), 512 * MIB, 1280 * MIB - 1),
        (Some(2560 * MIB as u64), 512 * MIB, 1280 * MIB),
        (Some(2560 * MIB as u64 + 1), 512 * MIB, 1280 * MIB),
        (Some(u64::MAX), 512 * MIB, 1280 * MIB),
    ] {
        let budget = Capabilities {
            memory_headroom_bytes: memory,
            lookup_operations_per_second: None,
        }
        .resolve();
        assert_eq!(budget.heap_bytes(), heap, "memory={memory:?}");
        assert_eq!(budget.reservation_ceiling(), pool, "memory={memory:?}");
        assert_eq!(budget.geometry_timeout(), Duration::from_secs(60));
        assert_eq!(budget.analysis_timeout(), Duration::from_secs(10));
        assert_eq!(budget.work_units(), 50_000_000);
    }
    for (rate, seconds, work) in [
        (None, 60, 50_000_000),
        (Some(0), 60, 50_000_000),
        (Some(100_000), 120, 5_000_000),
        (Some(499_999), 120, 5_000_000),
        (Some(500_000), 120, 5_000_000),
        (Some(500_001), 120, 5_000_010),
        (Some(2_500_000), 120, 25_000_000),
        (Some(2_500_001), 119, 25_000_010),
        (Some(3_333_333), 90, 33_333_330),
        (Some(5_000_000), 60, 50_000_000),
        (Some(5_000_001), 60, 50_000_010),
        (Some(19_999_999), 60, 199_999_990),
        (Some(20_000_000), 60, 200_000_000),
        (Some(20_000_001), 60, 200_000_000),
        (Some(u64::MAX), 60, 200_000_000),
    ] {
        let budget = Capabilities {
            memory_headroom_bytes: None,
            lookup_operations_per_second: rate,
        }
        .resolve();
        assert_eq!(budget.heap_bytes(), 256 * MIB);
        assert_eq!(budget.reservation_ceiling(), 512 * MIB);
        assert_eq!(
            budget.geometry_timeout(),
            Duration::from_secs(seconds),
            "rate={rate:?}"
        );
        assert_eq!(budget.analysis_timeout(), Duration::from_secs(10));
        assert_eq!(budget.work_units(), work, "rate={rate:?}");
    }
    let saturated = Capabilities {
        memory_headroom_bytes: Some(u64::MAX),
        lookup_operations_per_second: Some(u64::MAX),
    }
    .resolve();
    assert_eq!(saturated.heap_bytes(), 512 * MIB);
    assert_eq!(saturated.reservation_ceiling(), 1280 * MIB);
    assert_eq!(saturated.geometry_timeout(), Duration::from_secs(60));
    assert_eq!(saturated.analysis_timeout(), Duration::from_secs(10));
    assert_eq!(saturated.work_units(), 200_000_000);
}

#[test]
fn historical_defaults_match_named_policy_constants() {
    let budget = Capabilities {
        memory_headroom_bytes: None,
        lookup_operations_per_second: None,
    }
    .resolve();
    assert_eq!(
        budget.heap_bytes(),
        usize::try_from(FALLBACK_MEMORY / HEAP_SHARE_DIVISOR).unwrap()
    );
    assert_eq!(
        budget.geometry_timeout(),
        Duration::from_secs(MIN_GEOMETRY_SECONDS)
    );
    assert_eq!(
        budget.work_units(),
        usize::try_from(REFERENCE_RATE * WORK_UNITS_PER_RATE).unwrap()
    );
}

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
