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
fn stock_hose_bores_only_seat_on_the_matching_nmr_manifold() {
    let mut drawing = Document::default();
    let definition = builtin::find("reshiki/jy-nmr-evacuation-manifold").unwrap();
    let scale = placed::world_per_mm(&drawing, placed::DEFAULT_PT_PER_MM);
    for x in [100., 400.] {
        let id = drawing.next_id();
        drawing.graphics.push(placed::graphic(
            id,
            definition.clone(),
            Point::new(x, 200.),
            scale,
        ));
    }
    let barbs: Vec<_> = drawing
        .graphics
        .iter()
        .flat_map(solve::ports)
        .filter(|port| port.at.joint == "hose")
        .collect();
    assert_eq!(barbs.len(), 2);
    for (name, expected) in [("reshiki/hose-9", 2), ("reshiki/hose-7", 0)] {
        let mut doc = drawing.clone();
        let (hose, matched) = hose::place_hose(
            &mut doc,
            builtin::find(name).unwrap(),
            barbs[0].point,
            barbs[1].point,
            REACH,
        );
        assert_eq!(matched.len(), expected, "{name}");
        assert_eq!(doc.seats.len(), expected, "{name}");
        let seats = doc.seats.clone();
        solve::check(&mut doc, None);
        assert_eq!(doc.seats, seats);
        for (end, barb) in ["a", "b"].into_iter().zip(&barbs) {
            let end = port(&doc, hose, end);
            if expected == 2 {
                assert_seated(&doc, &end, barb);
            } else {
                assert!(solve::partner(&doc, &end.at).is_none());
            }
        }
    }
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
    assert_eq!(line.len(), 19);
    let (x, y) = (g.axis_x, g.axis_y);
    let frame = Affine([x.x, x.y, y.x, y.y, 0., 0.].map(f64::from));
    assert_near(frame.apply_dir(sub(line[1], line[0])), outlet.dir);
    assert_near(frame.apply_dir(sub(line[17], line[18])), inlet.dir);
    assert!(solve::closure(&doc, &[condenser]).contains(&hose));
}

#[test]
fn clicking_a_barb_hangs_a_hose_round_a_turn_and_down() {
    let (mut doc, condenser) = with_condenser();
    let barb = port(&doc, condenser, "water-in");
    let hose = place(&mut doc, barb.point, barb.point);

    assert_eq!(doc.seats.len(), 1);
    assert_seated(&doc, &port(&doc, hose, "a"), &barb);
    assert_near(
        port(&doc, hose, "b").point,
        add(barb.point, mul([20. + 1.5 * 9., 50.], barb.mm)),
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
    // The whole route is straight, not only its ends.
    let g = graphic(&doc, hose);
    let line = placed::sampled(&g.piece.as_ref().unwrap().centre_line);
    assert!(
        line.iter().all(|p| (p[1] - line[0][1]).abs() < 1e-3),
        "{line:?}"
    );
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

#[test]
fn a_hose_doubling_back_to_a_barb_loops_clear_of_itself() {
    // Both barbs face right: the hose passes the second barb and comes back.
    let (mut doc, first) = with_condenser();
    let d = builtin::find("reshiki/liebig-15").unwrap();
    let second = doc.next_id();
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    doc.graphics
        .push(placed::graphic(second, d, Point::new(300., 300.), scale));
    let from = port(&doc, first, "water-out").point;
    let to = port(&doc, second, "water-in").point;
    let hose = place(&mut doc, from, to);
    assert_eq!(doc.seats.len(), 2);

    let g = graphic(&doc, hose);
    let (x, y, o) = (g.axis_x, g.axis_y, g.origin);
    let frame = Affine([x.x, x.y, y.x, y.y, o.x, o.y].map(f64::from));
    let line: Vec<Xy> = g
        .piece
        .as_ref()
        .unwrap()
        .centre_line
        .iter()
        .map(|p| frame.apply(*p))
        .collect();
    let samples = placed::sampled(&line);
    // The way in (lead, turn and the run to the middle and on) stays a hose
    // width clear of the straight lead onto the second barb.
    let (way, lead) = (&samples[..=96], &samples[121..]);
    let clear = way
        .iter()
        .flat_map(|p| lead.iter().map(move |q| len(sub(*p, *q))))
        .fold(f64::INFINITY, f64::min);
    assert!(clear >= 9. * f64::from(scale), "{clear}");
}

fn flexible_metal_hose() -> std::sync::Arc<super::format::Definition> {
    std::sync::Arc::new(
        super::format::parse(include_str!(
            "../../../../assets/pieces/flexible-metal-hose-fm-1-4.reshiki-piece"
        ))
        .unwrap(),
    )
}

fn connected_metal_hose() -> (Document, u64, [Port; 2]) {
    let mut doc = Document::default();
    let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
    let mut ends = Vec::new();
    for (x, joint) in [(200., "right"), (700., "left")] {
        let id = doc.next_id();
        doc.graphics.push(placed::graphic(
            id,
            builtin::find("reshiki/compression-union-1-4").unwrap(),
            Point::new(x, 200.),
            scale,
        ));
        ends.push(port(&doc, id, joint));
    }
    let ends: [Port; 2] = ends.try_into().unwrap();
    let (hose, matched) = hose::place_hose(
        &mut doc,
        flexible_metal_hose(),
        ends[0].point,
        ends[1].point,
        REACH,
    );
    assert_eq!(matched.len(), 2);
    (doc, hose, ends)
}

#[test]
fn flexible_metal_tube_adapters_seat_at_compression_stops_and_survive_reshaping() {
    let (mut doc, hose, ends) = connected_metal_hose();
    let seats = doc.seats.clone();
    for (joint, socket) in ["a", "b"].into_iter().zip(&ends) {
        let end = port(&doc, hose, joint);
        assert_seated(&doc, &end, socket);
        assert_near(
            end.point,
            sub(socket.point, mul(socket.dir, socket.insertion)),
        );
        assert!(matches!(
            end.kind,
            solve::Kind::Plug(super::format::PlugFamily::Compression, size) if size == 6.35
        ));
    }
    let to = add(hose::grips(graphic(&doc, hose))[1].1, [30., 110.]);
    hose::drag(&mut doc, hose, Grip::Middle, to, REACH);
    solve::check(&mut doc, None);
    assert_eq!(doc.seats, seats);
    for (joint, socket) in ["a", "b"].into_iter().zip(&ends) {
        assert_seated(&doc, &port(&doc, hose, joint), socket);
    }
    let g = graphic(&doc, hose);
    assert!(g.hit(Point::new(to[0] as f32, to[1] as f32), 0.1));
    let plan = placed::plan(g);
    let braid = plan
        .iter()
        .find(|p| p.pass == super::plan::Pass::Front && p.depth == 1)
        .unwrap();
    assert!(braid.path.len() > 12);
    assert!(braid.path.len() <= 3072, "braid complexity stays bounded");
    let max_y = super::geometry::flatten(&braid.path)
        .iter()
        .flat_map(|(points, _)| points)
        .map(|p| p[1])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(max_y >= to[1] - 13.5 * f64::from(g.axis_x.x).abs());

    let bytes = doc.file_json().unwrap();
    let reopened = Document::from_json(&bytes).unwrap();
    assert_eq!(reopened.seats, seats);
    assert_eq!(
        graphic(&reopened, hose).piece.as_ref().unwrap().centre_line,
        g.piece.as_ref().unwrap().centre_line
    );
    assert_eq!(
        placed::plan(graphic(&reopened, hose)).as_ref(),
        plan.as_ref()
    );
}

#[test]
fn dragging_a_metal_hose_end_releases_and_reseats_at_the_internal_stop() {
    let (mut doc, hose, ends) = connected_metal_hose();
    let away = add(ends[1].point, [80., 90.]);
    assert!(hose::drag(&mut doc, hose, Grip::End(1), away, REACH).is_none());
    assert_eq!(doc.seats.len(), 1);
    assert_near(port(&doc, hose, "b").point, away);
    assert_seated(&doc, &port(&doc, hose, "a"), &ends[0]);
    // Both the visible mouth and the inserted tip are valid snap targets.
    let inserted = sub(ends[1].point, mul(ends[1].dir, ends[1].insertion));
    assert!(hose::drag(&mut doc, hose, Grip::End(1), inserted, REACH).is_some());
    solve::check(&mut doc, None);
    assert_eq!(doc.seats.len(), 2);
    assert_seated(&doc, &port(&doc, hose, "b"), &ends[1]);
}

#[test]
fn metal_hose_tips_need_enough_straight_length_for_the_compression_stop() {
    let (doc, hose, ends) = connected_metal_hose();
    let tip = port(&doc, hose, "a");
    assert!((tip.contact_length / tip.mm - 18.).abs() < 1e-6);
    let mut socket = ends[0].clone();
    socket.insertion = 18. * socket.mm;
    assert!(solve::mates(&tip, &socket));
    socket.insertion = 18.1 * socket.mm;
    assert!(!solve::mates(&tip, &socket));
}

#[test]
fn metal_hose_tips_do_not_get_rubber_cuffs_and_free_clicks_leave_room_for_collars() {
    let mut doc = Document::default();
    let d = flexible_metal_hose();
    let (id, matched) = hose::place_hose(&mut doc, d.clone(), [0., 0.], [0., 0.], REACH);
    assert!(matched.is_empty());
    let g = graphic(&doc, id);
    let line = &g.piece.as_ref().unwrap().centre_line;
    let fitting = d.tube.as_ref().unwrap().end_fittings.as_ref().unwrap();
    assert!(len(sub(line[0], *line.last().unwrap())) > 2. * fitting.length_mm());
    assert!(placed::sampled(line).iter().all(|p| p[1].abs() < 1e-6));
    let free_plan = placed::plan(g);
    let g = doc.graphics.iter_mut().find(|g| g.id == id).unwrap();
    g.piece.as_mut().unwrap().seated.insert("a".into());
    let seated_plan = placed::plan(g);
    assert_eq!(free_plan.as_ref(), seated_plan.as_ref());
    assert_eq!(
        free_plan
            .iter()
            .filter(|p| p.pass == super::plan::Pass::Back)
            .count(),
        2
    );
}

#[test]
fn flexible_hose_parts_lists_report_end_connections_without_inventing_cut_lengths() {
    let (mut doc, hose, ends) = connected_metal_hose();
    let report = super::parts_list::parts_list(&doc, &[hose]).unwrap();
    assert_eq!(report.len(), 1);
    let row = &report[0];
    assert!(row.tube.is_none());
    let assembly = row.hose.as_ref().unwrap();
    assert_eq!(assembly.interfaces, ["Ø6.35 mm compression male"; 2]);
    for (reported, socket) in assembly.ends.iter().zip(&ends) {
        let connection = reported.connection.as_ref().unwrap();
        assert_eq!(connection.id, socket.at.piece);
        assert_eq!(connection.port, socket.at.joint);
    }
    assert_eq!(
        row.variant["catalog_assembly_length"],
        "36 in (914.4 mm) catalog assembly"
    );
    assert_eq!(
        super::parts_list::cut_length_mm(&flexible_metal_hose(), &Default::default()).unwrap(),
        None
    );
    hose::drag(&mut doc, hose, Grip::Middle, [450., 500.], REACH);
    assert_eq!(
        super::parts_list::parts_list(&doc, &[hose]).unwrap(),
        report
    );
}

#[test]
fn optional_braid_and_end_fittings_keep_legacy_definitions_unchanged_and_validate_dimensions() {
    let old = builtin::find("reshiki/hose-7").unwrap();
    let json = serde_json::to_value(old.as_ref()).unwrap();
    assert!(json["tube"].get("braid").is_none());
    assert!(json["tube"].get("end_fittings").is_none());
    let tube = old.tube.as_ref().unwrap();
    assert!(tube.braid.is_none() && tube.end_fittings.is_none());
    let mut invalid = (*flexible_metal_hose()).clone();
    invalid
        .tube
        .as_mut()
        .unwrap()
        .braid
        .as_mut()
        .unwrap()
        .pitch_mm = 0.;
    assert!(
        super::validate::validate(&invalid)
            .unwrap_err()
            .contains("positive pitch")
    );
    let mut invalid = (*flexible_metal_hose()).clone();
    invalid
        .tube
        .as_mut()
        .unwrap()
        .end_fittings
        .as_mut()
        .unwrap()
        .collar_width_mm = 1.;
    assert!(
        super::validate::validate(&invalid)
            .unwrap_err()
            .contains("at least as wide")
    );
    let mut invalid = (*flexible_metal_hose()).clone();
    if let super::format::Joint::Plug { family, .. } = &mut invalid.joints[0] {
        *family = super::format::PlugFamily::Hose;
    }
    assert!(
        super::validate::validate(&invalid)
            .unwrap_err()
            .contains("male compression joint")
    );
}
