use crate::{document::Document, graphics::PathCommand};

#[test]
fn retained_pen_shapes_are_connected_editable_paths_with_native_geometry() {
    let doc = Document::from_native_file(include_bytes!(
        "../../../../tests/fixtures/tunable-pen-lines-67/before.rsk"
    ))
    .unwrap();
    assert_eq!(doc.graphics.len(), 2);
    for graphic in &doc.graphics {
        assert_eq!(
            graphic.commands().iter().filter(|c| matches!(c, PathCommand::Move(_))).count(),
            1
        );
        let point = graphic.edit_points()[0];
        assert!(graphic.hit(point, 2.));
        let (lo, hi) = graphic.bounds();
        assert!(lo.x <= point.x && point.x <= hi.x && lo.y <= point.y && point.y <= hi.y);
    }
    assert_eq!(
        Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
        doc.current()
    );
}
