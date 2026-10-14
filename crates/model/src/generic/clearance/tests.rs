use super::*;

#[test]
fn clearance_certificate_refuses_touch_collinearity_and_review_crossing() {
    for x in [0., 0.00021] {
        assert!(
            !Footprint::segment(Point::new(0., 0.), Point::new(42., 0.), 0.).separated(
                Footprint::segment(Point::new(x, -1.), Point::new(x, 1.), 0.)
            )
        );
    }
    assert!(
        !Footprint::segment(Point::new(0., 0.), Point::new(42., 0.), 0.).separated(
            Footprint::segment(Point::new(21., 0.), Point::new(63., 0.), 0.)
        )
    );
    assert!(
        Footprint::segment(Point::new(0., 0.), Point::new(42., 0.), 1.).separated(
            Footprint::segment(Point::new(0., 3.), Point::new(42., 3.), 1.)
        )
    );
    assert!(
        !Footprint::segment(Point::new(0., 0.), Point::new(42., 0.), 1.).separated(
            Footprint::segment(Point::new(0., 2.), Point::new(42., 2.), 1.)
        )
    );
}
