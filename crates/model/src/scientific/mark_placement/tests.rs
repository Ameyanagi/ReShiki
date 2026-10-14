use super::*;

#[test]
fn a_wedge_closing_edge_blocks_a_mark_even_when_its_center_is_outside_the_fill() {
    let wedge = Obstacle {
        points: vec![
            Point::new(-10., -10.),
            Point::new(10., -10.),
            Point::new(10., 10.),
        ],
        width: 0.,
        filled: true,
    };
    assert!(intersects(
        &wedge,
        (Point::new(-1., -0.5), Point::new(1., 1.5))
    ));
    assert!(!intersects(
        &wedge,
        (Point::new(-5., 2.), Point::new(-3., 4.))
    ));
    assert!(intersects(
        &wedge,
        (Point::new(3., -1.), Point::new(4., 0.))
    ));
}
