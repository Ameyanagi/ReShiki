use reshiki::{
    document::{Document, History, Point},
    editing::{self, Transform},
    engine::{ChemistryEngine, LocalEngine, Request},
    graphics::{ArcGeometry, BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand},
};

fn arc(start: f32, sweep: f32) -> Graphic {
    Graphic::dragged(
        1,
        GraphicKind::Arc,
        Point::new(20., 30.),
        Point::new(180., 110.),
        GraphicStyle::default(),
        BracketSides::Both,
        false,
    )
    .with_arc(ArcGeometry {
        start_degrees: start,
        sweep_degrees: sweep,
    })
}

fn close_points(a: impl IntoIterator<Item = Point>, b: impl IntoIterator<Item = Point>) {
    let a: Vec<_> = a.into_iter().collect();
    let b: Vec<_> = b.into_iter().collect();
    assert_eq!(a.len(), b.len());
    for (a, b) in a.into_iter().zip(b) {
        assert!(a.distance(b) < 0.001, "{a:?} != {b:?}");
    }
}

fn points(graphic: &Graphic) -> Vec<Point> {
    graphic
        .commands()
        .iter()
        .flat_map(PathCommand::points)
        .collect()
}

#[test]
fn presets_and_arbitrary_sweeps_share_finite_cubics_bounds_and_hits() {
    for sweep in [0.1, 12.7, 90., 120., 180., 270., 359.9, 360.] {
        for start in [0., 37., 180., 359., -47., 1e30] {
            let g = arc(start, sweep);
            g.validate().unwrap();
            let commands = g.commands();
            assert_eq!(commands.len(), 1 + (sweep / 90.).ceil() as usize);
            assert!(!g.filled());
            let endpoints = g.edit_points();
            assert!(
                matches!(commands.first(), Some(PathCommand::Move(p)) if p.distance(endpoints[0]) < 0.001)
            );
            assert!(
                matches!(commands.last(), Some(PathCommand::Cubic(_,_,p)) if p.distance(endpoints[1]) < 0.001)
            );
            let (lo, hi) = g.bounds();
            for p in reshiki::graphics::flattened(&commands)
                .into_iter()
                .flatten()
            {
                assert!(p.x.is_finite() && p.y.is_finite());
                assert!(p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y);
                assert!(g.hit(p, 0.1));
            }
            if sweep == 360. {
                assert!(endpoints[0].distance(endpoints[1]) < 0.001);
            }
        }
    }
    let g = arc(180., 90.);
    assert!(
        !g.hit(Point::new(180., 70.), 1.),
        "missing quarter must not be selectable"
    );
    for (start, sweep) in [
        (f32::NAN, 90.),
        (0., f32::NAN),
        (0., 0.),
        (0., -90.),
        (0., 361.),
        (f32::INFINITY, 90.),
    ] {
        assert!(arc(start, sweep).validate().is_err());
    }
}

#[test]
fn endpoint_drags_remain_parametric_through_affine_transforms() {
    let mut g = arc(180., 180.);
    let initial = g.edit_points();
    g.edit_point(1, Point::new(100., 110.));
    assert_eq!(g.kind, GraphicKind::Arc);
    assert_eq!(g.arc.unwrap().sweep_degrees, 270.);
    assert!(g.edit_points()[0].distance(initial[0]) < 0.001);
    // The same world-space pointer transformed with the ellipse must choose
    // the same angle even after shear, reflection and nonuniform scaling.
    let transform = |p: Point| Point::new(-2. * p.x + 0.4 * p.y, 0.3 * p.x + p.y + 75.);
    let mut transformed = g.clone();
    transformed.map_positions(transform);
    let pointer = Point::new(100., 30.);
    g.edit_point(0, pointer);
    transformed.edit_point(0, transform(pointer));
    assert!((g.arc.unwrap().start_degrees - transformed.arc.unwrap().start_degrees).abs() < 0.001);
    close_points(points(&g).into_iter().map(transform), points(&transformed));
    let before = transformed.clone();
    transformed.edit_point(2, Point::default());
    assert_eq!(transformed, before);
    transformed.axis_y = transformed.axis_x;
    let before = transformed.clone();
    transformed.edit_point(1, Point::default());
    assert_eq!(
        transformed, before,
        "singular projected frames must not introduce NaN"
    );
}

#[test]
fn full_circle_end_can_be_dragged_open_again_without_losing_the_start() {
    let mut g = arc(180., 180.);
    let start = g.edit_points()[0];
    g.edit_point(1, start);
    assert_eq!(
        g.arc.unwrap().sweep_degrees,
        360.,
        "closing a half arc must create a full circle"
    );
    g.edit_point(1, start);
    assert_eq!(g.arc.unwrap().sweep_degrees, 360.);
    g.edit_point(1, Point::new(100., 110.));
    assert_eq!(g.arc.unwrap().sweep_degrees, 270.);
    assert!(g.edit_points()[0].distance(start) < 0.001);
}

#[test]
fn legacy_half_ellipse_loads_and_upgrades_without_moving_or_losing_depth() {
    let mut g = arc(180., 180.);
    g.arc = None;
    g.depth = [7., 9., 11.];
    let value = serde_json::to_value(&g).unwrap();
    assert_eq!(value["kind"], "arc");
    assert!(value.get("arc").is_none());
    let mut loaded: Graphic = serde_json::from_value(value).unwrap();
    assert_eq!(loaded, g);
    loaded.set_arc(ArcGeometry::default());
    close_points(points(&g), points(&loaded));
    assert_eq!(loaded.depth, [7., 9., 22.]);
    assert_eq!(loaded.arc, Some(ArcGeometry::default()));
}

#[test]
fn native_paths_remain_visible_to_old_readers_and_editable_to_new_readers() {
    for sweep in [90., 120., 180., 270., 360.] {
        let mut g = arc(23., sweep);
        g.map_positions(|p| Point::new(p.y, -p.x));
        let wire = serde_json::to_value(&g).unwrap();
        assert_eq!(wire["kind"], "path");
        assert!(!wire["path"].as_array().unwrap().is_empty());
        let restored: Graphic = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(restored, g);
        // An older reader ignores the new optional property and sees a normal
        // editable path; saving that drawing still retains the exact geometry.
        let mut old_wire = wire;
        old_wire.as_object_mut().unwrap().remove("arc");
        let old: Graphic = serde_json::from_value(old_wire).unwrap();
        assert_eq!(old.kind, GraphicKind::Path);
        close_points(points(&old), points(&g));
        let resaved: Graphic = serde_json::from_str(&serde_json::to_string(&old).unwrap()).unwrap();
        close_points(points(&resaved), points(&g));
    }
}

#[test]
fn transforms_groups_copy_history_and_native_document_roundtrip_keep_arc_parameters() {
    let mut doc = Document::default();
    doc.graphics.push(arc(33., 271.25));
    let id = doc.add_atom("O", Point::new(220., 70.));
    doc.group_selection(&[1, id]).unwrap();
    let selection = doc.expand_groups(&[1]);
    editing::transform(&mut doc, &selection, Transform::Rotate(37.));
    editing::transform(&mut doc, &selection, Transform::FlipHorizontal);
    reshiki::projection::tilt(&mut doc, &selection, 28., true);
    let selected = editing::selection(&doc, &selection);
    let ids = editing::append(&mut doc, &selected, Point::new(270., 0.));
    assert_eq!(doc.graphics[0].arc, doc.graphics[1].arc);
    assert_eq!(doc.graphics[0].depth, doc.graphics[1].depth);
    assert_eq!(doc.groups.len(), 2);
    let before = doc.clone();
    let mut history = History::default();
    doc.delete(&ids);
    history.commit(before.clone(), &doc);
    assert!(history.undo(&mut doc));
    assert_eq!(doc, before);
    doc.validate().unwrap();
    let wire = serde_json::to_string(&doc).unwrap();
    assert_eq!(serde_json::from_str::<Document>(&wire).unwrap(), doc);
}

#[tokio::test]
async fn adjustable_arc_exports_use_same_curves_in_svg_pdf_png_and_cdxml() {
    let engine = LocalEngine::default();
    for sweep in [90., 120., 180., 270., 360.] {
        let mut doc = Document::default();
        doc.graphics.push(arc(21.5, sweep));
        let svg = reshiki::scene::svg(&doc);
        assert!(svg.contains("<path "));
        assert!(
            reshiki::export::drawing(&doc, "pdf")
                .unwrap()
                .starts_with(b"%PDF-")
        );
        assert!(
            reshiki::export::drawing(&doc, "png")
                .unwrap()
                .starts_with(b"\x89PNG")
        );
        let mut request = Request::molecule("export", doc.clone());
        request.format = Some("cdxml".into());
        let xml = engine.execute(request).await.unwrap().output.unwrap();
        let restored = engine
            .execute(Request::import("cdxml", &xml))
            .await
            .unwrap()
            .document
            .unwrap();
        assert_eq!(restored.graphics.len(), 1);
        assert_eq!(restored.graphics[0].kind, GraphicKind::Path);
        let original = points(&doc.graphics[0]);
        let imported = points(&restored.graphics[0]);
        // Import places the drawing near the origin; compare the complete
        // curve after accounting for this document-wide translation.
        let shift = Point::new(imported[0].x - original[0].x, imported[0].y - original[0].y);
        close_points(
            original.into_iter().map(|p| p.offset(shift.x, shift.y)),
            imported,
        );
    }
}
