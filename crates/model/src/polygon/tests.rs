use super::*;

#[test]
fn signed_area_unit_square_has_twice_the_area_with_orientation() {
    let mut points = [
        Point::new(0., 0.),
        Point::new(1., 0.),
        Point::new(1., 1.),
        Point::new(0., 1.),
    ];
    assert_eq!(signed_area(&points), 2.);
    points.reverse();
    assert_eq!(signed_area(&points), -2.);
}

#[test]
fn signed_area_empty_slice_is_zero() {
    assert_eq!(signed_area(&[]), 0.);
}

#[test]
fn closed_edges_includes_closing_edge_in_order() {
    assert_eq!(
        closed_edges(&[1, 2, 3]).collect::<Vec<_>>(),
        [(1, 2), (2, 3), (3, 1)]
    );
}

#[test]
fn closed_edges_empty_slice_has_no_edges() {
    assert_eq!(closed_edges::<u64>(&[]).count(), 0);
}

#[test]
fn closed_edges_singleton_closes_to_itself() {
    assert_eq!(closed_edges(&[1]).collect::<Vec<_>>(), [(1, 1)]);
}
