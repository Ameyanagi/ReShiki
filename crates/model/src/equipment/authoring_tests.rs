use super::*;
use crate::{graphics::GraphicStyle, palette::Color};

fn rectangle(doc: &mut Document, lo: Point, hi: Point, filled: bool) -> u64 {
    let id = doc.next_id();
    let style = GraphicStyle {
        fill: filled.then_some(Color::Paper),
        ..Default::default()
    };
    doc.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        lo,
        hi,
        style,
        Default::default(),
        false,
    ));
    id
}

fn vessel() -> (Document, u64, [Point; 2]) {
    let mut doc = Document::default();
    let id = rectangle(&mut doc, Point::new(-10., 0.), Point::new(10., 50.), true);
    (doc, id, [Point::new(-10., 0.), Point::new(10., 0.)])
}

fn plug(size_mm: f64) -> JointKind {
    JointKind::Plug {
        family: PlugFamily::Stopper,
        gender: Gender::F,
        size_mm,
    }
}

fn close(a: Xy, b: Xy) {
    assert!(geom::len(geom::sub(a, b)) < 1e-7, "{a:?} != {b:?}");
}

fn world_paths(draft: &Draft) -> Vec<Vec<Seg>> {
    draft
        .definition
        .shapes
        .iter()
        .map(|s| draft.frame().map(&geom::parse_path(&s.path).unwrap()))
        .collect()
}

fn same_paths(a: &[Vec<Seg>], b: &[Vec<Seg>]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            match (a, b) {
                (Seg::Move(a), Seg::Move(b)) | (Seg::Line(a), Seg::Line(b)) => close(*a, *b),
                (Seg::Cubic(a, b, c), Seg::Cubic(x, y, z)) => {
                    close(*a, *x);
                    close(*b, *y);
                    close(*c, *z);
                }
                (Seg::Close, Seg::Close) => {}
                _ => panic!("Commands changed: {a:?} != {b:?}"),
            }
        }
    }
}

#[test]
fn selection_conversion_is_deterministic_and_keeps_world_curves() {
    let (mut doc, body, _) = vessel();
    let curve = doc.next_id();
    let mut g = Graphic::dragged(
        curve,
        GraphicKind::Curve,
        Point::new(40., 15.),
        Point::new(100., 35.),
        Default::default(),
        Default::default(),
        false,
    );
    g.axis_y = Point::new(-13., 42.);
    doc.graphics.push(g);
    let original = doc.clone();
    let draft = Draft::from_selection(&doc, &[curve, body, curve]).unwrap();
    assert_eq!(draft, Draft::from_selection(&doc, &[body, curve]).unwrap());
    assert_eq!(doc, original);
    assert_eq!(draft.definition.format_version, PREVIEW_VERSION);
    assert_eq!(draft.definition.shapes[0].role, Role::Glass);
    assert_eq!(draft.definition.shapes[0].material, Material::Glass);
    assert_eq!(draft.definition.shapes[1].role, Role::Outline);
    assert_eq!(draft.definition.shapes[1].material, Material::Ink);
    assert!(
        draft
            .definition
            .shapes
            .iter()
            .all(|s| s.layer == Layer::Body)
    );
    let expected: Vec<_> = doc
        .graphics
        .iter()
        .map(|g| commands(&g.commands()))
        .collect();
    same_paths(&world_paths(&draft), &expected);
    assert!(!draft.preview().unwrap().is_empty());
    assert!(
        draft
            .finish("lab/vessel")
            .unwrap_err()
            .contains("set the scale")
    );
}

#[test]
fn first_confirmed_joint_calibrates_every_shape_and_keeps_the_source() {
    let (mut doc, body, edges) = vessel();
    let line = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        line,
        GraphicKind::Line,
        Point::new(-5., 20.),
        Point::new(5., 20.),
        Default::default(),
        Default::default(),
        false,
    ));
    let original = doc.clone();
    let mut draft = Draft::from_selection(&doc, &[body, line]).unwrap();
    let before = world_paths(&draft);
    assert!(draft.taper_sizes(edges, Gender::F).is_err());
    let joint = draft.add_joint("top", edges, plug(10.), false).unwrap();
    assert!(draft.calibrated());
    close(draft.frame().apply(joint.at()), [0., 0.]);
    close(draft.frame().apply_dir(joint.dir()), [0., -1.]);
    assert!((draft.rim(edges).unwrap().gap_mm - 10.).abs() < 1e-8);
    same_paths(&world_paths(&draft), &before);
    let ghost = draft.preview_graphic(777).unwrap();
    let ports = super::super::solve::ports(&ghost);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].at.piece, 777);
    close(ports[0].point, [0., 0.]);
    close(ports[0].dir, [0., -1.]);
    validate::validate(&draft.finish("lab/vessel").unwrap()).unwrap();
    assert_eq!(doc, original);
}

#[test]
fn rim_normal_is_outward_in_both_click_orders_and_in_a_rotated_frame() {
    let (doc, body, edges) = vessel();
    let mut draft = Draft::from_selection(&doc, &[body]).unwrap();
    draft.calibrate(edges, 10.).unwrap();
    let calibrated = draft.clone();
    draft.calibrate(edges, 10.).unwrap();
    assert_eq!(draft, calibrated);
    for edges in [edges, [edges[1], edges[0]]] {
        let rim = draft.rim(edges).unwrap();
        assert!(!rim.ambiguous);
        close(draft.frame().apply(rim.at), [0., 0.]);
        close(draft.frame().apply_dir(rim.dir), [0., -1.]);
    }
    let rotate = Affine::rotate(37., [12., -8.]);
    draft.frame = rotate.then(draft.frame);
    let edges = edges.map(|p| {
        let [x, y] = rotate.apply([f64::from(p.x), f64::from(p.y)]);
        Point::new(x as f32, y as f32)
    });
    let rim = draft.rim(edges).unwrap();
    assert!(!rim.ambiguous);
    let expected = rotate.apply_dir([0., -1.]);
    let actual = draft.frame().apply_dir(rim.dir);
    assert!(geom::len(geom::sub(actual, expected)) < 1e-6);
}

#[test]
fn joint_errors_are_transactional_and_later_openings_do_not_reset_scale() {
    let (doc, body, edges) = vessel();
    let mut draft = Draft::from_selection(&doc, &[body]).unwrap();
    let before = draft.clone();
    assert!(
        draft
            .add_joint("top", edges, plug(10.), true)
            .unwrap_err()
            .contains("points into")
    );
    assert_eq!(draft, before);
    for bad in [[edges[0], edges[0]], [Point::new(f32::NAN, 0.), edges[1]]] {
        assert!(draft.add_joint("top", bad, plug(10.), false).is_err());
        assert_eq!(draft, before);
    }
    draft.add_joint("top", edges, plug(10.), false).unwrap();
    let before = draft.clone();
    let bottom = [Point::new(-10., 50.), Point::new(10., 50.)];
    assert!(
        draft
            .add_joint("bottom", bottom, plug(20.), false)
            .unwrap_err()
            .contains("opening is")
    );
    assert!(
        draft
            .add_joint("top", bottom, plug(10.), false)
            .unwrap_err()
            .contains("Two joints")
    );
    assert!(
        draft
            .add_joint(
                "far",
                [Point::new(-10., -20.), Point::new(10., -20.)],
                plug(10.),
                false
            )
            .unwrap_err()
            .contains("from the outline")
    );
    assert_eq!(draft, before);
    draft.add_joint("bottom", bottom, plug(10.), false).unwrap();
    assert_eq!(draft.frame(), before.frame());
    assert!(
        draft
            .calibrate(edges, 20.)
            .unwrap_err()
            .contains("before adding")
    );
}

#[test]
fn open_strokes_expose_ambiguous_rims_and_body_interiors_are_refused() {
    let mut doc = Document::default();
    let id = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        id,
        GraphicKind::Line,
        Point::new(-10., 0.),
        Point::new(10., 0.),
        Default::default(),
        Default::default(),
        false,
    ));
    let mut draft = Draft::from_selection(&doc, &[id]).unwrap();
    let edges = [Point::new(-10., 0.), Point::new(10., 0.)];
    draft.calibrate(edges, 10.).unwrap();
    assert!(draft.rim(edges).unwrap().ambiguous);
    let (doc, id, _) = vessel();
    let draft = Draft::from_selection(&doc, &[id]).unwrap();
    assert!(
        draft
            .rim([Point::new(-5., 25.), Point::new(5., 25.)])
            .unwrap_err()
            .contains("Both directions")
    );
}

#[test]
fn standard_taper_is_a_real_validated_joint_at_the_clicked_mouth() {
    let mut doc = Document::default();
    let id = doc.next_id();
    let mut g = Graphic::pen_curve(
        id,
        Point::default(),
        Point::new(1., 1.),
        GraphicStyle {
            fill: Some(Color::Paper),
            ..Default::default()
        },
    );
    g.reset_frame();
    g.node_types.clear();
    g.path = vec![
        PathCommand::Move(Point::new(-9.6, 0.)),
        PathCommand::Line(Point::new(9.6, 0.)),
        PathCommand::Line(Point::new(6.5, 29.)),
        PathCommand::Line(Point::new(25., 45.)),
        PathCommand::Line(Point::new(-25., 45.)),
        PathCommand::Line(Point::new(-6.5, 29.)),
        PathCommand::Close,
    ];
    doc.graphics.push(g);
    let mut draft = Draft::from_selection(&doc, &[id]).unwrap();
    let edges = [Point::new(-9.6, 0.), Point::new(9.6, 0.)];
    let joint = draft
        .add_joint(
            "mouth",
            edges,
            JointKind::Taper {
                size: "15/25".into(),
                gender: Gender::F,
                draw: JointDraw::Standard,
            },
            false,
        )
        .unwrap();
    assert!(matches!(
        joint,
        Joint::Taper {
            draw: JointDraw::Standard,
            ..
        }
    ));
    close(draft.frame().apply(joint.at()), [0., 0.]);
    validate::validate(&draft.definition).unwrap();
    assert_eq!(
        draft.taper_sizes(edges, Gender::F).unwrap()[0].label,
        "15/25"
    );
    assert!(draft.preview().unwrap().len() > 2);
}

#[test]
fn accept_replaces_sources_only_after_confirmation_and_round_trips() {
    let (mut doc, body, edges) = vessel();
    let mut draft = Draft::from_selection(&doc, &[body]).unwrap();
    draft.add_joint("top", edges, plug(10.), false).unwrap();
    draft.definition.names.insert("ja".into(), "容器".into());
    let unrelated = doc.add_atom("N", Point::new(500., 0.));
    let original = doc.clone();
    let accepted = draft.accept(&doc, "lab/vessel").unwrap();
    assert_eq!(doc, original);
    assert!(accepted.document.graphics.iter().all(|g| g.id != body));
    assert_eq!(accepted.document.graphics.len(), 1);
    assert_eq!(accepted.document.graphics[0].kind, GraphicKind::Piece);
    assert_eq!(accepted.document.atom(unrelated), doc.atom(unrelated));
    assert_eq!(accepted.definition.revision, 1);
    assert_eq!(accepted.definition.format_version, PREVIEW_VERSION);
    assert_eq!(accepted.definition.names["ja"], "容器");
    let bytes = accepted.document.file_json().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["version"], crate::document::DRAWING_ELEMENTS);
    let back = Document::from_json(&bytes).unwrap();
    let mut saved = accepted.document.clone();
    saved.version = crate::document::DRAWING_ELEMENTS;
    assert_eq!(back, saved);
    let mut changed = doc.clone();
    changed.graphics[0].origin.x += 1.;
    assert!(
        draft
            .accept(&changed, "lab/vessel")
            .unwrap_err()
            .contains("source shapes changed")
    );
}

#[test]
fn user_revision_and_builtin_copy_leave_other_placed_copies_unchanged() {
    let built_in = super::super::builtin::find("reshiki/rbf-100-15").unwrap();
    let mut user = (*built_in).clone();
    user.id = "lab/flask".into();
    user.revision = 7;
    let user = Arc::new(user);
    let mut doc = Document::default();
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    for at in [Point::default(), Point::new(200., 0.)] {
        let id = doc.next_id();
        doc.graphics
            .push(placed::graphic(id, user.clone(), at, scale));
    }
    let original = doc.clone();
    let mut draft = Draft::from_piece(&doc, doc.graphics[0].id).unwrap();
    draft
        .definition
        .names
        .insert("en".into(), "Edited flask".into());
    assert_eq!(draft.finish("lab/flask").unwrap().revision, 8);
    assert_eq!(draft.finish("lab/flask").unwrap().revision, 8);
    assert_eq!(draft.finish("lab/new-flask").unwrap().revision, 1);
    let accepted = draft.accept(&doc, "lab/flask").unwrap();
    assert_eq!(doc, original);
    assert_eq!(accepted.document.graphics[0].id, doc.graphics[0].id);
    assert_eq!(accepted.document.graphics[0].origin, doc.graphics[0].origin);
    assert_eq!(accepted.document.graphics[1], doc.graphics[1]);
    assert_eq!(accepted.definition.revision, 8);
    let id = doc.next_id();
    doc.graphics.push(placed::graphic(
        id,
        built_in.clone(),
        Point::new(400., 0.),
        scale,
    ));
    let draft = Draft::from_piece(&doc, id).unwrap();
    assert!(
        draft
            .finish(&built_in.id)
            .unwrap_err()
            .contains("new piece")
    );
    assert_eq!(draft.finish("lab/my-flask").unwrap().revision, 1);
    assert_eq!(built_in.revision, 1);
}

#[test]
fn unsupported_and_mixed_selections_are_refused_without_changes() {
    let (mut doc, body, _) = vessel();
    let atom = doc.add_atom("O", Point::new(100., 100.));
    let original = doc.clone();
    assert!(Draft::from_selection(&doc, &[]).is_err());
    assert!(Draft::from_selection(&doc, &[u64::MAX]).is_err());
    assert!(Draft::from_selection(&doc, &[body, atom]).is_err());
    for change in [0, 1, 2, 3] {
        let mut unsupported = doc.clone();
        match change {
            0 => unsupported.graphics[0].kind = GraphicKind::Picture,
            1 => unsupported.graphics[0].style.pattern = LinePattern::Dashed,
            2 => unsupported.graphics[0].depth[0] = 1.,
            _ => unsupported.graphics[0].text = Some(atom),
        }
        assert!(Draft::from_selection(&unsupported, &[body]).is_err());
    }
    assert_eq!(doc, original);
}

#[test]
fn selection_expands_native_groups_and_respects_authoring_limits() {
    let (mut doc, body, _) = vessel();
    let other = rectangle(&mut doc, Point::new(30., 0.), Point::new(40., 10.), false);
    doc.group_selection(&[body, other]).unwrap();
    let draft = Draft::from_selection(&doc, &[body]).unwrap();
    assert_eq!(draft.definition.shapes.len(), 2);
    assert_eq!(draft, Draft::from_selection(&doc, &[other, body]).unwrap());
    doc.graphics[1].layer += 1;
    assert!(
        Draft::from_selection(&doc, &[body])
            .unwrap_err()
            .contains("same drawing layer")
    );

    let mut doc = Document::default();
    for n in 0..=validate::MAX_SHAPES {
        rectangle(
            &mut doc,
            Point::new(n as f32, 0.),
            Point::new(n as f32 + 1., 1.),
            false,
        );
    }
    assert!(
        Draft::from_selection(&doc, &doc.all_ids())
            .unwrap_err()
            .contains("at most")
    );
    let mut doc = Document::default();
    for _ in 0..2 {
        let id = doc.next_id();
        let mut g =
            Graphic::pen_curve(id, Point::default(), Point::new(1., 1.), Default::default());
        g.node_types.clear();
        g.path = std::iter::once(PathCommand::Move(Point::default()))
            .chain((1..=10_000).map(|n| PathCommand::Line(Point::new(n as f32 / 1000., 0.))))
            .collect();
        doc.graphics.push(g);
    }
    assert!(
        Draft::from_selection(&doc, &doc.all_ids())
            .unwrap_err()
            .contains("path commands")
    );
}

#[test]
fn invalid_calibration_is_transactional() {
    let (doc, body, edges) = vessel();
    let mut draft = Draft::from_selection(&doc, &[body]).unwrap();
    let before = draft.clone();
    for width in [0., f64::NAN, f64::INFINITY, 1e-6, 1e6] {
        assert!(draft.calibrate(edges, width).is_err());
        assert_eq!(draft, before);
    }
}

#[test]
fn an_incompatible_standard_socket_never_replaces_the_drawn_end() {
    let (doc, body, edges) = vessel();
    let original = doc.clone();
    let mut draft = Draft::from_selection(&doc, &[body]).unwrap();
    let before = draft.clone();
    for draw in [JointDraw::Standard, JointDraw::None] {
        let error = draft
            .add_joint(
                "top",
                edges,
                JointKind::Taper {
                    size: "15/25".into(),
                    gender: Gender::F,
                    draw,
                },
                false,
            )
            .unwrap_err();
        assert!(
            error.contains(if draw == JointDraw::Standard {
                "from the outline"
            } else {
                "must draw it"
            }),
            "{error}"
        );
        assert_eq!(draft, before);
    }
    assert_eq!(doc, original);
}

#[test]
fn replacing_sources_keeps_object_callouts_and_chemistry_targets() {
    use crate::leaders::{self, Target};

    let (mut doc, body, edges) = vessel();
    let other = rectangle(&mut doc, Point::new(30., 0.), Point::new(40., 10.), false);
    let a = doc.add_atom("C", Point::new(400., 0.));
    let b = doc.add_atom("O", Point::new(440., 0.));
    doc.add_bond(a, b, 1, "plain");
    let targets = [
        Target::Object { id: body },
        Target::Object { id: other },
        Target::Atom { atom: a },
        Target::Molecule { atom: a },
        Target::Bond {
            a,
            b,
            fraction: 0.5,
        },
    ];
    let cards: Vec<_> = targets
        .iter()
        .map(|target| leaders::add_callout(&mut doc, target.clone(), "Keep this link").unwrap())
        .collect();
    let original = doc.clone();
    let mut draft = Draft::from_selection(&doc, &[body, other]).unwrap();
    draft.calibrate(edges, 10.).unwrap();
    let accepted = draft.accept(&doc, "lab/linked-vessel").unwrap();
    assert_eq!(doc, original);
    let piece_id = accepted.piece_id;
    let mut result = accepted.document;
    let _ = crate::transaction::reconcile(&mut result, original).unwrap();
    for (card, target) in cards.into_iter().zip(targets) {
        let line = result
            .graphics
            .iter()
            .find_map(|g| {
                g.leader
                    .as_ref()
                    .filter(|leader| leader.start == Some(Target::Object { id: card }))
            })
            .unwrap();
        let expected = match target {
            Target::Object { .. } => Target::Object { id: piece_id },
            other => other,
        };
        assert_eq!(line.end, Some(expected));
    }
}

#[test]
fn replacing_sources_keeps_groups_added_while_the_draft_is_open() {
    let (mut doc, body, edges) = vessel();
    let other = rectangle(&mut doc, Point::new(30., 0.), Point::new(40., 10.), false);
    doc.group_selection(&[body, other]).unwrap();
    let mut draft = Draft::from_selection(&doc, &[body]).unwrap();
    draft.calibrate(edges, 10.).unwrap();

    let atom = doc.add_atom("N", Point::new(400., 0.));
    doc.group_selection(&[body, atom]).unwrap();
    let outer = doc.groups.last_mut().unwrap();
    outer.integral = true;
    let group_id = outer.id;
    let original = doc.clone();
    let accepted = draft.accept(&doc, "lab/grouped-vessel").unwrap();
    assert_eq!(doc, original);
    let group = accepted
        .document
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .expect("the replacement keeps its group with the unrelated atom");
    assert!(group.integral);
    let mut expected = vec![atom, accepted.piece_id];
    expected.sort_unstable();
    assert_eq!(group.members, expected);
    accepted.document.validate().unwrap();
}

#[test]
fn a_plug_can_join_the_rim_of_an_open_native_outline() {
    let mut doc = Document::default();
    for (from, to) in [
        (Point::new(-10., 0.), Point::new(-10., 50.)),
        (Point::new(10., 0.), Point::new(10., 50.)),
        (Point::new(-10., 50.), Point::new(10., 50.)),
    ] {
        let id = doc.next_id();
        doc.graphics.push(Graphic::dragged(
            id,
            GraphicKind::Line,
            from,
            to,
            Default::default(),
            Default::default(),
            false,
        ));
    }
    let original = doc.clone();
    let edges = [Point::new(-10., 0.), Point::new(10., 0.)];
    let mut draft = Draft::from_selection(&doc, &doc.all_ids()).unwrap();
    draft.calibrate(edges, 10.).unwrap();
    assert!(draft.rim(edges).unwrap().ambiguous);

    let mut one_wall = draft.clone();
    one_wall.definition.shapes.truncate(1);
    let before = one_wall.clone();
    assert!(one_wall.add_joint("mouth", edges, plug(10.), true).is_err());
    assert_eq!(one_wall, before, "both rim edges are required");

    draft.add_joint("mouth", edges, plug(10.), true).unwrap();
    let ghost = draft.preview_graphic(777).unwrap();
    let ports = super::super::solve::ports(&ghost);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].at.joint, "mouth");
    close(ports[0].point, [0., 0.]);
    close(ports[0].dir, [0., -1.]);
    validate::validate(&draft.finish("lab/open-vessel").unwrap()).unwrap();
    let before = draft.clone();
    assert!(
        draft
            .add_joint(
                "floating",
                [Point::new(-10., -20.), Point::new(10., -20.)],
                plug(10.),
                true,
            )
            .is_err()
    );
    assert_eq!(draft, before, "a detached opening is still refused");

    let mut moving = draft.definition.clone();
    moving.params.push(Param::Range {
        id: "slide".into(),
        names: BTreeMap::from([("en".into(), "Slide".into())]),
        min: 0.,
        max: 10.,
        default: 0.,
        unit: Unit::Mm,
    });
    moving.parts.push(Part {
        id: "neck".into(),
        parent: None,
        param: "slide".into(),
        motion: Motion::Translate([0., 1.]),
        handle: None,
    });
    for shape in &mut moving.shapes {
        shape.part = Some("neck".into());
    }
    if let Joint::Plug { part, .. } = &mut moving.joints[0] {
        *part = Some("neck".into());
    }
    validate::validate(&moving).unwrap();
    moving.shapes[1].part = None;
    assert!(
        validate::validate(&moving)
            .unwrap_err()
            .contains("sits on the fixed body"),
        "both sides of a moving mouth must belong to its part"
    );
    // Filled moving parts around the mouth do not own the fixed rim merely
    // because they enclose its endpoints.
    for shape in &mut moving.shapes {
        shape.part = None;
    }
    let at = moving.joints[0].at();
    for (id, left, right) in [("left-pad", -6., -1.), ("right-pad", 1., 6.)] {
        let mut pad = moving.shapes[0].clone();
        pad.id = id.into();
        pad.layer = Layer::Inner;
        pad.role = Role::Solid;
        pad.material = Material::Rubber;
        pad.part = Some("neck".into());
        let points =
            [[left, -10.], [right, -10.], [right, 10.], [left, 10.]].map(|p| geom::add(at, p));
        pad.path = path(&geom::polygon(&points, true));
        moving.shapes.push(pad);
    }
    assert!(
        validate::validate(&moving)
            .unwrap_err()
            .contains("sits on the fixed body"),
        "a moving part must reach the rim boundary"
    );
    assert_eq!(doc, original);
}

#[test]
fn an_embedded_plug_keeps_its_original_attachment_rule() {
    let (doc, body, edges) = vessel();
    let mut draft = Draft::from_selection(&doc, &[body]).unwrap();
    draft.calibrate(edges, 10.).unwrap();
    draft.definition.joints.push(Joint::Plug {
        id: "embedded".into(),
        family: PlugFamily::Stopper,
        gender: Gender::M,
        size_mm: 10.,
        at: [0., 0.],
        dir: [0., 1.],
        part: None,
        poses: BTreeMap::new(),
    });
    validate::validate(&draft.definition).unwrap();
}

#[test]
fn unsupported_visible_strokes_are_refused_before_conversion() {
    for filled in [false, true] {
        let mut doc = Document::default();
        let id = rectangle(&mut doc, Point::default(), Point::new(40., 30.), filled);
        for style in [
            GraphicStyle {
                width_pt: 2.,
                fill: filled.then_some(Color::Paper),
                ..Default::default()
            },
            GraphicStyle {
                stroke: Color::Custom([180, 50, 55]),
                width_pt: if filled { 0.6 } else { 0.4 },
                fill: filled.then_some(Color::Paper),
                ..Default::default()
            },
        ] {
            doc.graphics[0].style = style;
            let before = doc.clone();
            let problem = Draft::from_selection(&doc, &[id]).unwrap_err();
            assert!(
                problem.contains("Ink") || problem.contains("0.6 pt") || problem.contains("0.4 pt"),
                "{problem}"
            );
            assert_eq!(doc, before);
        }
    }
}

#[test]
fn supported_outlines_details_and_imported_filled_artwork_still_convert() {
    let (mut doc, body, _) = vessel(); // ordinary editable 0.6 pt Ink outline
    let outline = rectangle(&mut doc, Point::new(60., 0.), Point::new(80., 10.), false);
    let detail = rectangle(&mut doc, Point::new(30., 0.), Point::new(50., 10.), false);
    doc.graphics
        .iter_mut()
        .find(|g| g.id == detail)
        .unwrap()
        .style
        .width_pt = 0.4;
    let draft = Draft::from_selection(&doc, &[body, outline, detail]).unwrap();
    assert_eq!(draft.definition.shapes[1].role, Role::Outline);
    assert_eq!(draft.definition.shapes[2].role, Role::Detail);
    for graphic in doc.graphics.iter_mut().filter(|g| g.style.fill.is_none()) {
        graphic.style.width_pt = (graphic.style.width_pt * 65536.).trunc() / 65536.;
    }
    let exchanged = Draft::from_selection(&doc, &[body, outline, detail]).unwrap();
    assert_eq!(exchanged.definition.shapes, draft.definition.shapes);
    let shown = draft.preview_graphic(100).unwrap();
    let paths = placed::primitives(&shown, doc.canvas_theme);
    assert!(paths.iter().any(|p| matches!(p, crate::scene::Primitive::Path { style, filled: false, .. } if style.stroke == Color::Ink && style.width_pt == 0.4)));
    assert!(paths.iter().any(|p| matches!(p, crate::scene::Primitive::Path { style, filled: false, .. } if style.stroke == Color::Ink && style.width_pt == 0.6)));
    // The SVG importer represents fills (including outlined SVG strokes) as
    // filled paths with a minimum-width stroke in the same color.
    let mut art = doc.graphics[0].clone();
    art.kind = GraphicKind::Path;
    art.path = doc.graphics[0].commands();
    art.origin = Point::default();
    art.axis_x = Point::new(1., 0.);
    art.axis_y = Point::new(0., 1.);
    art.style = GraphicStyle {
        stroke: Color::Custom([20, 80, 150]),
        fill: Some(Color::Custom([20, 80, 150])),
        width_pt: 0.1,
        ..Default::default()
    };
    doc.graphics = vec![art];
    assert!(Draft::from_selection(&doc, &[body]).is_ok());
    let closed = doc.graphics[0].path.clone();
    for path in [
        vec![
            PathCommand::Move(Point::default()),
            PathCommand::Line(Point::new(40., 0.)),
        ],
        vec![
            PathCommand::Move(Point::default()),
            PathCommand::Line(Point::new(40., 0.)),
            PathCommand::Close,
        ],
        closed
            .iter()
            .cloned()
            .chain([
                PathCommand::Move(Point::new(50., 0.)),
                PathCommand::Line(Point::new(80., 0.)),
            ])
            .collect(),
        closed
            .iter()
            .cloned()
            .chain([PathCommand::Line(Point::new(80., 0.))])
            .collect(),
    ] {
        doc.graphics[0].path = path;
        assert!(
            Draft::from_selection(&doc, &[body]).is_err(),
            "an open or empty fill cannot hide a visible stroke"
        );
    }
}

#[test]
fn outline_role_stays_unfilled_in_geometry_preview_save_and_materialization() {
    let mut doc = Document::default();
    let id = rectangle(&mut doc, Point::default(), Point::new(40., 30.), false);
    let mut draft = Draft::from_selection(&doc, &[id]).unwrap();
    draft
        .calibrate([Point::default(), Point::new(40., 0.)], 20.)
        .unwrap();
    assert_eq!(draft.definition.shapes[0].role, Role::Outline);
    assert!(!Role::Outline.filled());
    assert!(!Role::Detail.filled());
    assert!(
        draft
            .rim([Point::new(15., 15.), Point::new(25., 15.)])
            .unwrap()
            .ambiguous
    );
    let accepted = draft.accept(&doc, "lab/open-outline").unwrap();
    let graphic = &accepted.document.graphics[0];
    assert!(!graphic.hit(Point::new(20., 15.), 0.));
    assert!(graphic.hit(Point::new(0., 15.), 0.));
    let (lo, hi) = graphic.shape_bounds();
    assert!(lo.x <= -crate::style::DEFAULT.world(0.6) / 2.);
    assert!(hi.x >= 40. + crate::style::DEFAULT.world(0.6) / 2.);
    let materialized = placed::materialized(&accepted.document);
    assert!(
        materialized
            .graphics
            .iter()
            .all(|g| g.style.fill.is_none() && g.style.width_pt == 0.6)
    );
    let reopened = Document::from_json(&accepted.document.file_json().unwrap()).unwrap();
    assert_eq!(reopened.piece_definitions[0].shapes[0].role, Role::Outline);
    assert_eq!(placed::plan(&reopened.graphics[0]), placed::plan(graphic));
    let mut invalid = draft.definition.clone();
    invalid.liquids.push(Liquid {
        id: "sample".into(),
        region: "M-1 -1H1V1H-1Z".into(),
        meniscus: Default::default(),
    });
    assert!(
        validate::validate(&invalid)
            .unwrap_err()
            .contains("outside the glass")
    );
    invalid.liquids.clear();
    invalid.format_version = 1;
    assert!(
        validate::validate(&invalid)
            .unwrap_err()
            .contains("preview")
    );
    invalid.shapes[0].role = Role::Detail;
    validate::validate(&invalid).unwrap();
}

#[test]
fn shape_style_changes_are_typed_transactional_and_keep_geometry() {
    let (mut doc, body, edges) = vessel();
    let line = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        line,
        GraphicKind::Line,
        Point::new(-5., 20.),
        Point::new(5., 20.),
        Default::default(),
        Default::default(),
        false,
    ));
    let original = doc.clone();
    let mut draft = Draft::from_selection(&doc, &[body, line]).unwrap();
    draft.calibrate(edges, 10.).unwrap();
    let paths = world_paths(&draft);
    let body_id = draft.definition.shapes[0].id.clone();
    let line_id = draft.definition.shapes[1].id.clone();
    let style = ShapeStyle {
        layer: Layer::Front,
        material: Material::Metal,
        role: Role::Solid,
    };
    draft.set_shape_style(&body_id, style).unwrap();
    assert_eq!(ShapeStyle::from(&draft.definition.shapes[0]), style);
    assert_eq!(draft.definition.shapes[1].role, Role::Outline);
    same_paths(&world_paths(&draft), &paths);
    let before = draft.clone();
    for (id, style) in [
        (
            &body_id,
            ShapeStyle {
                role: Role::Glass,
                ..style
            },
        ),
        (&line_id, style),
        (&"missing".into(), style),
    ] {
        assert!(draft.set_shape_style(id, style).is_err());
        assert_eq!(draft, before);
    }
    let accepted = draft.accept(&doc, "lab/metal-body").unwrap();
    assert_eq!(doc, original);
    let saved = Document::from_json(&accepted.document.file_json().unwrap()).unwrap();
    assert_eq!(saved.piece_definitions[0].format_version, PREVIEW_VERSION);
    assert_eq!(
        ShapeStyle::from(&saved.piece_definitions[0].shapes[0]),
        style
    );
    let plan = placed::plan(&saved.graphics[0]);
    assert!(plan.iter().any(|p| p.pass == plan::Pass::Front
        && p.material == Material::Metal
        && p.ink == plan::Ink::FillStroke(plan::Weight::Outline)));
}

#[test]
fn shape_picking_follows_paint_order_and_never_fills_an_open_stroke() {
    let (mut doc, body, _) = vessel();
    let overlapping = rectangle(&mut doc, Point::new(-5., 10.), Point::new(5., 30.), true);
    let line = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        line,
        GraphicKind::Line,
        Point::new(-4., 20.),
        Point::new(4., 20.),
        Default::default(),
        Default::default(),
        false,
    ));
    let mut draft = Draft::from_selection(&doc, &[body, overlapping, line]).unwrap();
    let ids: Vec<_> = draft
        .definition
        .shapes
        .iter()
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(
        draft.shape_at(Point::new(0., 20.), 0.1),
        Some(ids[2].as_str())
    );
    // Body glass paints small fills before large ones, unlike source z-order.
    assert_eq!(
        draft.shape_at(Point::new(0., 25.), 0.1),
        Some(ids[0].as_str())
    );
    let style = ShapeStyle {
        layer: Layer::Front,
        material: Material::Rubber,
        role: Role::Solid,
    };
    draft.set_shape_style(&ids[1], style).unwrap();
    assert_eq!(
        draft.shape_at(Point::new(0., 20.), 0.1),
        Some(ids[1].as_str())
    );
    draft
        .set_shape_style(
            &ids[1],
            ShapeStyle {
                layer: Layer::Back,
                ..style
            },
        )
        .unwrap();
    assert_eq!(
        draft.shape_at(Point::new(0., 20.), 0.1),
        Some(ids[2].as_str())
    );
    assert_eq!(
        draft.shape_at(Point::new(0., 25.), 0.1),
        Some(ids[0].as_str())
    );
    assert_eq!(draft.shape_at(Point::new(100., 100.), 0.1), None);
    assert_eq!(draft.shape_at(Point::new(f32::NAN, 0.), 0.1), None);
    let p = draft.shape_path(&ids[2]).unwrap();
    same_paths(&[p], &[commands(&doc.graphics[2].commands())]);
}

#[test]
fn compound_shape_picking_keeps_same_winding_fills_and_opposite_winding_holes() {
    use PathCommand::{Close, Line, Move};

    let contour = |points: [Point; 4]| {
        std::iter::once(Move(points[0]))
            .chain(points.into_iter().skip(1).map(Line))
            .chain([Close])
    };
    for (nested, imported_fill) in [(false, false), (true, false), (true, true)] {
        for hole in [false, true] {
            let mut doc = Document::default();
            let behind = rectangle(&mut doc, Point::new(60., 30.), Point::new(90., 70.), true);
            let compound = rectangle(&mut doc, Point::default(), Point::new(150., 100.), true);
            let outer_right = if nested { 150. } else { 100. };
            let outer = [
                Point::default(),
                Point::new(outer_right, 0.),
                Point::new(outer_right, 100.),
                Point::new(0., 100.),
            ];
            let mut inner = if nested {
                [
                    Point::new(50., 20.),
                    Point::new(100., 20.),
                    Point::new(100., 80.),
                    Point::new(50., 80.),
                ]
            } else {
                [
                    Point::new(50., 0.),
                    Point::new(150., 0.),
                    Point::new(150., 100.),
                    Point::new(50., 100.),
                ]
            };
            if hole {
                inner.reverse();
            }
            let shape = &mut doc.graphics[1];
            shape.kind = GraphicKind::Path;
            shape.origin = Point::default();
            shape.axis_x = Point::new(1., 0.);
            shape.axis_y = Point::new(0., 1.);
            shape.path = contour(outer).chain(contour(inner)).collect();
            if imported_fill {
                // The SVG fill route also certifies these disjoint boundaries.
                shape.style.fill = Some(Color::Ink);
                shape.style.width_pt = 0.1;
            }
            let source = doc.clone();
            let mut draft = Draft::from_selection(&doc, &[behind, compound]).unwrap();
            draft
                .calibrate([Point::default(), Point::new(100., 0.)], 30.)
                .unwrap();
            let compound_id = format!("shape-{compound}");
            let expected = format!("shape-{}", if hole { behind } else { compound });
            assert_eq!(
                draft.shape_at(Point::new(75., 50.), 1.),
                Some(expected.as_str()),
                "nested={nested}, imported_fill={imported_fill}, hole={hole}"
            );
            assert_eq!(
                draft.shape_at(Point::new(25., 50.), 1.),
                Some(compound_id.as_str()),
                "the outer filled region remains pickable"
            );
            assert_eq!(doc, source);
        }
    }
}

#[test]
fn changing_joint_glass_to_solid_is_rejected_without_losing_the_port() {
    let (doc, body, edges) = vessel();
    let mut draft = Draft::from_selection(&doc, &[body]).unwrap();
    draft
        .add_joint(
            "mouth",
            edges,
            JointKind::Taper {
                size: "10/18".into(),
                gender: Gender::M,
                draw: JointDraw::Standard,
            },
            false,
        )
        .unwrap();
    let before = draft.clone();
    let id = draft.definition.shapes[0].id.clone();
    let error = draft
        .set_shape_style(
            &id,
            ShapeStyle {
                layer: Layer::Body,
                material: Material::Rubber,
                role: Role::Solid,
            },
        )
        .unwrap_err();
    assert!(
        error.contains("outline") || error.contains("glass"),
        "{error}"
    );
    assert_eq!(draft, before);
    assert_eq!(
        super::super::solve::ports(&draft.preview_graphic(42).unwrap()).len(),
        1
    );
}
