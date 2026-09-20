use moruno::{
    arrows::{ArrowStyle, Head, HeadShape, NoGo, Preset},
    document::{Arrow, Document, Point},
    editing,
    graphics::{LinePattern, PathCommand},
    scene,
};
fn p(x: f32, y: f32) -> Point {
    Point::new(x, y)
}
fn arrow(preset: Preset) -> Arrow {
    Arrow::new(
        1,
        p(0., 0.),
        p(120., 0.),
        preset,
        ArrowStyle::preset(preset),
    )
}
fn near(a: Point, b: Point) {
    assert!(a.distance(b) < 0.001, "{a:?} != {b:?}");
}

#[test]
fn middle_handle_bends_on_the_curve_and_transform_copy_keep_shape() {
    let mut a = arrow(Preset::Forward);
    a.edit_handle(2, p(60., -40.));
    near(a.point(0.5), p(60., -40.));
    assert!(a.hit(p(60., -40.), 1.));
    assert!(!a.hit(p(60., 0.), 5.));
    let mut doc = Document::default();
    doc.arrows.push(a.clone());
    let ids = editing::append(
        &mut doc,
        &Document {
            arrows: vec![a.clone()],
            ..Document::default()
        },
        p(160., 10.),
    );
    near(doc.arrows[1].point(0.5), p(220., -30.));
    editing::transform_about(&mut doc, &ids, p(160., 10.), 2., 90.);
    near(doc.arrows[1].point(0.5), p(240., 130.));
    doc.translate(&[1], 20., 30.);
    near(doc.arrows[0].point(0.5), p(80., -10.));
    let mut a = doc.arrows[0].clone();
    let midpoint = a.point(0.5);
    a.reverse();
    near(a.point(0.5), midpoint);
    a.flip_bend();
    near(a.point(0.5), p(80., 70.));
    a.straighten();
    near(a.point(0.5), p(80., 30.));
    doc.validate().unwrap();
}

#[test]
fn all_arrow_parts_participate_in_bounds_selection_and_exports() {
    let mut doc = Document::default();
    for (i, preset) in Preset::ALL.iter().copied().enumerate() {
        let mut a = arrow(preset);
        a.id = i as u64 + 1;
        a.map_points(|p| p.offset(0., i as f32 * 90.));
        doc.arrows.push(a);
    }
    doc.arrows[3].style.as_mut().unwrap().pattern = LinePattern::Dashed;
    doc.arrows[0].style.as_mut().unwrap().color = [32, 80, 145];
    let svg = scene::svg(&doc);
    assert!(svg.contains("rgb(32,80,145)"));
    assert!(svg.contains("stroke-dasharray"));
    assert!(svg.contains("C"));
    for a in &doc.arrows {
        let bounds = a.bounds();
        assert_eq!(scene::selection_bounds(&doc, &[a.id]), Some(bounds));
        for path in a.paths() {
            for p in moruno::graphics::flattened(&path.commands)
                .into_iter()
                .flatten()
            {
                assert!(
                    p.x >= bounds.0.x - 0.001
                        && p.x <= bounds.1.x + 0.001
                        && p.y >= bounds.0.y - 0.001
                        && p.y <= bounds.1.y + 0.001
                );
            }
        }
    }
    let outline = [p(-30., -20.), p(160., -20.), p(160., 750.), p(-30., 750.)];
    assert_eq!(
        moruno::selection_region::objects(&doc, &outline),
        doc.all_ids()
    );
    doc.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<Document>(&serde_json::to_string(&doc).unwrap()).unwrap(),
        doc
    );
}

#[test]
fn hollow_heads_leave_open_centers_and_unequal_equilibrium_keeps_the_long_shaft() {
    let mut a = arrow(Preset::Forward);
    let s = a.style.as_mut().unwrap();
    s.shape = HeadShape::Hollow;
    s.tail = Head::Full;
    s.no_go = NoGo::Hash;
    let paths = a.paths();
    assert!(!paths.iter().any(|p| p.filled));
    let start = paths[0].commands[0].points()[0];
    let end = paths[0].commands[1].points()[0];
    assert!(start.x > 0. && end.x < 120.);
    let mut a = arrow(Preset::Equilibrium);
    a.style.as_mut().unwrap().equilibrium_ratio = 0.5;
    let paths = a.paths();
    let second = &paths[2].commands;
    assert!(matches!(second[0],PathCommand::Move(q) if (q.x-90.).abs()<0.001));
    assert!(matches!(second[1],PathCommand::Line(q) if (q.x-30.).abs()<0.001));
}

#[test]
fn legacy_curves_materialize_before_reflection_and_invalid_styles_fail() {
    let mut a: Arrow = serde_json::from_str(
        r#"{"id":1,"start":{"x":0,"y":0},"end":{"x":120,"y":0},"kind":"curved"}"#,
    )
    .unwrap();
    near(a.point(0.5), p(60., 30.));
    a.map_points(|q| p(q.x, -q.y));
    near(a.point(0.5), p(60., -30.));
    a.style = Some(ArrowStyle {
        head_length_pt: f32::NAN,
        ..Default::default()
    });
    assert!(a.validate().is_err());
    a.style = None;
    a.control = Some(p(f32::INFINITY, 0.));
    assert!(a.validate().is_err());
    assert_eq!(
        ArrowStyle::default().width_pt,
        moruno::style::DEFAULT.line_width_pt
    );
}

#[tokio::test]
async fn arrow_styles_and_controls_survive_chemistry_and_cdxml_round_trip() {
    use moruno::engine::{PythonEngine, Request};
    let engine = PythonEngine::default();
    let mut doc = engine
        .request(Request::import_smiles("CCO"))
        .await
        .unwrap()
        .document
        .unwrap();
    let mut a = arrow(Preset::Fishhook);
    a.id = doc.next_id();
    a.edit_handle(2, p(60., -35.));
    a.style.as_mut().unwrap().color = [32, 80, 145];
    doc.arrows.push(a.clone());
    let clean = engine
        .request(Request::molecule("clean", doc.clone()))
        .await
        .unwrap()
        .document
        .unwrap();
    assert_eq!(clean.arrows, doc.arrows);
    let mut request = Request::molecule("export", doc);
    request.format = Some("cdxml".into());
    let xml = engine.request(request).await.unwrap().output.unwrap();
    assert!(xml.contains("<curve"));
    let imported = engine
        .request(Request::import("cdxml", &xml))
        .await
        .unwrap()
        .document
        .unwrap();
    assert_eq!(imported.arrows[0].appearance(), a.appearance());
    // CDXML translates the document; compare relative curve geometry.
    let b = &imported.arrows[0];
    near(p(b.end.x - b.start.x, b.end.y - b.start.y), p(120., 0.));
    near(
        p(b.point(0.5).x - b.start.x, b.point(0.5).y - b.start.y),
        p(60., -35.),
    );
}

#[test]
fn elbow_has_two_straight_segments_and_editable_corner() {
    let mut a = arrow(Preset::Bent);
    a.edit_handle(2, p(30., -50.));
    near(a.handles()[2], p(30., -50.));
    assert!(a.hit(p(15., -25.), 0.1));
    assert!(!a.hit(p(60., 0.), 1.));
    assert!(
        a.paths()
            .iter()
            .flat_map(|p| &p.commands)
            .all(|c| !matches!(c, PathCommand::Cubic(..)))
    );
    let mid = a.handles()[2];
    a.reverse();
    near(a.handles()[2], mid);
    a.flip_bend();
    near(a.handles()[2], p(30., 50.));
    a.straighten();
    near(a.handles()[2], p(60., 0.));
}
