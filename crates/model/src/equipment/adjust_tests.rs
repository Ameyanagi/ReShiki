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
fn turning_a_cow_moves_the_side_receiver_and_keeps_the_largest_receiver_anchored() {
    let mut doc = Document::default();
    let cow = add(&mut doc, "reshiki/cow-15", Point::new(0., 0.));
    let large = add(&mut doc, "reshiki/rbf-100-15", Point::new(100., 100.));
    let small = add(&mut doc, "reshiki/rbf-50-15", Point::new(-100., 100.));
    seat(&mut doc, large, "neck", cow, "o2");
    seat(&mut doc, small, "neck", cow, "o1");
    assert_eq!(doc.seats.len(), 2);
    seated(&doc);
    let seats = doc.seats.clone();
    let frame = |doc: &Document, id| {
        let g = graphic(doc, id);
        (g.origin, g.axis_x, g.axis_y)
    };
    let (large_frame, cow_frame, small_frame) =
        (frame(&doc, large), frame(&doc, cow), frame(&doc, small));

    // The middle outlet stays on the largest flask; the side outlet moves.
    for state in ["r1", "r3", "r2"] {
        let released = adjust::set(&mut doc, cow, "turn", Value::State(state.into())).unwrap();
        assert!(released.is_empty(), "{state}: {released:?}");
        assert_eq!(doc.seats, seats, "{state}");
        seated(&doc);
        assert_eq!(frame(&doc, large), large_frame, "{state}");
        assert_eq!(frame(&doc, cow), cow_frame, "{state}");
        if state == "r1" {
            assert!(graphic(&doc, small).origin.distance(small_frame.0) > 1e-3);
        }
    }
    let (at, x, y) = frame(&doc, small);
    assert!(at.distance(small_frame.0) < 1e-6, "{at:?} {small_frame:?}");
    assert!(x.distance(small_frame.1) < 1e-6);
    assert!(y.distance(small_frame.2) < 1e-6);
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
    let Value::Number(dragged) = v else { panic!() };
    // Depth drags round to half a millimetre: 190 + 10 = 200.
    assert!((dragged - 200.).abs() <= 0.5, "{dragged}");

    let released = adjust::set(&mut doc, thermometer, "depth", Value::Number(120.)).unwrap();
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
        (before.y - after.y - mm * 70.).abs() < 1e-3,
        "{before:?} {after:?}"
    );
}

#[test]
fn inflating_a_balloon_grows_its_bounds_without_moving_its_needle_or_septum() {
    let mut doc = Document::default();
    let septum = add(&mut doc, "reshiki/septum-15", Point::new(0., 0.));
    let balloon = add(&mut doc, "reshiki/balloon", Point::new(100., -100.));
    seat(&mut doc, balloon, "needle", septum, "needle");
    assert_eq!(doc.seats.len(), 1);
    seated(&doc);
    let seats = doc.seats.clone();
    let origins = [septum, balloon].map(|id| graphic(&doc, id).origin);
    let needle = port(&doc, balloon, "needle").point;
    let bounds = [0.2, 1.].map(|inflate| {
        let released = adjust::set(&mut doc, balloon, "inflate", Value::Number(inflate)).unwrap();
        assert!(released.is_empty());
        assert_eq!(doc.seats, seats);
        seated(&doc);
        assert_eq!(
            [septum, balloon].map(|id| graphic(&doc, id).origin),
            origins
        );
        assert_eq!(port(&doc, balloon, "needle").point, needle);
        graphic(&doc, balloon).shape_bounds()
    });
    let [(small_lo, small_hi), (full_lo, full_hi)] = bounds;
    assert!(full_hi.x - full_lo.x > small_hi.x - small_lo.x);
    assert!(full_hi.y - full_lo.y > small_hi.y - small_lo.y);
    assert!(full_lo.y < small_lo.y);
}
