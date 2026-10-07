use super::*;
#[test]
fn aromatic_circle_tilts_with_ring_without_losing_radius_or_connectivity() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::default(), 6, true, 5.);
    let original = circles(&doc);
    let radius = original[0].radius;
    let bonds = doc.bonds.clone();
    crate::projection::tilt(&mut doc, &ids, 60., true);
    let tilted = circles(&doc);
    assert_eq!(tilted.len(), 1);
    assert!((tilted[0].radius - radius).abs() < 0.001);
    let ellipse = tilted[0].graphic();
    // Measure the ellipse itself: drawing bounds also include the stroke
    // and Bézier control points, which are not its projected radii.
    let width = ellipse.axis_x.x.hypot(ellipse.axis_y.x);
    let height = ellipse.axis_x.y.hypot(ellipse.axis_y.y);
    assert!((height / width - 0.5).abs() < 0.001);
    assert_eq!(doc.bonds, bonds);
}
