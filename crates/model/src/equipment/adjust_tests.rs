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
