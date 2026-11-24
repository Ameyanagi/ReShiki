//! Movable parts and liquids: parameters set through the re-solve rule, and
//! the handles that drag or click them.
use super::builtin;
use super::eval::Value;
use super::placed;
use super::solve::{self, JointRef, Port, adjust, ports, residual};
use crate::document::{Document, Point};
use crate::graphics::Graphic;

fn add(doc: &mut Document, id: &str, at: Point) -> u64 {
    let d = builtin::find(id).unwrap();
    let n = doc.next_id();
    let scale = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics.push(placed::graphic(n, d, at, scale));
    n
}

fn port(doc: &Document, piece: u64, name: &str) -> Port {
    let g = graphic(doc, piece);
    ports(g).into_iter().find(|p| p.at.joint == name).unwrap()
}

fn middle(p: &Port) -> [f64; 2] {
    [
        p.point[0] + p.dir[0] * p.length / 2.,
        p.point[1] + p.dir[1] * p.length / 2.,
    ]
}

fn graphic(doc: &Document, id: u64) -> &Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

fn param(doc: &Document, id: u64, name: &str) -> Option<Value> {
    graphic(doc, id).piece.as_ref()?.params.get(name).cloned()
}

/// Bring `piece`'s hold `name` onto `at` and snap it there.
fn hold(doc: &mut Document, piece: u64, name: &str, at: [f64; 2]) {
    let c = middle(&port(doc, piece, name));
    let ids = solve::closure(doc, &[piece]);
    doc.translate(&ids, (at[0] - c[0]) as f32 + 3., (at[1] - c[1]) as f32 - 2.);
    solve::snap(doc, &ids, 14., false).unwrap();
}

/// Bring `piece`'s joint near `onto`'s joint and seat it through snapping.
fn seat(doc: &mut Document, piece: u64, name: &str, onto: u64, joint: &str) {
    let (m, s) = (port(doc, piece, name), port(doc, onto, joint));
    let ids = solve::closure(doc, &[piece]);
    doc.translate(
        &ids,
        (s.point[0] - m.point[0]) as f32 + 3.,
        (s.point[1] - m.point[1]) as f32 - 2.,
    );
    let snap = solve::snap(doc, &ids, 14., false).unwrap();
    assert!(matches!(snap.offer, solve::Offer::Seat));
    assert_eq!(snap.target.at, s.at);
    assert_eq!(snap.moving.at, m.at);
}

/// A 100 mL flask held by a clamp.
fn clamped() -> (Document, u64, u64) {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(300., 400.));
    let clamp = add(&mut doc, "reshiki/clamp-3finger", Point::new(100., 100.));
    let neck = middle(&port(&doc, flask, "neck-grip"));
    hold(&mut doc, clamp, "jaw", neck);
    (doc, flask, clamp)
}

fn seated(doc: &Document) {
    for s in &doc.seats {
        let (a, b) = (
            port(doc, s.a.piece, &s.a.joint),
            port(doc, s.b.piece, &s.b.joint),
        );
        let (off, turn) = residual(&a, &b);
        assert!(off < 0.01 && turn < 0.01, "{s:?}: {off} mm, {turn}°");
    }
}

/// A free socket whose stop lies at this end's eventual cut position.
fn end_socket(doc: &mut Document, tube: u64, end: &str, kind: &str, length: f64) -> u64 {
    let current = param(doc, tube, "length").unwrap_or(Value::Number(60.));
    adjust::set_end(doc, tube, "length", end, Value::Number(length)).unwrap();
    let m = port(doc, tube, end);
    let socket = add(doc, kind, Point::default());
    let s = port(doc, socket, "tube");
    let turn = (-m.dir[1]).atan2(-m.dir[0]).to_degrees() - s.dir[1].atan2(s.dir[0]).to_degrees();
    let pivot = graphic(doc, socket).origin;
    crate::editing::transform_about(doc, &[socket], pivot, 1., turn as f32);
    let s = port(doc, socket, "tube");
    doc.translate(
        &[socket],
        (m.point[0] - s.point[0] + s.dir[0] * s.insertion) as f32,
        (m.point[1] - s.point[1] + s.dir[1] * s.insertion) as f32,
    );
    adjust::set_end(doc, tube, "length", end, current).unwrap();
    socket
}

#[test]
fn either_tube_end_snaps_to_the_exact_stop_without_moving_its_other_end_or_fittings() {
    for end in ["a", "b"] {
        for (turn, mirror) in [(0., false), (33., false), (33., true)] {
            let mut doc = Document::default();
            let tube = add(
                &mut doc,
                "reshiki/compression-tube-60-1-4",
                Point::new(100., 120.),
            );
            crate::editing::transform_about(&mut doc, &[tube], Point::new(100., 120.), 1., turn);
            if mirror {
                let g = doc.graphics.iter_mut().find(|g| g.id == tube).unwrap();
                g.axis_x.x = -g.axis_x.x;
                g.axis_x.y = -g.axis_x.y;
            }
            let opposite = if end == "a" { "b" } else { "a" };
            let fixed = add(&mut doc, "reshiki/compression-cap-1-4", Point::default());
            let fixed_joint = port(&doc, tube, opposite).at;
            solve::connect(
                &mut doc,
                &[fixed],
                &fixed_joint,
                &JointRef {
                    piece: fixed,
                    joint: "tube".into(),
                },
            )
            .unwrap();
            let socket = end_socket(&mut doc, tube, end, "reshiki/compression-cap-1-4", 100.25);
            let pinned = port(&doc, tube, opposite).point;
            let cap = graphic(&doc, socket).clone();
            let fixed_cap = graphic(&doc, fixed).clone();
            let old_seats = doc.seats.clone();
            let (_, snap) =
                adjust::snap_end(&mut doc, tube, "length", end, Value::Number(98.5), 14.).unwrap();
            assert!(matches!(snap.unwrap().offer, solve::Offer::Seat));
            assert!(
                matches!(param(&doc, tube, "length"), Some(Value::Number(n)) if (n - 100.25).abs() < 1e-4)
            );
            assert!(old_seats.iter().all(|s| doc.seats.contains(s)));
            assert_eq!(graphic(&doc, socket).origin, cap.origin);
            assert_eq!(graphic(&doc, fixed).origin, fixed_cap.origin);
            let now = port(&doc, tube, opposite).point;
            assert!((now[0] - pinned[0]).hypot(now[1] - pinned[1]) < 1e-4);
            assert_eq!(doc.seats.len(), 2);
            seated(&doc);
        }
    }
}

#[test]
fn invalid_end_snap_keeps_the_native_length_edit_and_existing_placements() {
    for end in ["a", "b"] {
        for invalid in ["sideways", "angle", "occupied", "out of range", "scale"] {
            let mut doc = Document::default();
            let tube = add(
                &mut doc,
                "reshiki/compression-tube-60-1-4",
                Point::default(),
            );
            let length = if invalid == "out of range" {
                1000.
            } else {
                100.25
            };
            let socket = end_socket(&mut doc, tube, end, "reshiki/compression-cap-1-4", length);
            match invalid {
                "out of range" => {
                    let m = port(&doc, tube, end);
                    doc.translate(
                        &[socket],
                        (m.dir[0] * m.mm) as f32,
                        (m.dir[1] * m.mm) as f32,
                    );
                }
                "sideways" => doc.translate(&[socket], 0., 3.),
                "angle" | "scale" => {
                    let pivot = graphic(&doc, socket).origin;
                    crate::editing::transform_about(
                        &mut doc,
                        &[socket],
                        pivot,
                        if invalid == "scale" { 1.2 } else { 1. },
                        if invalid == "angle" { 3. } else { 0. },
                    );
                }
                "occupied" => {
                    let other = add(
                        &mut doc,
                        "reshiki/compression-tube-60-1-4",
                        Point::default(),
                    );
                    solve::connect(
                        &mut doc,
                        &[other],
                        &JointRef {
                            piece: socket,
                            joint: "tube".into(),
                        },
                        &JointRef {
                            piece: other,
                            joint: "a".into(),
                        },
                    )
                    .unwrap();
                }
                _ => {}
            }
            let value = Value::Number(if invalid == "out of range" {
                999.5
            } else {
                98.5
            });
            let mut native = doc.clone();
            adjust::set_end(&mut native, tube, "length", end, value.clone()).unwrap();
            let (_, snap) = adjust::snap_end(&mut doc, tube, "length", end, value, 14.).unwrap();
            assert!(snap.is_none(), "{end}: {invalid}");
            assert_eq!(doc, native, "{end}: {invalid}");
        }
    }
}

#[test]
fn either_end_suggests_an_exact_reducer_and_inserts_only_on_explicit_acceptance() {
    for end in ["a", "b"] {
        let mut doc = Document::default();
        let tube = add(
            &mut doc,
            "reshiki/compression-tube-60-1-4",
            Point::default(),
        );
        let socket = end_socket(&mut doc, tube, end, "reshiki/compression-cap-1-8", 100.25);
        let opposite = if end == "a" { "b" } else { "a" };
        let pinned = port(&doc, tube, opposite).point;
        let target = graphic(&doc, socket).clone();
        let mut native = doc.clone();
        adjust::set_end(&mut native, tube, "length", end, Value::Number(98.5)).unwrap();
        let (_, snap) =
            adjust::snap_end(&mut doc, tube, "length", end, Value::Number(98.5), 14.).unwrap();
        let solve::Offer::Reducer(d) = snap.unwrap().offer else {
            panic!("expected reducer suggestion");
        };
        assert_eq!(d.id, "reshiki/compression-reducer-1-4-1-8");
        assert_eq!(doc, native);
        let snap = adjust::connect_end(
            &mut doc,
            tube,
            "length",
            end,
            &JointRef {
                piece: socket,
                joint: "tube".into(),
            },
        )
        .unwrap();
        assert!(snap.added.is_some());
        assert_eq!(doc.seats.len(), 2);
        let now = port(&doc, tube, opposite).point;
        assert!((now[0] - pinned[0]).hypot(now[1] - pinned[1]) < 1e-4);
        assert_eq!(graphic(&doc, socket).origin, target.origin);
        assert_eq!(graphic(&doc, socket).axis_x, target.axis_x);
        seated(&doc);
        let accepted = doc.clone();
        assert!(
            adjust::connect_end(
                &mut doc,
                tube,
                "length",
                end,
                &JointRef {
                    piece: socket,
                    joint: "tube".into()
                }
            )
            .is_none()
        );
        assert_eq!(doc, accepted);
    }
}

#[test]
fn compression_reducer_move_offers_keep_geometry_and_require_exact_both_ends() {
    for (tube_size, socket_size, expected) in [
        ("1-4", "1-8", Some("reshiki/compression-reducer-1-4-1-8")),
        ("1-8", "1-4", Some("reshiki/compression-reducer-1-8-1-4")),
        ("6-mm", "1-4", None),
        ("1-4", "6-mm", None),
    ] {
        let mut doc = Document::default();
        let tube = add(
            &mut doc,
            &format!("reshiki/compression-tube-60-{tube_size}"),
            Point::default(),
        );
        let socket = end_socket(
            &mut doc,
            tube,
            "b",
            &format!("reshiki/compression-cap-{socket_size}"),
            100.25,
        );
        adjust::set_end(&mut doc, tube, "length", "b", Value::Number(98.5)).unwrap();
        let before = doc.clone();
        let snap = solve::snap(&mut doc, &[tube], 14., false).unwrap();
        assert_eq!(
            doc, before,
            "a reducer offer must not move equipment or insert"
        );
        match expected {
            Some(expected) => {
                let solve::Offer::Reducer(d) = snap.offer else {
                    panic!("expected exact reducer");
                };
                assert_eq!(d.id, expected);
                assert_eq!(
                    solve::compression_reducer(&port(&doc, socket, "tube"), &port(&doc, tube, "b"))
                        .unwrap()
                        .id,
                    expected
                );
            }
            None => assert!(matches!(snap.offer, solve::Offer::Mismatch)),
        }
    }
}

#[test]
fn a_tube_follows_a_moving_compression_stop_inside_a_fixed_mouth() {
    use super::format::{Joint, Motion, Param, Part, Unit};

    let mut doc = Document::default();
    let mut definition = (*builtin::find("reshiki/compression-cap-1-4").unwrap()).clone();
    definition.id = "test/stretching-compression-cap".into();
    definition.grounded = true;
    let socket = &mut definition.joints[0];
    let (mouth, dir) = (socket.at(), socket.dir());
    let Joint::Plug { part, .. } = socket else {
        panic!("the cap has a compression plug");
    };
    *part = Some("body".into());
    for shape in &mut definition.shapes {
        shape.part = Some("body".into());
    }
    definition.params.push(Param::Range {
        id: "stretch".into(),
        names: [("en".into(), "Stretch".into())].into(),
        min: 0.,
        max: 1.,
        default: 0.,
        unit: Unit::Fraction,
    });
    definition.parts.push(Part {
        id: "body".into(),
        parent: None,
        param: "stretch".into(),
        motion: Motion::Stretch {
            pivot: mouth,
            axis: dir,
            per_unit: 1.,
        },
        handle: None,
    });
    super::validate::validate(&definition).unwrap();
    let cap = doc.next_id();
    let mm = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics.push(placed::graphic(
        cap,
        definition.into(),
        Point::default(),
        mm,
    ));
    let tube = add(
        &mut doc,
        "reshiki/compression-tube-60-1-4",
        Point::default(),
    );
    seat(&mut doc, tube, "b", cap, "tube");
    let before = port(&doc, tube, "b");
    let opening = port(&doc, cap, "tube");
    let released = adjust::set(&mut doc, cap, "stretch", Value::Number(1.)).unwrap();
    assert!(released.is_empty());
    assert_eq!(doc.seats.len(), 1);
    assert_eq!(port(&doc, cap, "tube").point, opening.point);
    assert!((port(&doc, tube, "b").point[0] - before.point[0] - 13.5 * f64::from(mm)).abs() < 1e-5);
    seated(&doc);
}

#[test]
fn tilting_a_clamp_on_a_stand_turns_the_flask_it_holds_and_leaves_the_stand() {
    let (mut doc, flask, clamp) = clamped();
    let stand = add(&mut doc, "reshiki/stand", Point::new(-300., 0.));
    let boss = middle(&port(&doc, clamp, "boss"));
    hold(&mut doc, stand, "rod", boss);
    assert_eq!(doc.seats.len(), 2);
    let (stand_at, boss) = (graphic(&doc, stand).origin, port(&doc, clamp, "boss"));
    let before = graphic(&doc, flask).axis_x;
    let released = adjust::set(&mut doc, clamp, "tilt", Value::Number(30.)).unwrap();
    assert!(released.is_empty());
    assert_eq!(doc.seats.len(), 2);
    seated(&doc);
    assert_eq!(graphic(&doc, stand).origin, stand_at);
    let after = port(&doc, clamp, "boss");
    assert!((after.point[0] - boss.point[0]).abs() < 1e-3);
    // The flask turned by the tilt.
    let x = graphic(&doc, flask).axis_x;
    let turned = (x.y.atan2(x.x) - before.y.atan2(before.x)).to_degrees();
    assert!((turned.abs() - 30.).abs() < 0.01, "{turned}°");
    // Through the transaction, every seat survives.
    let mut committed = doc.clone();
    let _ = crate::transaction::reconcile(&mut committed, doc.clone()).unwrap();
    assert_eq!(committed.seats.len(), 2);
}

#[test]
fn tilting_a_loose_clamp_leaves_the_flask_and_turns_the_clamp_around_it() {
    let (mut doc, flask, clamp) = clamped();
    let flask_frame = (graphic(&doc, flask).origin, graphic(&doc, flask).axis_x);
    let clamp_at = graphic(&doc, clamp).origin;
    adjust::set(&mut doc, clamp, "tilt", Value::Number(-20.)).unwrap();
    assert_eq!(
        (graphic(&doc, flask).origin, graphic(&doc, flask).axis_x),
        flask_frame
    );
    assert_ne!(graphic(&doc, clamp).origin, clamp_at);
    assert_eq!(doc.seats.len(), 1);
    seated(&doc);
}

#[test]
fn an_opened_jaw_lets_go_and_closing_it_on_the_neck_takes_hold_again() {
    let (mut doc, flask, clamp) = clamped();
    let flask_at = graphic(&doc, flask).origin;
    let open = Value::State("open".into());
    let released = adjust::set(&mut doc, clamp, "jaw", open).unwrap();
    assert_eq!(released.len(), 1);
    assert!(doc.seats.is_empty());
    assert_eq!(graphic(&doc, flask).origin, flask_at);
    // Opened, a different neck size does not change what it would grip.
    adjust::set(&mut doc, clamp, "grip", Value::Number(30.)).unwrap();
    let released = adjust::set(&mut doc, clamp, "jaw", Value::State("closed".into())).unwrap();
    assert!(released.is_empty());
    assert_eq!(doc.seats.len(), 1);
    assert_eq!(param(&doc, clamp, "grip"), Some(Value::Number(13.)));
    seated(&doc);
}

#[test]
fn a_part_that_would_tear_an_assembly_releases_the_nearest_hold() {
    // A tube held at its neck and its body by two clamps on one stand:
    // tilting the upper clamp lets go of the neck, and the lower clamp keeps
    // the tube where it is.
    let mut doc = Document::default();
    let tube = add(&mut doc, "reshiki/jy-schlenk-tube", Point::new(300., 400.));
    let stand = add(&mut doc, "reshiki/stand", Point::new(-300., 0.));
    let upper = add(&mut doc, "reshiki/clamp-3finger", Point::new(100., 100.));
    let lower = add(&mut doc, "reshiki/clamp-3finger", Point::new(100., 900.));
    let neck = middle(&port(&doc, tube, "neck"));
    hold(&mut doc, upper, "jaw", neck);
    let rod = middle(&port(&doc, stand, "rod"));
    hold(&mut doc, upper, "boss", rod);
    // Both clamps reach as far, so on the rod at the body's height the
    // lower one's jaw closes on the body as well.
    let rod = port(&doc, stand, "rod");
    let body = middle(&port(&doc, tube, "body"));
    hold(&mut doc, lower, "boss", [rod.point[0], body[1]]);
    assert_eq!(doc.seats.len(), 4, "{:?}", doc.seats);
    let tube_at = graphic(&doc, tube).origin;
    let released = adjust::set(&mut doc, upper, "tilt", Value::Number(25.)).unwrap();
    let jaw = JointRef {
        piece: upper,
        joint: "jaw".into(),
    };
    assert_eq!(released.len(), 1, "{released:?}");
    assert!(released[0].a == jaw || released[0].b == jaw);
    assert_eq!(graphic(&doc, tube).origin, tube_at);
    assert_eq!(doc.seats.len(), 3);
    seated(&doc);
}

#[test]
fn a_valve_and_a_stopcock_switch_state_by_their_click_handles() {
    let mut doc = Document::default();
    let tube = add(&mut doc, "reshiki/jy-schlenk-tube", Point::new(0., 0.));
    let flask = add(
        &mut doc,
        "reshiki/schlenk-flask-100-15",
        Point::new(300., 0.),
    );
    for (id, name, next) in [(tube, "valve", "open"), (flask, "stopcock", "side")] {
        let handles = adjust::handles(graphic(&doc, id));
        let h = handles.iter().find(|h| h.toggle).unwrap();
        assert_eq!(h.param, name);
        let v = adjust::toggled(graphic(&doc, id), name).unwrap();
        assert_eq!(v, Value::State(next.into()));
        let before = placed::plan(graphic(&doc, id));
        adjust::set(&mut doc, id, name, v).unwrap();
        assert_ne!(*placed::plan(graphic(&doc, id)), *before);
    }
    // The valve's handle rose with the stem.
    let h = |doc: &Document| adjust::handles(graphic(doc, tube))[0].at;
    let open = h(&doc);
    adjust::set(&mut doc, tube, "valve", Value::State("closed".into())).unwrap();
    assert!(open[1] < h(&doc)[1]);
    // All four stopcock positions come round again.
    for _ in 0..3 {
        let v = adjust::toggled(graphic(&doc, flask), "stopcock").unwrap();
        adjust::set(&mut doc, flask, "stopcock", v).unwrap();
    }
    assert_eq!(
        param(&doc, flask, "stopcock"),
        Some(Value::State("closed".into()))
    );
}

#[test]
fn dragging_the_liquid_surface_sets_the_share_below_it() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(0., 0.));
    let h = adjust::handles(graphic(&doc, flask));
    let surface = h.iter().find(|h| h.part.is_none()).unwrap().clone();
    assert_eq!(surface.param, "fill");
    // Empty: the handle sits at the bottom of the bulb. Halfway up a round
    // bulb is half its area.
    let bottom = surface.at;
    let top = {
        let mut copy = doc.clone();
        adjust::set(&mut copy, flask, "fill", Value::Number(1.)).unwrap();
        adjust::handles(graphic(&copy, flask))
            .into_iter()
            .find(|h| h.part.is_none())
            .unwrap()
            .at
    };
    let mid = [bottom[0], (bottom[1] + top[1]) / 2.];
    let v = adjust::dragged(graphic(&doc, flask), &surface, mid).unwrap();
    let Value::Number(fill) = v else { panic!() };
    assert!((fill - 0.5).abs() <= 0.011, "{fill}");
    adjust::set(&mut doc, flask, "fill", v).unwrap();
    let at = adjust::handles(graphic(&doc, flask))[0].at;
    assert!((at[1] - mid[1]).abs() < 1., "{at:?} {mid:?}");
    // Out of range is clamped, and the liquid is painted.
    adjust::set(&mut doc, flask, "fill", Value::Number(3.)).unwrap();
    assert_eq!(param(&doc, flask, "fill"), Some(Value::Number(1.)));
    // Full, its handle is at the top of the bulb, inside the glass.
    let full = adjust::handles(graphic(&doc, flask))[0].at;
    assert!((full[0] - top[0]).abs() < 1. && (full[1] - top[1]).abs() < 1.);
    assert!(
        placed::plan(graphic(&doc, flask))
            .iter()
            .any(|p| p.material == super::format::Material::Liquid)
    );
}

#[test]
fn dragging_a_clamps_tilt_handle_turns_it_to_the_pointer() {
    let mut doc = Document::default();
    let clamp = add(&mut doc, "reshiki/clamp-3finger", Point::new(0., 0.));
    let h = adjust::handles(graphic(&doc, clamp));
    let tilt = h.iter().find(|h| h.param == "tilt").unwrap().clone();
    assert!(!tilt.toggle);
    // Turn the handle 40° about the boss.
    let pivot = middle(&port(&doc, clamp, "boss"));
    let (dx, dy) = (tilt.at[0] - pivot[0], tilt.at[1] - pivot[1]);
    let (s, c) = 40_f64.to_radians().sin_cos();
    let to = [pivot[0] + c * dx - s * dy, pivot[1] + s * dx + c * dy];
    let v = adjust::dragged(graphic(&doc, clamp), &tilt, to).unwrap();
    assert_eq!(v, Value::Number(40.));
    // Unknown parameters and values of the wrong kind are refused.
    assert!(adjust::set(&mut doc, clamp, "nope", Value::Number(1.)).is_err());
    assert!(adjust::set(&mut doc, clamp, "tilt", Value::State("x".into())).is_err());
    assert_eq!(param(&doc, clamp, "tilt"), None);
}

#[test]
fn clicking_a_cow_moves_both_side_receivers_and_keeps_the_largest_receiver_anchored() {
    let mut doc = Document::default();
    let cow = add(&mut doc, "reshiki/cow-15", Point::new(0., 0.));
    let large = add(&mut doc, "reshiki/rbf-100-15", Point::new(100., 100.));
    let left = add(&mut doc, "reshiki/rbf-50-15", Point::new(-100., 100.));
    let right = add(&mut doc, "reshiki/rbf-50-15", Point::new(200., 100.));
    seat(&mut doc, large, "neck", cow, "o2");
    seat(&mut doc, left, "neck", cow, "o1");
    seat(&mut doc, right, "neck", cow, "o3");
    assert_eq!(doc.seats.len(), 3);
    seated(&doc);
    let seats = doc.seats.clone();
    let frame = |doc: &Document, id| {
        let g = graphic(doc, id);
        (g.origin, g.axis_x, g.axis_y)
    };
    let (large_frame, cow_frame) = (frame(&doc, large), frame(&doc, cow));
    let mm = f64::from(placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM));
    let handles = adjust::handles(graphic(&doc, cow));
    let turn = handles.iter().find(|h| h.param == "turn").unwrap();
    assert!(turn.toggle);
    let middle = port(&doc, cow, "o2").point;
    let sides = [(left, "o1", -54.), (right, "o3", 54.)];
    let initial = sides.map(|(receiver, outlet, offset)| {
        let point = port(&doc, cow, outlet).point;
        assert!(((point[0] - middle[0]) / mm - offset).abs() < 1e-3);
        (frame(&doc, receiver), point)
    });

    // The click handle cycles from r2 through r3 and r1, then back to r2.
    let turned = 35_f64.to_radians().cos();
    for (state, projection) in [("r3", turned), ("r1", turned), ("r2", 1.)] {
        let v = adjust::toggled(graphic(&doc, cow), &turn.param).unwrap();
        assert_eq!(v, Value::State(state.into()));
        let released = adjust::set(&mut doc, cow, &turn.param, v).unwrap();
        assert!(released.is_empty(), "{state}: {released:?}");
        assert_eq!(doc.seats, seats, "{state}");
        seated(&doc);
        assert_eq!(frame(&doc, large), large_frame, "{state}");
        assert_eq!(frame(&doc, cow), cow_frame, "{state}");
        for ((receiver, outlet, offset), ((before, before_x, before_y), start)) in
            sides.into_iter().zip(initial)
        {
            let point = port(&doc, cow, outlet).point;
            assert!(
                ((point[0] - middle[0]) / mm - offset * projection).abs() < 1e-3,
                "{state} {outlet}: {point:?}"
            );
            let (at, x, y) = frame(&doc, receiver);
            assert!(
                (f64::from(at.x - before.x) - (point[0] - start[0])).abs() < 1e-3,
                "{state} {outlet}: {at:?} {before:?}"
            );
            assert!(
                (f64::from(at.y - before.y) - (point[1] - start[1])).abs() < 1e-3,
                "{state} {outlet}: {at:?} {before:?}"
            );
            assert!(x.distance(before_x) < 1e-6);
            assert!(y.distance(before_y) < 1e-6);
        }
    }
}

#[test]
fn a_seated_thermometers_depth_handle_moves_its_glass_without_moving_the_assembly() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(0., 0.));
    let adapter = add(
        &mut doc,
        "reshiki/thermometer-adapter-15",
        Point::new(100., -100.),
    );
    let thermometer = add(&mut doc, "reshiki/thermometer", Point::new(-100., -100.));
    seat(&mut doc, adapter, "bottom", flask, "neck");
    seat(&mut doc, thermometer, "stem", adapter, "thermo");
    assert_eq!(doc.seats.len(), 2);
    seated(&doc);
    let seats = doc.seats.clone();
    let origins = [flask, adapter, thermometer].map(|id| graphic(&doc, id).origin);
    let stem = port(&doc, thermometer, "stem").point;
    let before = graphic(&doc, thermometer).shape_bounds().0;
    let mm = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);

    let handles = adjust::handles(graphic(&doc, thermometer));
    let depth = handles.iter().find(|h| h.param == "depth").unwrap();
    assert!(!depth.toggle);
    assert_eq!(depth.part.as_deref(), Some("glass"));
    let to = [depth.at[0], depth.at[1] + f64::from(mm) * 10.];
    let v = adjust::dragged(graphic(&doc, thermometer), depth, to).unwrap();
    assert_eq!(v, Value::Number(120.));

    let released = adjust::set(&mut doc, thermometer, "depth", v).unwrap();
    assert!(released.is_empty());
    assert_eq!(param(&doc, thermometer, "depth"), Some(Value::Number(120.)));
    assert_eq!(doc.seats, seats);
    seated(&doc);
    assert_eq!(
        [flask, adapter, thermometer].map(|id| graphic(&doc, id).origin),
        origins
    );
    assert_eq!(port(&doc, thermometer, "stem").point, stem);
    let after = graphic(&doc, thermometer).shape_bounds().0;
    assert!((after.x - before.x).abs() < 1e-3);
    assert!(
        (after.y - before.y - mm * 10.).abs() < 1e-3,
        "{before:?} {after:?}"
    );
}

#[test]
fn dragging_a_balloons_top_inflates_it_without_moving_its_needle_or_septum() {
    let mut doc = Document::default();
    let septum = add(&mut doc, "reshiki/septum-15", Point::new(0., 0.));
    let balloon = add(&mut doc, "reshiki/balloon", Point::new(100., -100.));
    seat(&mut doc, balloon, "needle", septum, "needle");
    assert_eq!(doc.seats.len(), 1);
    seated(&doc);
    let seats = doc.seats.clone();
    let origins = [septum, balloon].map(|id| graphic(&doc, id).origin);
    let needle = port(&doc, balloon, "needle").point;
    let (before_lo, before_hi) = graphic(&doc, balloon).shape_bounds();
    let mm = f64::from(placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM));
    let handles = adjust::handles(graphic(&doc, balloon));
    let inflate = handles.iter().find(|h| h.param == "inflate").unwrap();
    assert!(!inflate.toggle);
    assert_eq!(inflate.part.as_deref(), Some("balloon"));
    let to = [inflate.at[0], f64::from(before_lo.y) - mm * 10.];
    let v = adjust::dragged(graphic(&doc, balloon), inflate, to).unwrap();
    let Value::Number(dragged) = v else { panic!() };
    assert!(dragged > 0.8 && dragged <= 1., "{dragged}");
    let released = adjust::set(&mut doc, balloon, "inflate", v).unwrap();
    assert!(released.is_empty());
    assert_eq!(
        param(&doc, balloon, "inflate"),
        Some(Value::Number(dragged))
    );
    assert_eq!(doc.seats, seats);
    seated(&doc);
    assert_eq!(
        [septum, balloon].map(|id| graphic(&doc, id).origin),
        origins
    );
    assert_eq!(port(&doc, balloon, "needle").point, needle);
    let (after_lo, after_hi) = graphic(&doc, balloon).shape_bounds();
    assert!(after_hi.x - after_lo.x > before_hi.x - before_lo.x);
    assert!(after_hi.y - after_lo.y > before_hi.y - before_lo.y);
    assert!(after_lo.y < before_lo.y);
}

#[test]
fn either_straight_tube_end_changes_length_with_the_opposite_end_fixed() {
    for stock in [60, 120] {
        for size in ["1-16", "1-8", "1-4", "3-8", "6-mm"] {
            for end in ["a", "b"] {
                for flip in [false, true] {
                    let mut doc = Document::default();
                    let tube = add(
                        &mut doc,
                        &format!("reshiki/compression-tube-{stock}-{size}"),
                        Point::new(42., -18.),
                    );
                    let g = doc.graphics.iter_mut().find(|g| g.id == tube).unwrap();
                    let (s, c) = 37_f32.to_radians().sin_cos();
                    let mm = g.axis_x.x;
                    g.axis_x = Point::new(c * mm, s * mm);
                    let sign = if flip { -1. } else { 1. };
                    g.axis_y = Point::new(-s * mm * sign, c * mm * sign);
                    let axes = (g.axis_x, g.axis_y);
                    let handles = adjust::handles(g);
                    assert_eq!(handles.len(), 2);
                    let handle = handles
                        .iter()
                        .find(|h| h.tube_end.as_deref() == Some(end))
                        .unwrap();
                    let opposite = if end == "a" { "b" } else { "a" };
                    let pinned = port(&doc, tube, opposite).point;
                    let moving = port(&doc, tube, end);
                    // Lateral pointer motion does not change the tube's axis.
                    let to = [
                        moving.point[0]
                            + f64::from(mm) * (24. * moving.dir[0] - 9. * moving.dir[1]),
                        moving.point[1]
                            + f64::from(mm) * (24. * moving.dir[1] + 9. * moving.dir[0]),
                    ];
                    let value = adjust::dragged(graphic(&doc, tube), handle, to).unwrap();
                    assert_eq!(value, Value::Number(f64::from(stock) + 24.));
                    assert!(
                        adjust::set_end(&mut doc, tube, "length", end, value)
                            .unwrap()
                            .is_empty()
                    );
                    assert_eq!(
                        (graphic(&doc, tube).axis_x, graphic(&doc, tube).axis_y),
                        axes
                    );
                    let after = port(&doc, tube, opposite).point;
                    assert!((after[0] - pinned[0]).hypot(after[1] - pinned[1]) < 1e-4);
                    let moved = port(&doc, tube, end).point;
                    assert!(
                        (moved[0] - moving.point[0] - f64::from(mm) * 24. * moving.dir[0]).abs()
                            < 1e-4
                    );
                    assert!(
                        (moved[1] - moving.point[1] - f64::from(mm) * 24. * moving.dir[1]).abs()
                            < 1e-4
                    );
                    let row = super::parts_list::tube_row(&doc, tube).unwrap().unwrap();
                    assert_eq!(row.cut_length_mm, f64::from(stock) + 24.);
                    let reopened = Document::from_json(&doc.file_json().unwrap()).unwrap();
                    assert_eq!(
                        super::parts_list::tube_row(&reopened, tube)
                            .unwrap()
                            .unwrap(),
                        row
                    );
                    // Either end also shortens, with the original limits.
                    adjust::set_end(&mut doc, tube, "length", end, Value::Number(-100.)).unwrap();
                    assert_eq!(param(&doc, tube, "length"), Some(Value::Number(20.)));
                    let after = port(&doc, tube, opposite).point;
                    assert!((after[0] - pinned[0]).hypot(after[1] - pinned[1]) < 1e-4);
                    adjust::set_end(&mut doc, tube, "length", end, Value::Number(2000.)).unwrap();
                    assert_eq!(param(&doc, tube, "length"), Some(Value::Number(1000.)));
                }
            }
        }
    }
}

#[test]
fn tube_end_edits_carry_their_fitting_and_keep_the_opposite_fitting_and_seats() {
    for end in ["a", "b"] {
        let mut doc = Document::default();
        let tube = add(
            &mut doc,
            "reshiki/compression-tube-60-1-4",
            Point::default(),
        );
        let caps = ["a", "b"].map(|end| {
            let cap = add(
                &mut doc,
                "reshiki/compression-cap-1-4",
                Point::new(-100., -100.),
            );
            assert!(
                solve::connect(
                    &mut doc,
                    &[cap],
                    &JointRef {
                        piece: tube,
                        joint: end.into()
                    },
                    &JointRef {
                        piece: cap,
                        joint: "tube".into()
                    }
                )
                .is_some()
            );
            cap
        });
        let fixed = if end == "a" { caps[1] } else { caps[0] };
        let moving = if end == "a" { caps[0] } else { caps[1] };
        let g = doc.graphics.iter_mut().find(|g| g.id == fixed).unwrap();
        let mut definition = (**g.piece.as_ref().unwrap().linked.as_ref().unwrap()).clone();
        definition.grounded = true;
        definition.id = "test/grounded-opposite-cap".into();
        let piece = g.piece.as_mut().unwrap();
        piece.definition = placed::key(&definition);
        piece.linked = Some(definition.into());
        let before = doc.clone();
        let opposite = if end == "a" { "b" } else { "a" };
        let pinned = port(&doc, tube, opposite).point;
        let direction = port(&doc, tube, end).dir;
        let mm = f64::from(graphic(&doc, tube).axis_x.x);
        assert!(
            adjust::set_end(&mut doc, tube, "length", end, Value::Number(100.))
                .unwrap()
                .is_empty()
        );
        assert_eq!(doc.seats, before.seats);
        assert_eq!(graphic(&doc, fixed), graphic(&before, fixed));
        let fixed = port(&doc, tube, opposite).point;
        assert!((fixed[0] - pinned[0]).hypot(fixed[1] - pinned[1]) < 1e-4);
        let old = graphic(&before, moving).origin;
        let now = graphic(&doc, moving).origin;
        assert!((f64::from(now.x - old.x) - 40. * mm * direction[0]).abs() < 1e-4);
        assert!((f64::from(now.y - old.y) - 40. * mm * direction[1]).abs() < 1e-4);
        seated(&doc);
        let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
        let row = super::parts_list::tube_row(&doc, tube).unwrap().unwrap();
        assert_eq!(row.cut_length_mm, 100.);
        assert_eq!(row.ends.map(|e| e.connection.unwrap().id), caps);
    }
}

#[test]
fn tube_end_drag_releases_a_fixed_fitting_instead_of_moving_the_opposite_end() {
    for end in ["a", "b"] {
        let mut doc = Document::default();
        let tube = add(
            &mut doc,
            "reshiki/compression-tube-60-1-4",
            Point::default(),
        );
        let cap = add(
            &mut doc,
            "reshiki/compression-cap-1-4",
            Point::new(-100., -100.),
        );
        solve::connect(
            &mut doc,
            &[cap],
            &JointRef {
                piece: tube,
                joint: end.into(),
            },
            &JointRef {
                piece: cap,
                joint: "tube".into(),
            },
        )
        .unwrap();
        let g = doc.graphics.iter_mut().find(|g| g.id == cap).unwrap();
        let mut definition = (**g.piece.as_ref().unwrap().linked.as_ref().unwrap()).clone();
        definition.grounded = true;
        definition.id = "test/grounded-compression-cap".into();
        let piece = g.piece.as_mut().unwrap();
        piece.definition = placed::key(&definition);
        piece.linked = Some(definition.into());
        let before = doc.clone();
        let opposite = if end == "a" { "b" } else { "a" };
        let pinned = port(&doc, tube, opposite).point;
        let released = adjust::set_end(&mut doc, tube, "length", end, Value::Number(100.)).unwrap();
        assert_eq!(released, before.seats);
        assert!(doc.seats.is_empty());
        assert_eq!(graphic(&doc, cap), graphic(&before, cap));
        let fixed = port(&doc, tube, opposite).point;
        assert!((fixed[0] - pinned[0]).hypot(fixed[1] - pinned[1]) < 1e-4);
    }
}

#[test]
fn tube_end_handles_accept_old_adjustable_definitions_and_reject_invalid_edits() {
    let mut doc = Document::default();
    let tube = add(
        &mut doc,
        "reshiki/compression-tube-60-1-4",
        Point::default(),
    );
    let g = doc.graphics.iter_mut().find(|g| g.id == tube).unwrap();
    let mut definition = (**g.piece.as_ref().unwrap().linked.as_ref().unwrap()).clone();
    definition.tube_spec = None;
    let piece = g.piece.as_mut().unwrap();
    piece.definition = placed::key(&definition);
    piece.linked = Some(definition.into());
    let handles = adjust::handles(g);
    assert_eq!(handles.len(), 2);
    let before = doc.clone();
    for (param, end, value) in [
        ("length", "nope", Value::Number(100.)),
        ("nope", "a", Value::Number(100.)),
        ("length", "a", Value::Number(f64::NAN)),
        ("length", "b", Value::State("long".into())),
    ] {
        assert!(adjust::set_end(&mut doc, tube, param, end, value).is_err());
        assert_eq!(doc, before);
    }
    assert!(adjust::dragged(graphic(&doc, tube), &handles[0], [f64::NAN, 0.]).is_none());
    adjust::set_end(&mut doc, tube, "length", "a", Value::Number(75.)).unwrap();
    assert_eq!(param(&doc, tube, "length"), Some(Value::Number(75.)));
    // The numeric Properties operation retains its original pivot.
    let anchor = port(&doc, tube, "a").point;
    adjust::set(&mut doc, tube, "length", Value::Number(90.)).unwrap();
    assert_eq!(port(&doc, tube, "a").point, anchor);
    let bend = add(
        &mut doc,
        "reshiki/compression-tube-bend-90-1-4",
        Point::default(),
    );
    let handles = adjust::handles(graphic(&doc, bend));
    assert_eq!(handles.len(), 2);
    assert!(handles.iter().all(|h| h.tube_end.is_none()));
    assert!(adjust::set_end(&mut doc, bend, "inlet_length", "a", Value::Number(100.)).is_err());
}
