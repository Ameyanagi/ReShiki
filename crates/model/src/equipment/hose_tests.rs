//! Hose placement, reshaping, persistence and painting.
use super::builtin;
use super::format::Xy;
use super::geometry::{Affine, add, len, mul, sub};
use super::placed::{self, PieceRef};
use super::solve::hose::{self, Grip};
use super::solve::{self, Port};
use crate::document::{Document, Point};
use crate::graphics::Graphic;

const REACH: f64 = 14.;

fn with_condenser() -> (Document, u64) {
    let mut doc = Document::default();
    let d = builtin::find("reshiki/liebig-29").unwrap();
    let id = doc.next_id();
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics
        .push(placed::graphic(id, d, Point::new(200., 300.), scale));
    (doc, id)
}

fn place(doc: &mut Document, from: Xy, to: Xy) -> u64 {
    let d = builtin::find("reshiki/hose-7").unwrap();
    hose::place_hose(doc, d, from, to, REACH).0
}

fn connected() -> (Document, u64, u64) {
    let (mut doc, condenser) = with_condenser();
    let from = port(&doc, condenser, "water-out").point;
    let to = port(&doc, condenser, "water-in").point;
    let hose = place(&mut doc, from, to);
    (doc, condenser, hose)
}

fn graphic(doc: &Document, id: u64) -> &Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

fn port(doc: &Document, id: u64, joint: &str) -> Port {
    solve::ports(graphic(doc, id))
        .into_iter()
        .find(|p| p.at.joint == joint)
        .unwrap()
}

fn assert_near(actual: Xy, expected: Xy) {
    assert!(
        len(sub(actual, expected)) < 1e-3,
        "expected {expected:?}, got {actual:?}"
    );
}

fn assert_seated(doc: &Document, end: &Port, barb: &Port) {
    assert_eq!(solve::partner(doc, &end.at).as_ref(), Some(&barb.at));
    let (off, turn) = solve::residual(end, barb);
    assert!(
        off <= solve::TOLERANCE_MM && turn <= solve::TOLERANCE_DEG,
        "{:?} on {:?}: {off} mm, {turn}°",
        end.at,
        barb.at
    );
}

#[test]
fn a_hose_seats_at_both_ends_and_leaves_along_each_barb() {
    let (mut doc, condenser, hose) = connected();
    assert_eq!(doc.seats.len(), 2);
    let seats = doc.seats.clone();
    solve::check(&mut doc, None);
    assert_eq!(doc.seats, seats);

    let outlet = port(&doc, condenser, "water-out");
    let inlet = port(&doc, condenser, "water-in");
    assert_seated(&doc, &port(&doc, hose, "a"), &outlet);
    assert_seated(&doc, &port(&doc, hose, "b"), &inlet);

    let g = graphic(&doc, hose);
    let line = &g.piece.as_ref().unwrap().centre_line;
    assert_eq!(line.len(), 7);
    let (x, y) = (g.axis_x, g.axis_y);
    let frame = Affine([x.x, x.y, y.x, y.y, 0., 0.].map(f64::from));
    assert_near(frame.apply_dir(sub(line[1], line[0])), outlet.dir);
    assert_near(frame.apply_dir(sub(line[5], line[6])), inlet.dir);
    assert!(solve::closure(&doc, &[condenser]).contains(&hose));
}

#[test]
fn clicking_a_barb_hangs_a_hose_to_its_right_and_below_it() {
    let (mut doc, condenser) = with_condenser();
    let barb = port(&doc, condenser, "water-in");
    let hose = place(&mut doc, barb.point, barb.point);

    assert_eq!(doc.seats.len(), 1);
    assert_seated(&doc, &port(&doc, hose, "a"), &barb);
    assert_near(
        port(&doc, hose, "b").point,
        add(barb.point, mul([30., 50.], barb.mm)),
    );
}

#[test]
fn clicking_away_from_the_barbs_lays_a_free_hose() {
    let (mut doc, _) = with_condenser();
    let at = [600., 500.];
    let hose = place(&mut doc, at, at);

    assert!(doc.seats.is_empty());
    let start = port(&doc, hose, "a");
    assert_near(start.point, at);
    assert_near(port(&doc, hose, "b").point, add(at, [30. * start.mm, 0.]));
}

#[test]
fn dragging_one_end_releases_its_seat_and_snaps_it_back() {
    let (mut doc, condenser, hose) = connected();
    let seats = doc.seats.clone();
    let outlet = port(&doc, condenser, "water-out");
    let inlet = port(&doc, condenser, "water-in");
    let end = Grip::End(1);
    let away = add(inlet.point, [80., 40.]);

    assert!(hose::drag(&mut doc, hose, end, away, REACH).is_none());
    assert_eq!(doc.seats.len(), 1);
    assert_seated(&doc, &port(&doc, hose, "a"), &outlet);
    assert_near(port(&doc, hose, "b").point, away);

    let near = add(inlet.point, [3., -4.]);
    let snapped = hose::drag(&mut doc, hose, end, near, REACH).unwrap();
    assert_eq!(snapped.at, inlet.at);
    assert_eq!(doc.seats, seats);
    let b = port(&doc, hose, "b");
    assert_seated(&doc, &b, &inlet);
    assert_near(b.point, inlet.point);
}

#[test]
fn dragging_the_middle_bends_the_hose_without_moving_its_ends() {
    let (mut doc, _, hose) = connected();
    let seats = doc.seats.clone();
    let a = port(&doc, hose, "a").point;
    let b = port(&doc, hose, "b").point;
    let to = add(b, [60., 80.]);

    assert!(hose::drag(&mut doc, hose, Grip::Middle, to, REACH).is_none());
    assert_eq!(doc.seats, seats);
    for seat in &seats {
        let a = port(&doc, seat.a.piece, &seat.a.joint);
        let b = port(&doc, seat.b.piece, &seat.b.joint);
        assert_seated(&doc, &a, &b);
    }
    let middle = hose::grips(graphic(&doc, hose))
        .into_iter()
        .find(|(grip, _)| *grip == Grip::Middle)
        .unwrap();
    assert_near(middle.1, to);
    assert_near(port(&doc, hose, "a").point, a);
    assert_near(port(&doc, hose, "b").point, b);
}

#[test]
fn a_hose_reopens_with_its_centre_line_seats_and_drawing() {
    let (doc, _, hose) = connected();
    let original = graphic(&doc, hose);
    let plan = placed::plan(original);
    assert!(!plan.is_empty());

    let bytes = doc.file_json().unwrap();
    let reopened = Document::from_json(&bytes).unwrap();
    let restored = graphic(&reopened, hose);
    assert_eq!(
        restored.piece.as_ref().unwrap().centre_line,
        original.piece.as_ref().unwrap().centre_line
    );
    assert_eq!(reopened.seats, doc.seats);
    assert_eq!(placed::plan(restored).len(), plan.len());
    assert_eq!(restored.shape_bounds(), original.shape_bounds());
}

#[test]
fn a_malformed_centre_line_falls_back_to_a_straight_cubic() {
    let d = builtin::find("reshiki/hose-7").unwrap();
    let mut piece = PieceRef::new(d.clone());
    piece.centre_line = vec![[10., 5.]; 5];
    let joint_at = |id| d.joints.iter().find(|j| j.id() == id).unwrap().at();
    let (a, b) = (joint_at("a"), joint_at("b"));

    let line = placed::centre_line(&d, &piece).unwrap();
    assert_eq!(line.len(), 4);
    assert_eq!(line[0], a);
    assert_eq!(line[3], b);
    assert_near(line[1], add(a, mul(sub(b, a), 1. / 3.)));
    assert_near(line[2], add(a, mul(sub(b, a), 2. / 3.)));
}

#[test]
fn moving_the_condenser_carries_its_hose_and_keeps_the_seats() {
    let (mut doc, condenser, hose) = connected();
    let seats = doc.seats.clone();
    let before = hose::grips(graphic(&doc, hose));
    let ids = solve::closure(&doc, &[condenser]);
    doc.translate(&ids, 37., -24.);
    solve::check(&mut doc, None);

    assert_eq!(doc.seats, seats);
    assert_seated(
        &doc,
        &port(&doc, hose, "a"),
        &port(&doc, condenser, "water-out"),
    );
    assert_seated(
        &doc,
        &port(&doc, hose, "b"),
        &port(&doc, condenser, "water-in"),
    );
    let after = hose::grips(graphic(&doc, hose));
    assert_eq!(after.len(), before.len());
    for ((old_grip, old), (new_grip, new)) in before.into_iter().zip(after) {
        assert_eq!(new_grip, old_grip);
        assert_near(new, add(old, [37., -24.]));
    }
}

#[test]
fn the_paint_plan_extends_a_seated_hose_over_the_barb() {
    let (mut doc, condenser) = with_condenser();
    let barb = port(&doc, condenser, "water-in");
    // A straight hose isolates the cuff in the horizontal bounds.
    let to = add(barb.point, mul(barb.dir, 60. * barb.mm));
    let hose = place(&mut doc, barb.point, to);
    assert_eq!(doc.seats.len(), 1);
    assert_seated(&doc, &port(&doc, hose, "a"), &barb);
    let (seated_lo, seated_hi) = graphic(&doc, hose).shape_bounds();

    doc.seats.clear();
    solve::check(&mut doc, None);
    let (free_lo, free_hi) = graphic(&doc, hose).shape_bounds();
    let extension = f64::from(free_lo.x - seated_lo.x);
    assert!((extension - 12. * barb.mm).abs() < 0.01, "{extension}");
    assert!(seated_hi.distance(free_hi) < 0.01);
}

#[test]
fn a_new_hose_takes_the_scale_of_the_barb_it_starts_on() {
    let (mut doc, condenser) = with_condenser();
    let g = doc.graphics.iter_mut().find(|g| g.id == condenser).unwrap();
    g.axis_x = Point::new(g.axis_x.x * 2., g.axis_x.y * 2.);
    g.axis_y = Point::new(g.axis_y.x * 2., g.axis_y.y * 2.);
    let barb = port(&doc, condenser, "water-in");
    let hose = place(&mut doc, barb.point, barb.point);

    assert_eq!(doc.seats.len(), 1);
    assert_seated(&doc, &port(&doc, hose, "a"), &barb);
    solve::check(&mut doc, None);
    assert_eq!(doc.seats.len(), 1);
}
