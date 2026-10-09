//! Retained legacy geometry from the #91 before fixture.
use crate::document::{Document, Point};

#[test]
fn mechanism_curvature_91_legacy_fixture_retains_quadratic_render_and_native_data() {
    let bytes = include_bytes!("../../../../tests/fixtures/mechanism-curvature-91/before.rsk");
    let doc = Document::from_native_file(bytes).unwrap();
    assert_eq!(doc.arrows.len(), 2);
    for arrow in &doc.arrows {
        let control = arrow.control.unwrap();
        let midpoint = Point::new(
            (arrow.start.x + 2. * control.x + arrow.end.x) / 4.,
            (arrow.start.y + 2. * control.y + arrow.end.y) / 4.,
        );
        assert_eq!(arrow.point(0.5), midpoint);
        assert_eq!(arrow.handles()[2], midpoint);
        assert!(arrow.hit(midpoint, 2.));
        let (lo, hi) = arrow.bounds();
        assert!(lo.x <= midpoint.x && midpoint.x <= hi.x);
        assert!(lo.y <= midpoint.y && midpoint.y <= hi.y);
        let mut edited = arrow.clone();
        edited.edit_handle(2, midpoint.offset(0., -10.));
        assert_eq!((edited.start, edited.end), (arrow.start, arrow.end));
        assert_eq!(edited.point(0.5), midpoint.offset(0., -10.));
    }
    let saved = serde_json::to_vec(&doc).unwrap();
    assert_eq!(Document::from_native_file(&saved).unwrap(), doc);
}
