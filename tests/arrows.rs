use reshiki::{
    arrows::{ArrowStyle, Head, HeadShape, NoGo, Preset},
    document::{Arrow, Document, Point},
    editing,
    graphics::{LinePattern, PathCommand},
    scene,
};
fn p(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

#[test]
fn repeated_tools_swap_equilibrium_preference_and_dipole_direction() {
    let mut a = arrow(Preset::Equilibrium);
    let style = ArrowStyle {
        equilibrium_ratio: 0.6,
        ..a.appearance()
    };
    a.style = Some(style.clone());
    let (start, end) = (a.start, a.end);
    let original = a.paths();
    assert!(a.apply_tool(Preset::Equilibrium, &style));
    near(a.start, end);
    near(a.end, start);
    assert_eq!(a.appearance().equilibrium_ratio, 0.6);
    let reversed = a.paths();
    let original_long = reshiki::graphics::flattened(&original[0].commands);
    let reversed_long = reshiki::graphics::flattened(&reversed[0].commands);
    assert!(original_long[0][0].y < 0. && reversed_long[0][0].y > 0.);
    assert!(a.apply_tool(Preset::Equilibrium, &style));
    near(a.start, start);
    near(a.end, end);

    assert!(!a.apply_tool(Preset::Dipole, &ArrowStyle::preset(Preset::Dipole)));
    assert!(
        a.paths()
            .iter()
            .any(|p| p.filled && p.style.fill == Some([0; 3]))
    );
    assert!(a.apply_tool(Preset::Dipole, &a.appearance()));
    near(a.start, end);
    near(a.end, start);
    a.validate().unwrap();
}

#[test]
fn half_heads_are_filled_and_repeated_clicks_mirror_only_the_head() {
    for preset in [Preset::Forward, Preset::Fishhook] {
        let mut a = arrow(preset);
        a.style.as_mut().unwrap().head = Head::Left;
        let original = a.clone();
        assert!(a.paths()[1].filled);
        assert_eq!(a.paths()[1].style.fill, Some([0; 3]));
        assert!(!a.apply_tool(preset, &a.appearance()));
        assert_eq!(a.appearance().head, Head::Right);
        near(a.point(0.5), original.point(0.5));
        near(a.start, original.start);
        near(a.end, original.end);
        assert!(!a.apply_tool(preset, &a.appearance()));
        assert_eq!(a, original);
    }
}

#[test]
fn curved_equilibrium_shafts_keep_a_normal_gap_and_heads_meet_their_tips() {
    for height in [-180., -60., 60., 180.] {
        for ratio in [1., 0.6] {
            let mut a = arrow(Preset::Equilibrium);
            a.control = Some(p(60., height));
            a.style.as_mut().unwrap().equilibrium_ratio = ratio;
            let gap = reshiki::style::DEFAULT.world(a.appearance().gap_pt) * 0.5;
            let paths = a.paths();
            let inset = (1. - ratio) / 2.;
            for (path, from, to, offset) in [
                (&paths[0], 0., 1., -gap),
                (&paths[2], 1. - inset, inset, gap),
            ] {
                for (i, command) in path.commands.iter().enumerate() {
                    let t = from + (to - from) * i as f32 / 16.;
                    let center = a.point(t);
                    let v = p(120., 2. * height * (1. - 2. * t));
                    let speed = v.distance(p(0., 0.));
                    let expected = center.offset(-v.y / speed * offset, v.x / speed * offset);
                    near(*command.points().last().unwrap(), expected);
                }
            }
            for (shaft, head) in [(&paths[0], &paths[1]), (&paths[2], &paths[3])] {
                let tip = *head.commands.last().unwrap().points().last().unwrap();
                near(
                    *shaft.commands.last().unwrap().points().last().unwrap(),
                    tip,
                );
            }
        }
    }
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
            for p in reshiki::graphics::flattened(&path.commands)
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
        reshiki::selection_region::objects(&doc, &outline),
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
        reshiki::style::DEFAULT.line_width_pt
    );
}

#[tokio::test]
async fn arrow_styles_and_controls_survive_chemistry_and_cdxml_round_trip() {
    use reshiki::engine::{LocalEngine, Request};
    let engine = LocalEngine::default();
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

#[test]
fn filled_arrowheads_cover_the_shaft_cap_and_keep_a_visible_notch() {
    let a = arrow(Preset::Forward);
    let paths = a.paths();
    let PathCommand::Line(end) = paths[0].commands.last().unwrap() else {
        panic!("straight shaft")
    };
    let half_stroke = reshiki::style::DEFAULT.world(a.appearance().width_pt) / 2.;
    assert!(end.x + half_stroke < a.end.x);
    let style = a.appearance();
    assert!(style.head_notch > 0.);
    assert!(style.head_length_pt / style.head_width_pt >= 3.);
    assert!(paths[1].filled);
}

#[test]
fn double_shafts_join_the_retrosynthesis_head_at_its_actual_width() {
    let a = arrow(Preset::Retro);
    let paths = a.paths();
    let style = a.appearance();
    let length = reshiki::style::DEFAULT.world(style.head_length_pt);
    let width = reshiki::style::DEFAULT.world(style.head_width_pt);
    for shaft in &paths[..2] {
        let PathCommand::Line(end) = shaft.commands.last().unwrap() else {
            panic!("straight shaft")
        };
        let expected_inset = length * end.y.abs() / width;
        assert!((a.end.x - end.x - expected_inset).abs() < 0.001);
    }
}
