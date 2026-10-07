use super::*;

#[test]
fn screen_angles_snap_and_bound_each_gesture_without_nonfinite_edits() {
    let drag = TiltDrag {
        ids: vec![],
        start: Point::new(100., 100.),
    };
    assert_eq!(drag.angles(Point::new(136., 74.), false), (13., 18.));
    assert_eq!(drag.angles(Point::new(136., 74.), true), (15., 15.));
    assert_eq!(drag.angles(Point::new(1000., -1000.), true), (75., 75.));
    assert_eq!(drag.angles(Point::new(101., 101.), false), (0., 0.));
    assert_eq!(drag.angles(Point::new(f32::NAN, 100.), false), (0., 0.));
    let mut doc = reshiki::rings::Preset::Regular.document(42., false);
    let before = doc.clone();
    let ids = doc.all_ids();
    for (x, y) in [(f32::NAN, 15.), (15., f32::INFINITY), (76., 15.)] {
        apply(&mut doc, &ids, x, y);
        assert_eq!(doc, before);
    }
}
