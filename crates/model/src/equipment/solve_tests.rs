//! Snapping, seats and assemblies.
use super::builtin;
use super::placed;
use super::solve::{self, JointRef, Offer, ports, residual};
use crate::document::{Document, Point};

fn add(doc: &mut Document, id: &str, at: Point) -> u64 {
    let d = builtin::find(id).unwrap();
    let n = doc.next_id();
    let scale = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics.push(placed::graphic(n, d, at, scale));
    n
}

fn joint(piece: u64, joint: &str) -> JointRef {
    JointRef {
        piece,
        joint: joint.into(),
    }
}

fn port(doc: &Document, j: &JointRef) -> solve::Port {
    let g = doc.graphics.iter().find(|g| g.id == j.piece).unwrap();
    ports(g).into_iter().find(|p| p.at == *j).unwrap()
}

/// Move `piece` so its joint `name` is `off` world units from where `onto`'s
/// joint seats it.
fn bring(doc: &mut Document, piece: u64, name: &str, onto: &JointRef, off: (f32, f32)) {
    let s = port(doc, onto);
    let m = port(doc, &joint(piece, name));
    let exposure = 0.06 * 25. * s.mm;
    let target = [
        s.point[0] + s.dir[0] * exposure,
        s.point[1] + s.dir[1] * exposure,
    ];
    let ids = solve::closure(doc, &[piece]);
    doc.translate(
        &ids,
        (target[0] - m.point[0]) as f32 + off.0,
        (target[1] - m.point[1]) as f32 + off.1,
    );
}

/// A reconciled edit, as the app commits one.
fn commit(doc: &mut Document, before: Document) {
    let _ = crate::transaction::reconcile(doc, before).unwrap();
}

const BORE_FITTING: &str = "reshiki/compression-bulkhead-union-bored-through-1-4";

fn centre(p: &solve::Port) -> [f64; 2] {
    [
        p.point[0] + p.dir[0] * p.length / 2.,
        p.point[1] + p.dir[1] * p.length / 2.,
    ]
}

fn add_bore_on_tube(doc: &mut Document, tube: u64, distance_mm: f64) -> u64 {
    let span = port(doc, &joint(tube, "span"));
    let bore = add(doc, BORE_FITTING, Point::default());
    let old = centre(&port(doc, &joint(bore, "bore")));
    doc.translate(
        &[bore],
        (span.point[0] + span.dir[0] * distance_mm * span.mm - old[0]) as f32,
        (span.point[1] + span.dir[1] * distance_mm * span.mm - old[1] + 10. * span.mm) as f32,
    );
    let snap = solve::connect(doc, &[bore], &span.at, &joint(bore, "bore")).unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    bore
}

#[test]
fn through_bores_require_exact_nominal_od_and_a_full_length_tube_span() {
    use super::format::{Gender, HoldKind, PlugFamily};
    use super::solve::Kind;

    let mut doc = Document::default();
    let tube = add(
        &mut doc,
        "reshiki/compression-tube-60-1-4",
        Point::default(),
    );
    let fitting = add(&mut doc, BORE_FITTING, Point::default());
    let span = port(&doc, &joint(tube, "span"));
    let bore = port(&doc, &joint(fitting, "bore"));
    assert_eq!(span.kind, Kind::Hold(HoldKind::TubeSpan, [6.35; 2]));
    assert_eq!(bore.kind, Kind::Hold(HoldKind::ThroughBore, [6.35; 2]));
    assert_eq!((span.gender, bore.gender), (Gender::F, Gender::M));
    assert!(span.shared());
    assert!(!bore.shared());
    assert!((bore.contact_length / bore.mm - 48.).abs() < 1e-6);
    assert!(solve::mates(&span, &bore));
    for kind in [
        Kind::Hold(HoldKind::ThroughBore, [6.; 2]),
        Kind::Hold(HoldKind::ThroughBore, [6., 7.]),
        Kind::Hold(HoldKind::Jaw, [6.35; 2]),
        Kind::Plug(PlugFamily::Compression, 6.35),
    ] {
        assert!(!solve::mates(
            &span,
            &solve::Port {
                kind,
                ..bore.clone()
            }
        ));
    }
    let mut short = span.clone();
    short.length = 47.9 * short.mm;
    assert!(!solve::mates(&short, &bore));
    short.length = 48. * short.mm;
    assert!(solve::mates(&short, &bore));
}

#[test]
fn tube_span_and_through_bore_definitions_reject_ranges_and_adjustable_fit() {
    use super::format::{Diameter, Param, Unit};
    use std::collections::BTreeMap;

    for id in ["reshiki/compression-tube-60-1-4", BORE_FITTING] {
        let original = builtin::find(id).unwrap();
        let hold = if id == BORE_FITTING { "bore" } else { "span" };
        let mut ranged = (*original).clone();
        ranged
            .holds
            .iter_mut()
            .find(|h| h.id == hold)
            .unwrap()
            .diameter_mm = Diameter::Range([6.35, 6.35]);
        assert!(
            super::validate::validate(&ranged)
                .unwrap_err()
                .contains("one nominal tube diameter")
        );
        let mut adjustable = (*original).clone();
        adjustable.params.push(Param::Range {
            id: "fit".into(),
            names: BTreeMap::from([("en".into(), "Fit".into())]),
            min: 6.,
            max: 7.,
            default: 6.35,
            unit: Unit::Mm,
        });
        adjustable
            .holds
            .iter_mut()
            .find(|h| h.id == hold)
            .unwrap()
            .fit = Some("fit".into());
        assert!(
            super::validate::validate(&adjustable)
                .unwrap_err()
                .contains("only a jaw or slider fits")
        );
    }
}

#[test]
fn a_bore_turns_onto_a_tube_and_keeps_its_full_span_inside_either_end() {
    for angle in [90., 180.] {
        for end in [0., 1.] {
            let mut doc = Document::default();
            let tube = add(
                &mut doc,
                "reshiki/compression-tube-60-1-4",
                Point::default(),
            );
            let fitting = add(&mut doc, BORE_FITTING, Point::new(100., 100.));
            let pivot = doc
                .graphics
                .iter()
                .find(|g| g.id == fitting)
                .unwrap()
                .origin;
            crate::editing::transform_about(&mut doc, &[fitting], pivot, 1., angle);
            let span = port(&doc, &joint(tube, "span"));
            let old = centre(&port(&doc, &joint(fitting, "bore")));
            doc.translate(
                &[fitting],
                (span.point[0] + span.dir[0] * span.length * end - old[0]) as f32,
                (span.point[1] + span.dir[1] * span.length * end - old[1]) as f32,
            );
            solve::connect(&mut doc, &[fitting], &span.at, &joint(fitting, "bore")).unwrap();
            let bore = port(&doc, &joint(fitting, "bore"));
            let c = centre(&bore);
            let inset = bore.contact_length / 2.;
            let along = if end == 0. {
                inset
            } else {
                span.length - inset
            };
            let expected = [
                span.point[0] + span.dir[0] * along,
                span.point[1] + span.dir[1] * along,
            ];
            assert!((c[0] - expected[0]).hypot(c[1] - expected[1]) < 1e-3);
            let (off, turn) = residual(&span, &bore);
            assert!(off < 1e-3 && turn < 1e-3);
        }
    }
}

#[test]
fn one_continuous_tube_carries_multiple_bores_through_section_and_reopen() {
    use super::eval::Value;

    let mut doc = Document::default();
    let tube = add(
        &mut doc,
        "reshiki/compression-tube-120-1-4",
        Point::default(),
    );
    let first = add_bore_on_tube(&mut doc, tube, 28.);
    let second = add_bore_on_tube(&mut doc, tube, 92.);
    assert_eq!(doc.graphics.len(), 3);
    assert_eq!(doc.seats.len(), 2);
    assert!(builtin::find(BORE_FITTING).unwrap().joints.is_empty());
    let frames: Vec<_> = doc
        .graphics
        .iter()
        .map(|g| (g.id, g.origin, g.axis_x, g.axis_y))
        .collect();
    let seats = doc.seats.clone();
    for view in ["section", "exterior"] {
        for id in [tube, first, second] {
            let released =
                solve::adjust::set(&mut doc, id, "view", Value::State(view.into())).unwrap();
            assert!(released.is_empty());
            assert_eq!(doc.seats, seats);
        }
    }
    assert_eq!(
        doc.graphics
            .iter()
            .map(|g| (g.id, g.origin, g.axis_x, g.axis_y))
            .collect::<Vec<_>>(),
        frames
    );
    let mut back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(back.seats, seats);
    assert_eq!(back.graphics.len(), 3);
    assert!(solve::detach(&mut back, &joint(first, "bore")));
    assert_eq!(back.seats.len(), 1);
    assert!(back.seats[0].a == joint(second, "bore") || back.seats[0].b == joint(second, "bore"));
    assert!(solve::partner(&back, &joint(second, "bore")).is_some());
}

#[test]
fn tube_and_bore_sliding_both_respect_the_whole_contact_span() {
    let mut doc = Document::default();
    let tube = add(
        &mut doc,
        "reshiki/compression-tube-120-1-4",
        Point::default(),
    );
    let first = add_bore_on_tube(&mut doc, tube, 28.);
    let _second = add_bore_on_tube(&mut doc, tube, 92.);
    let mm = port(&doc, &joint(tube, "span")).mm;
    // Use the same click-expanded selection and movement helper as a normal
    // pointer press, rather than supplying the reduced moving set directly.
    let clicked = doc.click_selection(&[], &[first]);
    assert_eq!(clicked.len(), 3);
    let fitting_drag = solve::carried_through(&doc, &clicked, &[first], &[]).unwrap();
    assert_eq!(fitting_drag, [first]);
    for (direction, limit) in [(-1., -4.), (1., 68.)] {
        let moved = solve::constrain(&doc, &fitting_drag, [direction * 1e6, 100.]).unwrap();
        assert!((moved[0] / mm - limit).abs() < 1e-3 && moved[1].abs() < 1e-6);
        let mut slid = doc.clone();
        slid.translate(&fitting_drag, moved[0] as f32, moved[1] as f32);
        solve::check(&mut slid, None);
        assert_eq!(slid.seats.len(), 2);
    }
    assert_eq!(
        solve::carried_through(&doc, &[first], &[first], &[first]).unwrap(),
        [first]
    );
    let clicked = doc.click_selection(&[], &[tube]);
    let tube_drag = solve::carried_through(&doc, &clicked, &[tube], &[]).unwrap();
    assert_eq!(tube_drag, [tube]);
    // A click-release selects the assembly; dragging that full preselection
    // carries it whole. An individual selected piece remains able to slide.
    assert!(solve::carried_through(&doc, &clicked, &[tube], &clicked).is_none());
    let whole = solve::carried(&doc, &clicked, &[tube]);
    assert_eq!(whole.len(), 3);
    assert!(solve::constrain(&doc, &whole, [1e6, 100.]).is_none());
    // Both stationary bores constrain a tube dragged through them: only
    // 4 mm remains before either 48 mm passage reaches an endpoint.
    for direction in [-1., 1.] {
        let moved = solve::constrain(&doc, &tube_drag, [direction * 1e6, 100.]).unwrap();
        assert!((moved[0] / mm - direction * 4.).abs() < 1e-3);
        assert!(moved[1].abs() < 1e-6);
        let mut slid = doc.clone();
        slid.translate(&tube_drag, moved[0] as f32, moved[1] as f32);
        solve::check(&mut slid, None);
        assert_eq!(slid.seats.len(), 2);
    }
    doc.translate(&[tube], (5. * mm) as f32, 0.);
    solve::check(&mut doc, None);
    assert_eq!(doc.seats.len(), 1);
    // A fitting rotated off the common axis cannot keep its mechanical seat.
    let fitting = if doc.seats[0].a.piece == tube {
        doc.seats[0].b.piece
    } else {
        doc.seats[0].a.piece
    };
    let pivot = doc
        .graphics
        .iter()
        .find(|g| g.id == fitting)
        .unwrap()
        .origin;
    crate::editing::transform_about(&mut doc, &[fitting], pivot, 1., 90.);
    solve::check(&mut doc, None);
    assert!(doc.seats.is_empty());
}

#[test]
fn a_through_drag_carries_endpoint_groups_but_respects_groups_across_the_bore() {
    let mut doc = Document::default();
    let tube = add(
        &mut doc,
        "reshiki/compression-tube-120-1-4",
        Point::default(),
    );
    let fitting = add_bore_on_tube(&mut doc, tube, 60.);
    let cap = add(&mut doc, "reshiki/compression-cap-1-4", Point::default());
    solve::connect(&mut doc, &[cap], &joint(tube, "b"), &joint(cap, "tube")).unwrap();
    let annotation = doc.add_atom("C", Point::new(400., 100.));
    let group = doc.next_id();
    doc.groups.push(crate::grouping::Group {
        id: group,
        members: vec![cap, annotation],
        integral: false,
        layout: None,
    });
    let clicked = doc.click_selection(&[], &[tube]);
    let moving = solve::carried_through(&doc, &clicked, &[tube], &[]).unwrap();
    assert!(moving.contains(&tube) && moving.contains(&cap) && moving.contains(&annotation));
    assert!(!moving.contains(&fitting));
    let mm = port(&doc, &joint(tube, "span")).mm;
    let d = solve::constrain(&doc, &moving, [1e6, 100.]).unwrap();
    assert!((d[0] / mm - 36.).abs() < 1e-3 && d[1].abs() < 1e-6);
    doc.translate(&moving, d[0] as f32, d[1] as f32);
    solve::check(&mut doc, None);
    assert_eq!(doc.seats.len(), 2);
    // Grabbing an ordinary terminating fitting keeps ordinary assembly moves.
    assert!(solve::carried_through(&doc, &clicked, &[cap], &[]).is_none());
    for integral in [false, true] {
        doc.groups[0].members.push(tube);
        doc.groups[0].members.push(fitting);
        doc.groups[0].integral = integral;
        let clicked = doc.click_selection(&[], &[tube]);
        assert!(solve::carried_through(&doc, &clicked, &[tube], &[]).is_none());
    }
}

#[test]
fn rigid_tubes_snap_at_compression_mouths_and_seat_at_the_internal_stops() {
    use super::eval::Value;

    for name in [
        "compression-union-1-4",
        "compression-elbow-1-4",
        "compression-tee-1-4",
        "compression-cap-1-4",
        "compression-bulkhead-union-1-4",
    ] {
        let definition = builtin::find(&format!("reshiki/{name}")).unwrap();
        for socket in &definition.joints {
            for move_tube in [true, false] {
                let mut doc = Document::default();
                let tube_name = "reshiki/compression-tube-60-1-4";
                let origin = Point::default();
                let (fitting, tube) = if move_tube {
                    let fitting = add(&mut doc, &definition.id, origin);
                    (fitting, add(&mut doc, tube_name, origin))
                } else {
                    let tube = add(&mut doc, tube_name, origin);
                    (add(&mut doc, &definition.id, origin), tube)
                };
                let opening = port(&doc, &joint(fitting, socket.id()));
                let end = joint(tube, "a");
                let g = doc.graphics.iter_mut().find(|g| g.id == tube).unwrap();
                let [x, y] = opening.point.map(|v| v as f32);
                let [dx, dy] = opening.dir.map(|v| (v * opening.mm) as f32);
                g.origin = Point::new(x, y);
                g.axis_x = Point::new(dx, dy);
                g.axis_y = Point::new(-dy, dx);
                let moving = if move_tube { tube } else { fitting };
                let snap = solve::snap(&mut doc, &[moving], solve::REACH_PX / 2., false).unwrap();
                assert!(matches!(snap.offer, Offer::Seat));
                assert_eq!(doc.seats.len(), 1);
                let inside = |doc: &Document| {
                    let mouth = port(doc, &opening.at);
                    let tip = port(doc, &end);
                    super::geometry::dot(super::geometry::sub(mouth.point, tip.point), mouth.dir)
                        / mouth.mm
                };
                assert!((inside(&doc) - 13.5).abs() < 1e-5, "{name}");
                let section = Value::State("section".into());
                for piece in [fitting, tube] {
                    assert!(
                        solve::adjust::set(&mut doc, piece, "view", section.clone())
                            .unwrap()
                            .is_empty()
                    );
                }
                let mut reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
                assert_eq!(reopened.seats, doc.seats);
                assert!((inside(&reopened) - 13.5).abs() < 1e-5);
                assert!(solve::detach(&mut reopened, &opening.at));
                assert!(reopened.seats.is_empty());
                assert!((inside(&reopened) + 12.7).abs() < 1e-5);
            }
        }
    }
}

#[test]
fn a_mass_flow_controller_keeps_its_compression_ports_in_section_view() {
    use super::eval::Value;
    use super::format::{Gender, Joint, Material, PlugFamily};
    use super::solve::Kind;

    let mut doc = Document::default();
    let mfc = add(&mut doc, "reshiki/mass-flow-controller", Point::default());
    let inlet = port(&doc, &joint(mfc, "inlet"));
    let outlet = port(&doc, &joint(mfc, "outlet"));
    for socket in [&inlet, &outlet] {
        assert_eq!(socket.gender, Gender::F);
        assert_eq!(socket.kind, Kind::Plug(PlugFamily::Compression, 6.35));
        let tube_end = |family, size| solve::Port {
            kind: Kind::Plug(family, size),
            gender: Gender::M,
            ..socket.clone()
        };
        assert!(solve::mates(
            socket,
            &tube_end(PlugFamily::Compression, 6.35)
        ));
        assert!(!solve::mates(
            socket,
            &tube_end(PlugFamily::Compression, 6.)
        ));
        assert!(!solve::mates(socket, &tube_end(PlugFamily::Hose, 6.35)));
    }
    assert!((inlet.dir[0] + 1.).abs() < 1e-6 && inlet.dir[1].abs() < 1e-6);
    assert!((outlet.dir[0] - 1.).abs() < 1e-6 && outlet.dir[1].abs() < 1e-6);

    let mut tubing = (*builtin::find("reshiki/hose-7").unwrap()).clone();
    tubing.id = "test/compression-tube".into();
    let spec = tubing.tube.as_mut().unwrap();
    spec.width_mm = 6.35;
    spec.material = Material::Metal;
    for end in &mut tubing.joints {
        let Joint::Plug {
            family,
            gender,
            size_mm,
            ..
        } = end
        else {
            panic!("tube ends are plug joints");
        };
        (*family, *gender, *size_mm) = (PlugFamily::Compression, Gender::M, 6.35);
    }
    let tube = doc.next_id();
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics.push(placed::graphic(
        tube,
        tubing.into(),
        Point::new(200., 0.),
        scale,
    ));
    let end = joint(tube, "a");
    let snap = solve::connect(&mut doc, &[tube], &outlet.at, &end).unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    let seated = port(&doc, &end);

    let section = Value::State("section".into());
    let released = solve::adjust::set(&mut doc, mfc, "view", section.clone()).unwrap();
    assert!(released.is_empty());
    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.seats, doc.seats);
    assert_eq!(reopened.seats.len(), 1);
    assert_eq!(port(&reopened, &end).point, seated.point);
    assert_eq!(
        reopened.graphics[0]
            .piece
            .as_ref()
            .unwrap()
            .params
            .get("view"),
        Some(&section),
    );
    for exterior in [inlet, outlet] {
        let restored = port(&reopened, &exterior.at);
        assert_eq!(restored.kind, exterior.kind);
        assert_eq!(restored.point, exterior.point);
        assert_eq!(restored.dir, exterior.dir);
    }
}

#[test]
fn a_condenser_dropped_near_a_neck_seats_upright_and_stays_seated_when_reopened() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    let before = doc.clone();
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (6., -5.),
    );
    let snap = solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    assert_eq!(snap.target.at, joint(flask, "neck"));
    let (off, turn) = residual(
        &port(&doc, &joint(flask, "neck")),
        &port(&doc, &joint(condenser, "bottom")),
    );
    assert!(off < 0.01 && turn < 0.01, "{off} mm, {turn}°");
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 1);
    let seated = |doc: &Document, id: u64| {
        doc.graphics
            .iter()
            .find(|g| g.id == id)
            .unwrap()
            .piece
            .as_ref()
            .unwrap()
            .seated
            .clone()
    };
    assert!(seated(&doc, flask).contains("neck"));
    assert!(seated(&doc, condenser).contains("bottom"));
    let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(back.seats, doc.seats);
    assert_eq!(seated(&back, condenser), seated(&doc, condenser));
}

#[test]
fn a_turned_condenser_snaps_only_within_forty_degrees_unless_it_is_new() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    crate::editing::transform_about(&mut doc, &[condenser], Point::new(400., 100.), 1., 60.);
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (2., 0.),
    );
    let mut dragged = doc.clone();
    assert!(solve::snap(&mut dragged, &[condenser], 14., false).is_none());
    assert!(dragged.seats.is_empty());
    let snap = solve::snap(&mut doc, &[condenser], 14., true).unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    let c = port(&doc, &joint(condenser, "bottom"));
    assert!((c.dir[1] - 1.).abs() < 1e-6, "{:?}", c.dir);
}

#[test]
fn a_size_mismatch_inserts_the_adapter_between_the_joints() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-29", Point::new(400., 100.));
    let before = doc.clone();
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (3., 3.),
    );
    let snap = solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    let Offer::Adapter(d) = &snap.offer else {
        panic!("{:?}", snap.offer)
    };
    assert_eq!(d.id, "reshiki/exp-15-29");
    let adapter = snap.added.unwrap();
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 2);
    assert_eq!(
        solve::assembly(&doc, flask),
        [flask, condenser, adapter].into_iter().collect()
    );
    // The adapter is painted with its assembly, between flask and condenser.
    let order: Vec<u64> = doc.graphics.iter().map(|g| g.id).collect();
    assert_eq!(order, [flask, adapter, condenser]);
}

#[test]
fn two_sockets_only_say_they_do_not_fit() {
    let mut doc = Document::default();
    let a = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let b = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    bring(&mut doc, b, "top", &joint(a, "neck"), (1., 1.));
    let moved = doc.clone();
    let snap = solve::snap(&mut doc, &[b], 14., false);
    // The condenser's top points up, like the neck: too far turned to snap.
    assert!(snap.is_none());
    let snap = solve::snap(&mut doc, &[b], 14., true).unwrap();
    assert!(matches!(snap.offer, Offer::Mismatch));
    assert_eq!(doc, moved);
}

#[test]
fn moving_one_piece_alone_releases_its_seat_and_moving_the_assembly_keeps_it() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (0., 0.),
    );
    solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    commit(&mut doc, Document::default());
    // The whole assembly moves, turns and scales together.
    let before = doc.clone();
    let both = solve::closure(&doc, &[flask]);
    assert_eq!(both.len(), 2);
    doc.translate(&both, 5000., -3000.);
    crate::editing::transform_about(&mut doc, &both, Point::new(5100., -2900.), 1.7, 33.);
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 1);
    // A piece moved on its own comes apart.
    let before = doc.clone();
    doc.translate(&[condenser], 0., -1.);
    commit(&mut doc, before);
    assert!(doc.seats.is_empty());
}

#[test]
fn alt_drag_pulls_out_the_piece_and_what_hangs_on_it_but_not_the_flask() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    let stopper = add(&mut doc, "reshiki/stopper-15", Point::new(600., 100.));
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (0., 0.),
    );
    solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    bring(
        &mut doc,
        stopper,
        "bottom",
        &joint(condenser, "top"),
        (0., 0.),
    );
    solve::snap(&mut doc, &[stopper], 14., false).unwrap();
    assert_eq!(doc.seats.len(), 2);
    let mut out = solve::detached(&doc, &[condenser]);
    out.sort();
    assert_eq!(out, [condenser, stopper]);
    // The flask is the anchor: pulling it takes only itself.
    assert_eq!(solve::detached(&doc, &[flask]), [flask]);
    // Dragging the pulled-out part releases only the seat to the flask.
    let mut dragged = doc.clone();
    dragged.translate(&out, 80., 0.);
    assert!(solve::snap(&mut dragged, &out, 14., false).is_none());
    assert_eq!(dragged.seats.len(), 1);
}

#[test]
fn copying_keeps_seats_whose_both_pieces_are_copied() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (0., 0.),
    );
    solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    let both = crate::editing::selection(&doc, &[flask, condenser]);
    assert_eq!(both.seats.len(), 1);
    let one = crate::editing::selection(&doc, &[condenser]);
    assert!(one.seats.is_empty());
    let before = doc.clone();
    let pasted = crate::editing::append(&mut doc, &both, Point::new(300., 0.));
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 2);
    assert!(pasted.contains(&doc.seats[1].a.piece) && pasted.contains(&doc.seats[1].b.piece));
}

#[test]
fn a_snap_takes_the_target_layer_and_says_when_it_crosses_the_structures() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let note = add(&mut doc, "reshiki/stopper-29", Point::new(900., 900.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    doc.graphics[2].layer = 2;
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (0., 0.),
    );
    let snap = solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    assert_eq!(snap.behind, Some(true));
    let order: Vec<(u64, i32)> = doc.graphics.iter().map(|g| (g.id, g.layer)).collect();
    assert_eq!(order, [(flask, -1), (condenser, -1), (note, -1)]);
    // Bring to Front on one piece takes its assembly along.
    let before = doc.clone();
    doc.graphics[0].layer = 5;
    commit(&mut doc, before);
    assert!(
        doc.graphics
            .iter()
            .filter(|g| g.id != note)
            .all(|g| g.layer == 5)
    );
}

#[test]
fn connect_lists_nearest_fitting_joints_and_seats_the_chosen_one() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let big = add(&mut doc, "reshiki/rbf-100-29", Point::new(900., 300.));
    let stopper = add(&mut doc, "reshiki/stopper-15", Point::new(300., 100.));
    let options = solve::options(&doc, &[stopper]);
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].0.at, joint(flask, "neck"));
    assert!(matches!(options[0].2, Offer::Seat));
    assert_eq!(options[1].0.at, joint(big, "neck"));
    assert!(matches!(options[1].2, Offer::Adapter(_)));
    let (s, m) = (options[1].0.at.clone(), options[1].1.at.clone());
    let snap = solve::connect(&mut doc, &[stopper], &s, &m).unwrap();
    assert!(snap.added.is_some());
    assert_eq!(doc.seats.len(), 2);
    // Detach pulls the stopper and its adapter off the flask, along the neck.
    assert!(solve::detach(
        &mut doc,
        &JointRef {
            piece: big,
            joint: "neck".into()
        }
    ));
    assert_eq!(doc.seats.len(), 1);
    let neck = port(&doc, &joint(big, "neck"));
    let adapter = snap.added.unwrap();
    let a = port(&doc, &doc.seats[0].a.clone());
    assert!(a.at.piece == adapter || a.at.piece == stopper);
    assert!(a.point[1] < neck.point[1]);
}

#[test]
fn the_w_septum_seats_on_compatible_hose_barbs_throughout_the_catalogue() {
    use super::format::{Gender, Joint, PlugFamily};

    let mut checked = 0;
    for definition in builtin::catalogue() {
        for barb in definition.joints.iter().filter(|joint| {
            matches!(
                joint,
                Joint::Plug {
                    family: PlugFamily::Hose,
                    gender: Gender::M,
                    size_mm: 7.,
                    ..
                }
            )
        }) {
            let mut doc = Document::default();
            let vessel = add(&mut doc, &definition.id, Point::new(250., 300.));
            let septum = add(&mut doc, "reshiki/septum-hose-7", Point::new(80., 120.));
            let target = joint(vessel, barb.id());
            let cap = joint(septum, "barb");
            let snap = solve::connect(&mut doc, &[septum], &target, &cap)
                .unwrap_or_else(|| panic!("{} {}", definition.id, barb.id()));
            assert!(matches!(snap.offer, Offer::Seat));
            let (gap, angle) = residual(&port(&doc, &target), &port(&doc, &cap));
            assert!(
                gap < 0.01 && angle < 0.01,
                "{} {}",
                definition.id,
                barb.id()
            );
            doc.validate().unwrap();
            checked += 1;
        }
    }
    assert!(checked >= 52, "checked {checked} compatible hose barbs");
}

#[test]
fn the_w_septum_seats_narrow_end_first() {
    use super::geometry::{flatten, inside, parse_path};

    let septum = builtin::find("reshiki/septum-hose-7").unwrap();
    let outline = flatten(&parse_path(&septum.shapes[0].path).unwrap());
    assert!(inside([6., -8.], &outline), "the exposed face is wide");
    assert!(!inside([6., 4.], &outline), "the seated side is narrow");
}

#[test]
fn a_schlenk_downward_barb_can_hold_a_septum_while_its_side_barb_holds_a_hose() {
    for tube in [
        "reshiki/schlenk-tube-15",
        "reshiki/schlenk-tube-straight-29-left",
    ] {
        let mut doc = Document::default();
        let tube = add(&mut doc, tube, Point::new(250., 300.));
        let septum = add(&mut doc, "reshiki/septum-hose-7", Point::new(80., 120.));
        let hose = add(&mut doc, "reshiki/hose-7", Point::new(550., 300.));
        let downward = joint(tube, "hose2");
        solve::connect(&mut doc, &[septum], &downward, &joint(septum, "barb")).unwrap();
        assert!(solve::connect(&mut doc, &[hose], &downward, &joint(hose, "a")).is_none());
        solve::connect(&mut doc, &[hose], &joint(tube, "hose1"), &joint(hose, "a")).unwrap();
        let needle = add(&mut doc, "reshiki/balloon", Point::new(400., 500.));
        solve::connect(
            &mut doc,
            &[needle],
            &joint(septum, "needle"),
            &joint(needle, "needle"),
        )
        .unwrap();

        let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(back.seats, doc.seats);
        assert_eq!(back.seats.len(), 3);
    }
}

#[test]
fn schlenk_shoulders_keep_the_socket_and_clip_seat() {
    for (suffix, stopper) in [
        ("15", "reshiki/stopper-15"),
        ("15-left", "reshiki/stopper-15"),
        ("29", "reshiki/stopper-29"),
        ("29-left", "reshiki/stopper-29"),
    ] {
        for series in ["reshiki/schlenk-tube", "reshiki/schlenk-tube-straight"] {
            let mut doc = Document::default();
            let tube = add(
                &mut doc,
                &format!("{series}-{suffix}"),
                Point::new(200., 300.),
            );
            let stopper = add(&mut doc, stopper, Point::new(400., 100.));
            solve::connect(
                &mut doc,
                &[stopper],
                &joint(tube, "neck"),
                &joint(stopper, "bottom"),
            )
            .unwrap();
            assert_eq!(solve::add_clips(&mut doc, &[tube, stopper]), 1);
            assert!(doc.seats[0].clip.is_some());
            let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
            assert_eq!(back.seats, doc.seats, "{series}-{suffix}");
        }
    }
}

#[test]
fn standalone_stopcocks_keep_the_top_view_port_axes_in_every_flow_setting() {
    use super::eval::Value;

    for valve in [
        "reshiki/stopcock-three-way-hose",
        "reshiki/stopcock-three-way-15",
        "reshiki/stopcock-three-way-29",
    ] {
        let mut doc = Document::default();
        let stopcock = add(&mut doc, valve, Point::new(250., 300.));
        let origin = doc.graphics.last().unwrap().origin;
        let axes = [("left", [-1., 0.]), ("right", [1., 0.]), ("down", [0., 1.])];
        let initial_points = axes.map(|(name, _)| port(&doc, &joint(stopcock, name)).point);
        for state in ["left-right", "left-down", "right-down", "all", "closed"] {
            solve::adjust::set(&mut doc, stopcock, "stopcock", Value::State(state.into())).unwrap();
            for ((name, direction), initial_point) in axes.into_iter().zip(initial_points) {
                let outlet = port(&doc, &joint(stopcock, name));
                let x = outlet.point[0] - f64::from(origin.x);
                let y = outlet.point[1] - f64::from(origin.y);
                assert_eq!(outlet.point, initial_point, "{valve} {state} {name}");
                assert!(
                    (outlet.dir[0] - direction[0]).abs() < 1e-9
                        && (outlet.dir[1] - direction[1]).abs() < 1e-9,
                    "{valve} {state} {name}: {:?}",
                    outlet.dir
                );
                assert!((x * direction[1] - y * direction[0]).abs() < 1e-9);
                assert!(x * direction[0] + y * direction[1] > 0.);
            }
        }
    }
}

#[test]
fn a_ground_glass_stopcock_keeps_all_seats_when_the_flow_changes() {
    use super::eval::Value;

    for (vessel, valve) in [
        ("reshiki/rbf-100-15", "reshiki/stopcock-three-way-15"),
        ("reshiki/rbf-100-29", "reshiki/stopcock-three-way-29"),
    ] {
        let mut doc = Document::default();
        let flask = add(&mut doc, vessel, Point::new(250., 300.));
        let stopcock = add(&mut doc, valve, Point::new(250., 100.));
        let hose = add(&mut doc, "reshiki/hose-7", Point::new(90., 100.));
        let septum = add(&mut doc, "reshiki/septum-hose-7", Point::new(400., 100.));
        for (moving, port, target, socket) in [
            (stopcock, "left", flask, "neck"),
            (hose, "a", stopcock, "right"),
            (septum, "barb", stopcock, "down"),
        ] {
            solve::connect(
                &mut doc,
                &[moving],
                &joint(target, socket),
                &joint(moving, port),
            )
            .unwrap_or_else(|| panic!("{valve} {socket}"));
        }
        let seats = doc.seats.clone();
        assert_eq!(seats.len(), 3);
        for state in ["left-right", "left-down", "right-down", "all", "closed"] {
            solve::adjust::set(&mut doc, stopcock, "stopcock", Value::State(state.into())).unwrap();
            assert_eq!(doc.seats, seats);
            doc.validate().unwrap();
        }
    }
}

#[test]
fn a_tampered_seat_is_released_on_open_and_the_pieces_stay_put() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (0., 0.),
    );
    solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    json["graphics"][1]["origin"]["x"] =
        serde_json::json!(json["graphics"][1]["origin"]["x"].as_f64().unwrap() + 1.);
    let back = Document::from_json(json.to_string().as_bytes()).unwrap();
    assert!(back.seats.is_empty());
    assert_eq!(back.graphics[1].origin.x, doc.graphics[1].origin.x + 1.);
}

#[test]
fn a_flask_connected_by_its_neck_to_a_cone_stays_seated_after_the_edit() {
    let mut doc = Document::default();
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(200., 100.));
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(600., 400.));
    let before = doc.clone();
    // The socket moves onto the cone: the shoulder still ends 0.06 L outside the mouth.
    solve::connect(
        &mut doc,
        &[flask],
        &joint(condenser, "bottom"),
        &joint(flask, "neck"),
    )
    .unwrap();
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 1);
    let (off, turn) = residual(
        &port(&doc, &joint(flask, "neck")),
        &port(&doc, &joint(condenser, "bottom")),
    );
    assert!(off < 0.01 && turn < 0.01, "{off} mm, {turn}°");
}

#[test]
fn a_second_joint_that_fits_after_a_snap_is_seated_and_joins_the_layer() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    let stopper = add(&mut doc, "reshiki/stopper-15", Point::new(700., 100.));
    // Seat the condenser and stopper, note where the stopper sits, then part them.
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (0., 0.),
    );
    solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    bring(
        &mut doc,
        stopper,
        "bottom",
        &joint(condenser, "top"),
        (0., 0.),
    );
    solve::snap(&mut doc, &[stopper], 14., false).unwrap();
    let mut parted = doc.clone();
    parted.seats.clear();
    parted.graphics[2].layer = 3;
    parted.translate(&[condenser], 0., -0.5);
    // Dropping the condenser back closes both joints at once, and the
    // stopper joins the flask's layer and place.
    let snap = solve::snap(&mut parted, &[condenser], 14., false).unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    assert_eq!(snap.behind, Some(true));
    assert_eq!(parted.seats.len(), 2);
    assert!(parted.graphics.iter().all(|g| g.layer == -1));
    assert_eq!(solve::assembly(&parted, flask).len(), 3);
}

#[test]
fn a_cone_lowered_until_its_tip_meets_the_mouth_already_snaps() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    let mm = port(&doc, &joint(flask, "neck")).mm as f32;
    // The shoulder is a whole ground length plus 2 mm above its seat; the tip
    // is just above the mouth.
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (0., -27. * mm),
    );
    assert!(solve::snap(&mut doc.clone(), &[condenser], 14., false).is_some());
    // Further up, nothing catches.
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (0., -50. * mm),
    );
    assert!(solve::snap(&mut doc, &[condenser], 14., false).is_none());
}

#[test]
fn detach_leaves_the_cone_out_of_snap_reach_and_scale_limits_block_a_snap() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let stopper = add(&mut doc, "reshiki/stopper-15", Point::new(600., 100.));
    bring(&mut doc, stopper, "bottom", &joint(flask, "neck"), (0., 0.));
    solve::snap(&mut doc, &[stopper], 14., false).unwrap();
    assert!(solve::detach(&mut doc, &joint(flask, "neck")));
    let mut again = doc.clone();
    again.translate(&[stopper], 6., 0.);
    assert!(solve::snap(&mut again, &[stopper], 14., false).is_none());
    // Seating on a flask at the largest scale would grow a co-selected piece
    // past it, so nothing snaps.
    let mut big = Document::default();
    let flask = add(&mut big, "reshiki/rbf-100-15", Point::new(200., 300.));
    let stopper = add(&mut big, "reshiki/stopper-15", Point::new(900., 100.));
    let other = add(&mut big, "reshiki/stopper-29", Point::new(1200., 100.));
    let k = placed::MAX_PT_PER_MM / placed::DEFAULT_PT_PER_MM;
    crate::editing::transform_about(&mut big, &[flask], Point::new(200., 300.), k, 0.);
    crate::editing::transform_about(&mut big, &[other], Point::new(1200., 100.), 2.5, 0.);
    bring(&mut big, stopper, "bottom", &joint(flask, "neck"), (0., 0.));
    assert!(solve::snap(&mut big.clone(), &[stopper], 14., false).is_some());
    assert!(solve::snap(&mut big, &[stopper, other], 14., false).is_none());
}

#[test]
fn shrinking_an_assembly_past_the_limit_stops_every_piece_there_and_keeps_the_seat() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let stopper = add(&mut doc, "reshiki/stopper-15", Point::new(600., 100.));
    bring(&mut doc, stopper, "bottom", &joint(flask, "neck"), (0., 0.));
    let before = doc.clone();
    solve::snap(&mut doc, &[stopper], 14., false).unwrap();
    commit(&mut doc, before);
    for (k, limit) in [(0.05, placed::MIN_PT_PER_MM), (40., placed::MAX_PT_PER_MM)] {
        let mut scaled = doc.clone();
        let ids = [flask, stopper];
        let k = placed::scale_limit(&scaled, &ids, k);
        crate::editing::transform_about(&mut scaled, &ids, Point::new(200., 300.), k, 0.);
        commit(&mut scaled, doc.clone());
        assert_eq!(scaled.seats.len(), 1, "scaled by {k}");
        let pt = crate::style::DEFAULT.points_per_world();
        for g in &scaled.graphics {
            let s = g.axis_x.distance(Point::default()) * pt;
            assert!((s - limit).abs() < 1e-3 * limit, "{s} pt per mm");
        }
    }
}

#[test]
fn a_neck_the_selection_cannot_scale_to_does_not_hide_one_it_can_seat_on() {
    let mut doc = Document::default();
    let big = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let k = placed::MAX_PT_PER_MM / placed::DEFAULT_PT_PER_MM;
    crate::editing::transform_about(&mut doc, &[big], Point::new(200., 300.), k, 0.);
    let stopper = add(&mut doc, "reshiki/stopper-15", Point::new(900., 100.));
    let other = add(&mut doc, "reshiki/stopper-29", Point::new(1200., 100.));
    crate::editing::transform_about(&mut doc, &[other], Point::new(1200., 100.), 2.5, 0.);
    // The stopper sits exactly on the big flask's neck, and a few units off
    // where a normal flask's neck would seat it.
    bring(&mut doc, stopper, "bottom", &joint(big, "neck"), (0., 0.));
    let normal = add(&mut doc, "reshiki/rbf-100-15", Point::new(2000., 2000.));
    let m = port(&doc, &joint(stopper, "bottom"));
    let s = port(&doc, &joint(normal, "neck"));
    let exposure = 0.06 * 25. * s.mm;
    doc.translate(
        &[normal],
        (m.point[0] - s.dir[0] * exposure - s.point[0]) as f32 + 4.,
        (m.point[1] - s.dir[1] * exposure - s.point[1]) as f32,
    );
    let snap = solve::snap(&mut doc, &[stopper, other], 14., false).unwrap();
    assert_eq!(snap.target.at, joint(normal, "neck"));
}

#[test]
fn a_joint_an_adapter_joins_wins_over_a_closer_one_nothing_joins() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-29", Point::new(200., 300.));
    let stopper = add(&mut doc, "reshiki/stopper-15", Point::new(600., 100.));
    bring(&mut doc, stopper, "bottom", &joint(flask, "neck"), (4., 0.));
    // Another stopper's cone right on the moving cone: two cones never seat.
    let other = add(&mut doc, "reshiki/stopper-15", Point::new(900., 600.));
    let m = port(&doc, &joint(stopper, "bottom"));
    let o = port(&doc, &joint(other, "bottom"));
    doc.translate(
        &[other],
        (m.point[0] - o.point[0]) as f32,
        (m.point[1] - o.point[1]) as f32,
    );
    let snap = solve::snap(&mut doc, &[stopper], 14., true).unwrap();
    assert_eq!(snap.target.at, joint(flask, "neck"));
    assert!(matches!(snap.offer, Offer::Adapter(_)));
    // Leaving the scale as it is stays possible past the limits.
    let k = placed::MAX_PT_PER_MM / placed::DEFAULT_PT_PER_MM;
    crate::editing::transform_about(&mut doc, &[flask], Point::new(200., 300.), k * 1.0005, 0.);
    assert_eq!(placed::scale_limit(&doc, &[flask, other], 1.), 1.);
}

#[test]
fn a_first_click_picks_the_assembly_and_a_second_the_piece() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(400., 100.));
    let before = doc.clone();
    bring(
        &mut doc,
        condenser,
        "bottom",
        &joint(flask, "neck"),
        (6., -5.),
    );
    solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 1);
    let whole = doc.click_selection(&[], &[condenser]);
    assert!(whole.contains(&flask) && whole.contains(&condenser));
    assert_eq!(doc.click_selection(&whole, &[condenser]), vec![condenser]);
    assert_eq!(doc.click_selection(&[condenser], &[flask]), vec![flask]);
    // In a group, the assembly comes between the group and the piece.
    let note = doc.next_id();
    doc.annotations.push(crate::document::Annotation {
        angle: 0.,
        id: note,
        position: Point::new(0., 0.),
        text: "reflux".into(),
        format: Default::default(),
    });
    doc.group_selection(&[flask, condenser, note]).unwrap();
    let whole = doc.click_selection(&[], &[condenser]);
    assert_eq!(whole.len(), 3);
    let mut assembly = doc.click_selection(&whole, &[condenser]);
    assembly.sort_unstable();
    let mut pair = vec![flask, condenser];
    pair.sort_unstable();
    assert_eq!(assembly, pair);
}

fn table_cells(doc: &mut Document, members: [Vec<u64>; 2]) -> u64 {
    use crate::graphics::{
        Graphic, GraphicKind,
        table::{Cell, Table},
    };
    let id = doc.next_id();
    let mut graphic = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        Point::new(-100., -100.),
        Point::new(2000., 2000.),
        Default::default(),
        Default::default(),
        false,
    );
    graphic.kind = GraphicKind::Table;
    graphic.table = Some(Box::new(Table {
        cells: members
            .into_iter()
            .enumerate()
            .map(|(index, members)| Cell {
                id: index as u32 + 1,
                lo: Point::new(index as f32 * 0.5, 0.),
                hi: Point::new((index + 1) as f32 * 0.5, 1.),
                members,
                borders: vec![],
            })
            .collect(),
    }));
    doc.graphics.push(graphic);
    id
}

#[test]
fn table_adapter_inherits_the_actual_endpoints_cell_for_snap_and_connect() {
    use crate::graphics::table::Membership;
    for on_canvas in [false, true] {
        for drag in [false, true] {
            let mut doc = Document::default();
            let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
            let condenser = add(&mut doc, "reshiki/liebig-29", Point::new(400., 100.));
            bring(
                &mut doc,
                condenser,
                "bottom",
                &joint(flask, "neck"),
                (3., 3.),
            );
            table_cells(
                &mut doc,
                [
                    vec![],
                    if on_canvas {
                        vec![]
                    } else {
                        vec![flask, condenser]
                    },
                ],
            );
            doc.validate().unwrap();
            let before = doc.clone();
            let snap = if drag {
                solve::snap(&mut doc, &[condenser], 14., false)
            } else {
                solve::connect(
                    &mut doc,
                    &[condenser],
                    &joint(flask, "neck"),
                    &joint(condenser, "bottom"),
                )
            }
            .unwrap();
            assert!(matches!(snap.offer, Offer::Adapter(_)));
            let adapter = snap.added.unwrap();
            let ownership = Membership::new(&doc).unwrap();
            assert_eq!(ownership.cell_of(adapter), ownership.cell_of(flask));
            assert_eq!(ownership.cell_of(adapter).is_none(), on_canvas);
            commit(&mut doc, before);
            assert_eq!(doc.seats.len(), 2);
            let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
            assert_eq!(
                Membership::new(&back).unwrap().cell_of(adapter),
                Membership::new(&back).unwrap().cell_of(flask)
            );
        }
    }
}

#[test]
fn table_cross_cell_equipment_snap_and_connect_are_atomic() {
    for condenser_kind in ["reshiki/liebig-15", "reshiki/liebig-29"] {
        for condenser_on_canvas in [false, true] {
            for drag in [false, true] {
                let mut doc = Document::default();
                let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(200., 300.));
                let condenser = add(&mut doc, condenser_kind, Point::new(400., 100.));
                bring(
                    &mut doc,
                    condenser,
                    "bottom",
                    &joint(flask, "neck"),
                    (3., 3.),
                );
                table_cells(
                    &mut doc,
                    [
                        vec![flask],
                        if condenser_on_canvas {
                            vec![]
                        } else {
                            vec![condenser]
                        },
                    ],
                );
                doc.validate().unwrap();
                let before = doc.clone();
                let snap = if drag {
                    solve::snap(&mut doc, &[condenser], 14., false)
                } else {
                    solve::connect(
                        &mut doc,
                        &[condenser],
                        &joint(flask, "neck"),
                        &joint(condenser, "bottom"),
                    )
                };
                assert!(snap.is_none());
                assert_eq!(doc, before);
            }
        }
    }
}

#[test]
fn a_tube_end_drag_auto_seats_only_within_its_own_table_cell() {
    use super::{eval::Value, solve::adjust};
    use crate::graphics::table::Membership;
    for end in ["a", "b"] {
        for together in [false, true] {
            let mut doc = Document::default();
            let tube = add(
                &mut doc,
                "reshiki/compression-tube-60-1-4",
                Point::default(),
            );
            let cap = add(&mut doc, "reshiki/compression-cap-1-4", Point::default());
            solve::connect(&mut doc, &[cap], &joint(tube, end), &joint(cap, "tube")).unwrap();
            adjust::set_end(&mut doc, tube, "length", end, Value::Number(100.)).unwrap();
            // Leave the cap exactly where this end will reach on its next extension.
            doc.seats.clear();
            adjust::set_end(&mut doc, tube, "length", end, Value::Number(60.)).unwrap();
            let table = table_cells(
                &mut doc,
                if together {
                    [vec![tube, cap], vec![]]
                } else {
                    [vec![tube], vec![cap]]
                },
            );
            let before = doc.clone();
            let owners = Membership::new(&doc).unwrap();
            adjust::set_end(&mut doc, tube, "length", end, Value::Number(100.)).unwrap();
            assert_eq!(doc.seats.len(), usize::from(together));
            assert_eq!(
                Membership::new(&doc).unwrap().cell_of(tube),
                owners.cell_of(tube)
            );
            assert_eq!(
                Membership::new(&doc).unwrap().cell_of(cap),
                owners.cell_of(cap)
            );
            let stored_table =
                |d: &Document| d.graphics.iter().find(|g| g.id == table).unwrap().clone();
            assert_eq!(stored_table(&doc), stored_table(&before));
            commit(&mut doc, before);
            assert_eq!(doc.seats.len(), usize::from(together));
        }
    }
}

#[test]
fn table_rejected_adapter_snap_restores_released_seats_and_their_owned_clip() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-300-29", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-29", Point::new(400., 100.));
    solve::connect(
        &mut doc,
        &[condenser],
        &joint(flask, "neck"),
        &joint(condenser, "bottom"),
    )
    .unwrap();
    assert_eq!(solve::add_clips(&mut doc, &[flask, condenser]), 1);
    let clip = doc.seats[0].clip.unwrap();
    let other = add(&mut doc, "reshiki/rbf-100-15", Point::new(5000., 300.));
    table_cells(&mut doc, [vec![other], vec![flask, condenser, clip]]);
    let target = port(&doc, &joint(other, "neck"));
    let moving = port(&doc, &joint(condenser, "bottom"));
    let exposure = 0.06 * 25. * target.mm;
    let at = [
        target.point[0] + target.dir[0] * exposure,
        target.point[1] + target.dir[1] * exposure,
    ];
    doc.translate(
        &[condenser],
        (at[0] - moving.point[0]) as f32 + 3.,
        (at[1] - moving.point[1]) as f32 + 3.,
    );
    let before = doc.clone();
    assert!(solve::snap(&mut doc, &[condenser], 14., false).is_none());
    assert_eq!(doc, before);
    assert_eq!(doc.seats[0].clip, Some(clip));
    assert!(doc.graphics.iter().any(|g| g.id == clip));
}

#[test]
fn table_adapter_does_not_reuse_a_released_highest_id_clip() {
    use crate::graphics::table::Membership;
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-300-29", Point::new(200., 300.));
    let condenser = add(&mut doc, "reshiki/liebig-29", Point::new(400., 100.));
    solve::connect(
        &mut doc,
        &[condenser],
        &joint(flask, "neck"),
        &joint(condenser, "bottom"),
    )
    .unwrap();
    let other = add(&mut doc, "reshiki/rbf-100-15", Point::new(5000., 300.));
    table_cells(&mut doc, [vec![], vec![flask, condenser, other]]);
    assert_eq!(solve::add_clips(&mut doc, &[flask, condenser]), 1);
    let clip = doc.seats[0].clip.unwrap();
    assert_eq!(doc.object_ids().max(), Some(clip));
    let target = port(&doc, &joint(other, "neck"));
    let moving = port(&doc, &joint(condenser, "bottom"));
    let exposure = 0.06 * 25. * target.mm;
    doc.translate(
        &[condenser],
        (target.point[0] + target.dir[0] * exposure - moving.point[0]) as f32 + 3.,
        (target.point[1] + target.dir[1] * exposure - moving.point[1]) as f32 + 3.,
    );
    let before = doc.clone();
    let snap = solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    assert!(matches!(snap.offer, Offer::Adapter(_)));
    let adapter = snap.added.unwrap();
    assert_ne!(adapter, clip);
    assert!(doc.graphics.iter().all(|g| g.id != clip));
    let ownership = Membership::new(&doc).unwrap();
    assert_eq!(ownership.cell_of(adapter), ownership.cell_of(other));
    assert_eq!(ownership.cell_of(clip), None);
    commit(&mut doc, before);
    Document::from_json(&doc.file_json().unwrap())
        .unwrap()
        .validate()
        .unwrap();
}
