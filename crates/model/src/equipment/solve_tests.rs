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
