use super::*;
#[test]
fn legacy_capacity_and_explicit_capacity_cannot_bypass_the_heap_envelope() {
    let low = reshiki_geometry::Capacity::for_heap(64 * 1024 * 1024);
    assert_eq!(resolved_capacity(64 * 1024 * 1024, None).unwrap(), low);
    assert!(
        resolved_capacity(
            64 * 1024 * 1024,
            Some(reshiki_geometry::Capacity::default())
        )
        .is_err()
    );
    let high = reshiki_geometry::Capacity::for_heap(512 * 1024 * 1024);
    assert_eq!(
        resolved_capacity(512 * 1024 * 1024, Some(high)).unwrap(),
        high
    );
    assert_eq!(
        resolved_capacity(512 * 1024 * 1024, None).unwrap(),
        reshiki_geometry::Capacity::default()
    );
    assert!(
        resolved_capacity(
            512 * 1024 * 1024,
            Some(reshiki_geometry::Capacity { atoms: 0, ..high })
        )
        .is_err()
    );
}
