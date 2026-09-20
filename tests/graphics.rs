use moruno::{
    document::{Document, History, Point},
    editing::{self, Transform},
    engine::{ChemistryEngine, PythonEngine, Request},
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, LinePattern, PathCommand},
};

fn shape(id: u64, kind: GraphicKind) -> Graphic {
    Graphic::dragged(
        id,
        kind,
        Point::new(20., 30.),
        Point::new(160., 120.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    )
}

#[test]
fn shape_hit_testing_and_affine_transforms_retain_geometry() {
    for kind in GraphicKind::DRAWABLE {
        let mut g = shape(1, kind);
        g.validate().unwrap();
        let original = g.commands();
        let rotate = |p: Point| Point::new(-p.y + 100., p.x - 50.);
        g.map_positions(rotate);
        for (expected, actual) in original
            .iter()
            .flat_map(|c| c.map(rotate).points())
            .zip(g.commands().iter().flat_map(PathCommand::points))
        {
            assert!(expected.distance(actual) < 0.001, "{kind}");
        }
    }
    let mut g = shape(1, GraphicKind::Ellipse);
    assert!(!g.hit(Point::new(90., 75.), 2.));
    g.style.fill = Some([220, 239, 233]);
    assert!(g.hit(Point::new(90., 75.), 2.));
    assert!(!g.hit(Point::new(20., 30.), 2.));
    let mut brackets = shape(2, GraphicKind::Brackets);
    assert!(brackets.hit(Point::new(160., 75.), 2.));
    brackets.sides = BracketSides::Left;
    assert!(!brackets.hit(Point::new(160., 75.), 2.));
    assert!(brackets.hit(Point::new(20., 75.), 2.));
    let circle = Graphic::dragged(
        3,
        GraphicKind::Ellipse,
        Point::new(50., 60.),
        Point::new(20., 10.),
        GraphicStyle::default(),
        BracketSides::Both,
        true,
    );
    assert_eq!(circle.axis_x.x, circle.axis_y.y);
}

#[test]
fn graphics_copy_delete_history_and_native_roundtrip_preserve_affine_frames() {
    let mut doc = Document::default();
    doc.graphics.push(shape(1, GraphicKind::RoundedRectangle));
    editing::transform(&mut doc, &[1], Transform::Rotate(30.));
    editing::transform(&mut doc, &[1], Transform::FlipHorizontal);
    let source = editing::selection(&doc, &[1]);
    let ids = editing::append(&mut doc, &source, Point::new(200., 40.));
    assert_eq!(ids, vec![2]);
    assert_eq!(doc.graphics[1].axis_x, doc.graphics[0].axis_x);
    assert_eq!(
        doc.graphics[1].origin,
        doc.graphics[0].origin.offset(200., 40.)
    );
    doc.validate().unwrap();
    let before = doc.clone();
    let mut history = History::default();
    doc.delete(&[1]);
    history.commit(before.clone(), &doc);
    assert_eq!(doc.all_ids(), vec![2]);
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    let serialized = serde_json::to_string(&doc).unwrap();
    assert_eq!(serde_json::from_str::<Document>(&serialized).unwrap(), doc);
    let old: Document =
        serde_json::from_str(include_str!("fixtures/ui-drawn-ethanol.moruno")).unwrap();
    assert!(old.graphics.is_empty());
}

#[test]
fn curve_point_edit_is_local_and_invalid_paths_are_rejected() {
    let mut g = shape(1, GraphicKind::Curve);
    let before = g.commands();
    let replacement = Point::new(80., 10.);
    g.edit_point(1, replacement);
    let points: Vec<_> = g.commands().iter().flat_map(PathCommand::points).collect();
    let original: Vec<_> = before.iter().flat_map(PathCommand::points).collect();
    assert_eq!(points[1], replacement);
    for i in [0, 2, 3] {
        assert_eq!(points[i], original[i]);
    }
    g.validate().unwrap();
    g.path[0] = PathCommand::Line(Point::default());
    assert!(g.validate().is_err());
    g.path[0] = PathCommand::Move(Point::new(f32::NAN, 0.));
    assert!(g.validate().is_err());
}

#[test]
fn all_graphics_render_in_vector_and_raster_exports_with_colors_and_dashes() {
    let mut doc = Document::default();
    for (i, kind) in GraphicKind::DRAWABLE.iter().enumerate() {
        let mut g = shape(i as u64 + 1, *kind);
        g.origin.x += (i % 3) as f32 * 180.;
        g.origin.y += (i / 3) as f32 * 180.;
        g.style.stroke = [32, 80, 145];
        g.style.fill = Some([249, 223, 225]);
        g.style.pattern = if i % 2 == 0 {
            LinePattern::Dashed
        } else {
            LinePattern::Dotted
        };
        doc.graphics.push(g);
    }
    let svg = moruno::scene::svg(&doc);
    assert_eq!(svg.matches("<path ").count(), 9);
    assert!(svg.contains("stroke-dasharray="));
    assert!(svg.contains("rgb(32,80,145)"));
    assert!(svg.contains("rgb(249,223,225)"));
    assert!(
        moruno::export::drawing(&doc, "pdf")
            .unwrap()
            .starts_with(b"%PDF-")
    );
    let png = moruno::export::drawing(&doc, "png").unwrap();
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let pixels = &pixels[..info.buffer_size()];
    assert!(
        pixels
            .chunks_exact(4)
            .filter(|p| p[0] == 249 && p[1] == 223 && p[2] == 225)
            .count()
            > 1000
    );
    assert!(
        pixels
            .chunks_exact(4)
            .filter(|p| p[0] == 32 && p[1] == 80 && p[2] == 145)
            .count()
            > 100
    );
}

#[tokio::test]
async fn every_shape_survives_chemistry_and_cdxml_as_editable_geometry() {
    let engine = PythonEngine::default();
    let mut doc = engine
        .execute(Request::import_smiles("CCO"))
        .await
        .unwrap()
        .document
        .unwrap();
    for (i, kind) in GraphicKind::DRAWABLE.iter().enumerate() {
        let mut g = shape(10 + i as u64, *kind);
        g.origin = g.origin.offset(i as f32 * 200., 200.);
        if kind.closed() {
            g.style.fill = Some([221, 232, 248]);
        }
        g.style.stroke = [32, 80, 145];
        g.style.pattern = LinePattern::Dashed;
        g.layer = if i % 2 == 0 { -1 } else { 1 };
        doc.graphics.push(g);
    }
    for op in ["analyze", "clean"] {
        let result = engine
            .execute(Request::molecule(op, doc.clone()))
            .await
            .unwrap();
        assert_eq!(result.document.unwrap().graphics, doc.graphics);
        assert_eq!(result.analysis.unwrap().smiles, "CCO");
    }
    for graphics_only in [false, true] {
        let mut input = doc.clone();
        if graphics_only {
            input.atoms.clear();
            input.bonds.clear();
        }
        let mut request = Request::molecule("export", input);
        request.format = Some("cdxml".into());
        let xml = engine.execute(request).await.unwrap().output.unwrap();
        let result = engine
            .execute(Request::import("cdxml", &xml))
            .await
            .unwrap();
        if !graphics_only {
            assert_eq!(result.analysis.unwrap().smiles, "CCO");
        }
        let result = result.document.unwrap();
        result.validate().unwrap();
        assert_eq!(result.graphics.len(), 12); // Paired brackets retain two grouped strokes.
        assert_eq!(result.groups.len(), 3);
        let ids: Vec<_> = result.graphics.iter().map(|g| g.id).collect();
        let restored: Vec<_> = editing::groups(&result, &ids)
            .into_iter()
            .map(|ids| {
                let mut graphic = result
                    .graphics
                    .iter()
                    .find(|g| g.id == ids[0])
                    .unwrap()
                    .clone();
                for id in &ids[1..] {
                    graphic.path.extend(
                        result
                            .graphics
                            .iter()
                            .find(|g| g.id == *id)
                            .unwrap()
                            .commands(),
                    );
                }
                graphic
            })
            .collect();
        assert_eq!(restored.len(), 9);
        let mut original = doc.graphics.clone();
        original.sort_by_key(|g| g.layer);
        let shift = Point::new(
            result.graphics[0].bounds().0.x - original[0].bounds().0.x,
            result.graphics[0].bounds().0.y - original[0].bounds().0.y,
        );
        for (old, new) in original.iter().zip(&restored) {
            assert_eq!(new.kind, GraphicKind::Path);
            assert_eq!(old.style.stroke, new.style.stroke);
            assert_eq!(old.filled(), new.filled());
            if old.filled() {
                assert_eq!(old.style.fill, new.style.fill);
            }
            assert_eq!(old.style.pattern, new.style.pattern);
            assert_eq!(old.layer < 0, new.layer < 0);
            for (a, b) in [old.bounds().0, old.bounds().1]
                .iter()
                .zip([new.bounds().0, new.bounds().1])
            {
                assert!(
                    a.offset(shift.x, shift.y).distance(b) < 0.002,
                    "{} {:?} {:?}",
                    old.kind,
                    a,
                    b
                );
            }
        }
    }
}
