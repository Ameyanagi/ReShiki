use super::*;
#[test]
fn changing_color_on_a_complete_circle_does_not_open_gaps() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::default(), 6, false, 0.);
    toggle(&mut doc, &ids).unwrap();
    doc.bonds[0].color = crate::palette::Color::Custom([32, 80, 145]);
    let strokes = render(&doc).primitives;
    assert_eq!(strokes.len(), 2);
    let ends: Vec<_> = strokes
        .iter()
        .map(|p| {
            let Primitive::Path { commands, .. } = p else {
                panic!("Expected arc");
            };
            let Some(PathCommand::Move(start)) = commands.first() else {
                panic!("Missing start");
            };
            let Some(PathCommand::Cubic(_, _, end)) = commands.last() else {
                panic!("Missing end");
            };
            (*start, *end)
        })
        .collect();
    assert!(ends[0].0.distance(ends[1].1) < 0.001);
    assert!(ends[1].0.distance(ends[0].1) < 0.001);
}
#[test]
fn partial_curve_tracks_ring_and_tilt_without_changing_chemistry() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::default(), 5, false, 0.);
    doc.bonds[0].order = 2;
    let original = doc.clone();
    toggle(&mut doc, &ids[..3]).unwrap();
    assert_eq!(render(&doc).bonds.len(), 2);
    assert_eq!(render(&doc).primitives.len(), 1);
    let roundtrip: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(roundtrip, doc);
    let before = crate::scene::svg(&doc);
    crate::projection::tilt(&mut doc, &ids, 60., true);
    assert_eq!(render(&doc).bonds.len(), 2);
    assert_ne!(before, crate::scene::svg(&doc));
    crate::projection::tilt(&mut doc, &ids, -60., true);
    toggle(&mut doc, &ids[..3]).unwrap();
    assert_eq!(doc.bonds, original.bonds);
}
#[test]
fn broken_ring_restores_order_lines_and_stereo_cannot_be_hidden() {
    let mut doc = Document::default();
    let ids = crate::editing::ring(&mut doc, Point::default(), 5, false, 0.);
    doc.bonds[0].display = "wedge".into();
    assert!(toggle(&mut doc, &ids[..3]).is_err());
    doc.bonds[0].display = "plain".into();
    toggle(&mut doc, &ids[..3]).unwrap();
    doc.delete(&[ids[4]]);
    assert!(render(&doc).bonds.is_empty());
}
