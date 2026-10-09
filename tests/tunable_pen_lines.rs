use reshiki::{
    document::{Document, Point},
    engine::{LocalEngine, Request},
    graphics::{LinePattern, PathCommand},
};

fn fixture() -> Document {
    Document::from_native_file(include_bytes!("fixtures/tunable-pen-lines-67/before.rsk")).unwrap()
}
fn sampled(graphic: &reshiki::graphics::Graphic) -> Vec<Point> {
    reshiki::graphics::flattened(&graphic.commands())
        .into_iter()
        .flatten()
        .collect()
}

#[tokio::test]
async fn pen_multisegment_and_closed_edits_survive_editable_exchange_and_exports() {
    let mut doc = fixture();
    doc.graphics[0].edit_point(3, Point::new(110., 15.));
    doc.graphics[0].insert_path_node(0).unwrap();
    doc.graphics[0]
        .append_pen_node(Point::new(340., 60.), Some(Point::new(350., 110.)))
        .unwrap();
    doc.graphics[1].delete_path_node(6).unwrap();
    for graphic in &mut doc.graphics {
        graphic.style.stroke = reshiki::palette::Color::Custom([32, 80, 145]);
        graphic.style.pattern = LinePattern::Dashed;
        graphic.layer = 1;
    }
    // Annotations are unrelated to the editable path exchange under test.
    doc.annotations.clear();
    assert_eq!(
        Document::from_native_file(&doc.file_json().unwrap()).unwrap(),
        doc.current()
    );
    let engine = LocalEngine::default();
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some(format.into());
        let output = engine.request(request).await.unwrap().output.unwrap();
        let restored = engine
            .request(Request::import(format, &output))
            .await
            .unwrap()
            .document
            .unwrap();
        assert_eq!(restored.graphics.len(), 2);
        let origin = doc.graphics[0].edit_points()[0];
        let restored_origin = restored.graphics[0].edit_points()[0];
        for (original, restored) in doc.graphics.iter().zip(&restored.graphics) {
            assert!(restored.path_handles().is_some());
            assert_eq!(original.path_closed(), restored.path_closed());
            assert_eq!(original.style.stroke, restored.style.stroke);
            assert_eq!(original.style.pattern, restored.style.pattern);
            assert!((original.style.width_pt - restored.style.width_pt).abs() < 1. / 65536.);
            let (a, b) = (sampled(original), sampled(restored));
            assert_eq!(a.len(), b.len());
            for (a, b) in a.into_iter().zip(b) {
                assert!(
                    a.offset(-origin.x, -origin.y)
                        .distance(b.offset(-restored_origin.x, -restored_origin.y))
                        < 0.003
                );
            }
        }
        // External stacking ordinals are normalized on import; order is the
        // supported appearance guarantee for this graphics-only drawing.
        assert!(restored.graphics[0].layer < restored.graphics[1].layer);
    }
    for format in ["svg", "pdf", "png"] {
        assert!(!reshiki::export::drawing(&doc, format).unwrap().is_empty());
    }
    doc.graphics[0].style.pattern = LinePattern::Dotted;
    let mut request = Request::molecule("export", doc.clone());
    request.format = Some("cdxml".into());
    assert!(
        engine
            .request(request)
            .await
            .unwrap_err()
            .contains("Dotted")
    );
    assert!(!reshiki::export::drawing(&doc, "svg").unwrap().is_empty());
}

#[test]
fn compound_imported_paths_keep_generic_controls_without_node_operations() {
    let mut graphic = fixture().graphics.remove(0);
    let first = graphic.commands();
    graphic.path.extend([
        PathCommand::Move(Point::new(400., 0.)),
        PathCommand::Line(Point::new(440., 20.)),
    ]);
    assert!(graphic.path_handles().is_none());
    assert!(graphic.insert_path_node(0).is_err());
    assert!(graphic.delete_path_node(0).is_err());
    graphic.edit_point(1, Point::new(0., -45.));
    assert_eq!(graphic.edit_points()[1], Point::new(0., -45.));
    assert_eq!(graphic.commands()[0], first[0]);
    graphic.validate().unwrap();
}
