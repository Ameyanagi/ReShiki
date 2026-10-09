use super::*;

#[test]
fn mechanism_curvature_91_keeps_independent_cubic_controls_exactly() {
    let xml = include_str!(
        "../../../../../../tests/fixtures/mechanism-curvature-91/independent-controls.cdxml"
    );
    let reader = ArrowReader::new(xml).unwrap();
    let tree = presentation::parse(xml).unwrap();
    let colors = presentation::palette(tree.root_element()).unwrap().colors;
    let arrow = reader
        .read(5, 1., &colors, 1)
        .unwrap()
        .into_document()
        .unwrap();
    assert_eq!(arrow.start, Point::new(100., 100.));
    assert_eq!(arrow.end, Point::new(136., 100.));
    assert_eq!(arrow.control, None);
    assert_eq!(
        arrow.cubic,
        Some([Point::new(100., 76.), Point::new(136., 124.)])
    );
    assert_eq!(arrow.appearance().head, Head::Full);
    assert_eq!(arrow.appearance().color, crate::palette::Color::Ink);
    arrow.validate().unwrap();
}
