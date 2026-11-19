//! Holding: clamps on necks, clamps on stand rods, the ground lock, the
//! painted depth of an assembly, and joint clips.
use super::builtin;
use super::eval::Value;
use super::format::HoldKind;
use super::placed;
use super::solve::{self, JointRef, Offer, Port, ports, residual};
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

fn port(doc: &Document, piece: u64, name: &str) -> Port {
    let g = doc.graphics.iter().find(|g| g.id == piece).unwrap();
    ports(g).into_iter().find(|p| p.at.joint == name).unwrap()
}

fn along(p: &Port, t: f64) -> [f64; 2] {
    [
        p.point[0] + p.dir[0] * p.length * t,
        p.point[1] + p.dir[1] * p.length * t,
    ]
}

/// Move `piece`'s assembly so the middle of its hold `name` is `off` world
/// units from `at`.
fn bring(doc: &mut Document, piece: u64, name: &str, at: [f64; 2], off: (f32, f32)) {
    let c = along(&port(doc, piece, name), 0.5);
    let ids = solve::closure(doc, &[piece]);
    doc.translate(
        &ids,
        (at[0] - c[0]) as f32 + off.0,
        (at[1] - c[1]) as f32 + off.1,
    );
}

fn commit(doc: &mut Document, before: Document) {
    let _ = crate::transaction::reconcile(doc, before).unwrap();
}

fn graphic(doc: &Document, id: u64) -> &crate::graphics::Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

/// A 100 mL flask held by a clamp, and nothing else.
fn clamped() -> (Document, u64, u64) {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(300., 400.));
    let clamp = add(&mut doc, "reshiki/clamp-3finger", Point::new(100., 100.));
    let grip = along(&port(&doc, flask, "neck-grip"), 0.5);
    bring(&mut doc, clamp, "jaw", grip, (3., -2.));
    solve::snap(&mut doc, &[clamp], 14., false).unwrap();
    (doc, flask, clamp)
}

#[test]
fn a_clamp_brought_to_a_neck_holds_it_and_closes_its_fingers_on_it() {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-300-29", Point::new(300., 400.));
    let clamp = add(&mut doc, "reshiki/clamp-3finger", Point::new(100., 100.));
    let before = doc.clone();
    let grip = along(&port(&doc, flask, "neck-grip"), 0.5);
    bring(&mut doc, clamp, "jaw", grip, (4., 3.));
    let snap = solve::snap(&mut doc, &[clamp], 14., false).unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    assert_eq!(snap.target.at.piece, flask);
    assert_eq!(snap.target.hold(), Some(HoldKind::Grip));
    let (off, turn) = residual(&snap.target, &port(&doc, clamp, "jaw"));
    assert!(off < 0.01 && turn < 0.01, "{off} mm, {turn}°");
    // The fingers meet a 29/42 neck, 26 mm across.
    let params = &graphic(&doc, clamp).piece.as_ref().unwrap().params;
    assert_eq!(params.get("grip"), Some(&Value::Number(26.)));
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 1);
    let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(back.seats, doc.seats);
}

#[test]
fn a_clamp_mounts_on_the_rod_and_what_it_holds_slides_along_it_to_its_ends() {
    let (mut doc, flask, clamp) = clamped();
    let stand = add(&mut doc, "reshiki/stand", Point::new(0., 0.));
    let rod = port(&doc, stand, "rod");
    let stand_at = graphic(&doc, stand).origin;
    let before = doc.clone();
    bring(&mut doc, clamp, "boss", along(&rod, 0.3), (5., 0.));
    let ids = solve::closure(&doc, &[clamp]);
    let snap = solve::snap(&mut doc, &ids, 14., false).unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    assert_eq!(snap.target.at, joint(stand, "rod"));
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 2);
    assert_eq!(graphic(&doc, stand).origin, stand_at);
    // A drag on the flask leaves the stand; one on the stand takes it all.
    let all = solve::closure(&doc, &[flask]);
    assert_eq!(all.len(), 3);
    let mut held = solve::carried(&doc, &all, &[flask]);
    held.sort();
    assert_eq!(held, [flask, clamp]);
    assert_eq!(solve::carried(&doc, &all, &[stand]).len(), 3);
    // The rod is upright: a sideways drag goes nowhere, a slanted one
    // slides up, and a long one stops with the boss at the rod's end.
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-3;
    assert!(near(
        solve::constrain(&doc, &held, [30., 0.]).unwrap(),
        [0., 0.]
    ));
    let up = solve::constrain(&doc, &held, [7., -40.]).unwrap();
    assert!(near(up, [0., -40.]), "{up:?}");
    let far = solve::constrain(&doc, &held, [0., -1e5]).unwrap();
    let boss = along(&port(&doc, clamp, "boss"), 0.5);
    assert!((boss[1] + far[1] - along(&rod, 1.)[1]).abs() < 1e-3);
    // Nothing that stays holds the whole assembly.
    assert!(solve::constrain(&doc, &all, [7., -40.]).is_none());
    // The slide keeps both seats.
    let before = doc.clone();
    doc.translate(&held, up[0] as f32, up[1] as f32);
    solve::snap(&mut doc, &held, 14., false);
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 2);
    assert_eq!(graphic(&doc, stand).origin, stand_at);
}

#[test]
fn a_stand_brought_to_a_clamp_stays_put_and_the_clamp_comes_to_its_rod() {
    let (mut doc, flask, clamp) = clamped();
    let stand = add(&mut doc, "reshiki/stand", Point::new(-400., 0.));
    let boss = along(&port(&doc, clamp, "boss"), 0.5);
    let rod = port(&doc, stand, "rod");
    let at = along(&rod, 0.5);
    doc.translate(
        &[stand],
        (boss[0] - at[0]) as f32 - 6.,
        (boss[1] - at[1]) as f32,
    );
    let stand_at = graphic(&doc, stand).origin;
    let flask_at = graphic(&doc, flask).origin;
    let snap = solve::snap(&mut doc, &[stand], 14., false).unwrap();
    assert!(matches!(snap.offer, Offer::Seat));
    assert_eq!(snap.target.at, joint(stand, "rod"));
    assert_eq!(snap.moving.at, joint(clamp, "boss"));
    assert_eq!(graphic(&doc, stand).origin, stand_at);
    assert!((graphic(&doc, flask).origin.x - flask_at.x + 6.).abs() < 1e-3);
    assert_eq!(doc.seats.len(), 2);
}

#[test]
fn one_rod_holds_two_clamps_and_detaching_one_pulls_it_off_sideways() {
    let mut doc = Document::default();
    let stand = add(&mut doc, "reshiki/stand", Point::new(0., 0.));
    let low = add(&mut doc, "reshiki/clamp-3finger", Point::new(300., 0.));
    let high = add(&mut doc, "reshiki/clamp-3finger", Point::new(300., -200.));
    let rod = port(&doc, stand, "rod");
    let before = doc.clone();
    for (clamp, t) in [(low, 0.2), (high, 0.6)] {
        bring(&mut doc, clamp, "boss", along(&rod, t), (3., 0.));
        solve::snap(&mut doc, &[clamp], 14., false).unwrap();
    }
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 2);
    assert_eq!(solve::partners(&doc, &joint(stand, "rod")).len(), 2);
    let x = graphic(&doc, high).origin.x;
    assert!(solve::detach(&mut doc, &joint(high, "boss")));
    assert_eq!(doc.seats.len(), 1);
    assert!(graphic(&doc, high).origin.x > x + 10.);
}

#[test]
fn an_assembly_paints_the_fingers_around_the_neck_and_hits_what_shows() {
    let (doc, flask, clamp) = clamped();
    let first = doc.graphics.iter().find(|g| g.piece.is_some()).unwrap();
    let other = doc.graphics.iter().rfind(|g| g.piece.is_some()).unwrap();
    let alone = placed::primitives(first, doc.canvas_theme).len()
        + placed::primitives(other, doc.canvas_theme).len();
    assert_eq!(placed::painted(first, &doc).len(), alone);
    assert!(placed::painted(other, &doc).is_empty());
    // The front finger crosses the neck below the jaw's middle; above it
    // the back fingers pass behind the glass.
    let jaw = port(&doc, clamp, "jaw");
    let at = |mm: f64| {
        let c = along(&jaw, 0.5);
        Point::new(
            (c[0] + jaw.dir[0] * mm * jaw.mm) as f32,
            (c[1] + jaw.dir[1] * mm * jaw.mm) as f32,
        )
    };
    assert!(placed::on_top(&doc, clamp, at(5.), 0.05));
    assert!(!placed::on_top(&doc, flask, at(5.), 0.05));
    assert!(placed::on_top(&doc, flask, at(-7.5), 0.05));
    assert!(!placed::on_top(&doc, clamp, at(-7.5), 0.05));
}

/// A 300 mL flask with a 29/42 condenser seated in it.
fn reflux() -> (Document, u64, u64) {
    let mut doc = Document::default();
    let flask = add(&mut doc, "reshiki/rbf-300-29", Point::new(300., 400.));
    let condenser = add(&mut doc, "reshiki/liebig-29", Point::new(300., 100.));
    let ids = solve::options(&doc, &[condenser]);
    let (s, m, _) = ids.first().unwrap();
    solve::connect(&mut doc, &[condenser], &s.at.clone(), &m.at.clone()).unwrap();
    (doc, flask, condenser)
}

#[test]
fn add_clips_puts_one_on_each_joint_and_it_follows_its_seat_until_released() {
    let (mut doc, flask, condenser) = reflux();
    let ids = solve::closure(&doc, &[flask]);
    assert_eq!(solve::unclipped(&doc, &ids), 1);
    let before = doc.clone();
    assert_eq!(solve::add_clips(&mut doc, &ids), 1);
    commit(&mut doc, before);
    let clip = doc.seats[0].clip.unwrap();
    assert_eq!(solve::unclipped(&doc, &solve::closure(&doc, &[flask])), 0);
    let on_mouth = |doc: &Document| {
        let mouth = port(doc, flask, "neck").point;
        let o = graphic(doc, clip).origin;
        (f64::from(o.x) - mouth[0]).hypot(f64::from(o.y) - mouth[1]) < 1e-3
    };
    assert!(on_mouth(&doc));
    // The clip is part of the assembly, in its layer and next to it.
    let all = solve::closure(&doc, &[condenser]);
    assert!(all.contains(&clip));
    assert!(doc.expand_groups(&[clip]).contains(&flask));
    // Moving only the two pieces brings the clip along to the new mouth.
    let before = doc.clone();
    doc.translate(&[flask, condenser], 50., 20.);
    commit(&mut doc, before);
    assert!(on_mouth(&doc));
    assert_eq!(graphic(&doc, clip).axis_x, graphic(&doc, flask).axis_x);
    let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(back.seats[0].clip, Some(clip));
    // Copying the assembly copies its clip with it.
    let part = crate::editing::selection(&doc, &all);
    let before = doc.clone();
    let pasted = crate::editing::append(&mut doc, &part, Point::new(400., 0.));
    commit(&mut doc, before);
    assert!(pasted.contains(&doc.seats[1].clip.unwrap()));
    // Releasing the seat takes its clip off the drawing.
    let before = doc.clone();
    assert!(solve::detach(&mut doc, &joint(flask, "neck")));
    commit(&mut doc, before);
    assert!(doc.graphics.iter().all(|g| g.id != clip));
}

#[test]
fn a_clip_dropped_near_a_joint_of_its_size_goes_on_it_and_deleting_either_frees_the_other() {
    let (mut doc, flask, _) = reflux();
    let mouth = port(&doc, flask, "neck").point;
    let near = Point::new(mouth[0] as f32 + 3., mouth[1] as f32 + 2.);
    let small = add(&mut doc, "reshiki/keck-clip-15", near);
    assert!(solve::snap(&mut doc, &[small], 14., true).is_none());
    let clip = add(&mut doc, "reshiki/keck-clip-29", near);
    let snap = solve::snap(&mut doc, &[clip], 14., true).unwrap();
    assert!(matches!(snap.offer, Offer::Clip));
    assert_eq!(doc.seats[0].clip, Some(clip));
    // Deleting the clip leaves the seat; deleting the flask takes the clip.
    let mut gone = doc.clone();
    gone.delete(&[clip]);
    assert_eq!(gone.seats.len(), 1);
    assert_eq!(gone.seats[0].clip, None);
    doc.delete(&[flask]);
    assert!(doc.seats.is_empty());
    assert!(doc.graphics.iter().all(|g| g.id != clip));
}

#[test]
fn a_clamp_slid_along_the_rod_brings_a_loose_flask_to_it_and_two_stands_never_join() {
    let mut doc = Document::default();
    let stand = add(&mut doc, "reshiki/stand", Point::new(0., 0.));
    let clamp = add(&mut doc, "reshiki/clamp-3finger", Point::new(300., 0.));
    let rod = port(&doc, stand, "rod");
    bring(&mut doc, clamp, "boss", along(&rod, 0.5), (0., 0.));
    solve::snap(&mut doc, &[clamp], 14., false).unwrap();
    // A loose flask whose neck is beside the jaw, a little lower.
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(300., 400.));
    let jaw = along(&port(&doc, clamp, "jaw"), 0.5);
    bring(&mut doc, flask, "neck-grip", jaw, (6., 30.));
    let stand_at = graphic(&doc, stand).origin;
    let flask_at = graphic(&doc, flask).origin;
    // Slide the clamp down the rod to it: the flask comes to the jaw, and
    // the clamp stays on the rod.
    let held = solve::carried(&doc, &[clamp], &[clamp]);
    let d = solve::constrain(&doc, &held, [3., 30.]).unwrap();
    doc.translate(&held, d[0] as f32, d[1] as f32);
    let clamp_at = graphic(&doc, clamp).origin;
    let snap = solve::snap(&mut doc, &held, 14., false).unwrap();
    assert_eq!(snap.moving.at, joint(flask, "neck-grip"));
    assert_eq!(graphic(&doc, clamp).origin, clamp_at);
    assert_eq!(graphic(&doc, stand).origin, stand_at);
    assert!((graphic(&doc, flask).origin.x - flask_at.x + 6.).abs() < 1e-3);
    let before = doc.clone();
    commit(&mut doc, before);
    assert_eq!(doc.seats.len(), 2);
    // A second stand brought to the mounted clamp's flask stays apart.
    let other = add(&mut doc, "reshiki/stand", Point::new(-600., 0.));
    let held = add(&mut doc, "reshiki/clamp-3finger", Point::new(-300., 0.));
    let rod2 = port(&doc, other, "rod");
    bring(&mut doc, held, "boss", along(&rod2, 0.5), (0., 0.));
    solve::snap(&mut doc, &[held], 14., false).unwrap();
    let seats = doc.seats.len();
    let to = along(&port(&doc, flask, "neck-neck"), 0.5);
    bring(&mut doc, held, "jaw", to, (2., 0.));
    let both = solve::closure(&doc, &[other]);
    let other_at = graphic(&doc, other).origin;
    assert!(solve::snap(&mut doc, &both, 14., false).is_none());
    assert_eq!(doc.seats.len(), seats);
    assert_eq!(graphic(&doc, other).origin, other_at);
}

#[test]
fn a_bad_clip_reference_deletes_nothing_and_a_clip_copied_alone_stays_a_clip() {
    let (mut doc, flask, condenser) = reflux();
    let ids = solve::closure(&doc, &[flask]);
    solve::add_clips(&mut doc, &ids);
    let clip = doc.seats[0].clip.unwrap();
    // A clip copied without its joint comes as a loose clip.
    let part = crate::editing::selection(&doc, &[clip]);
    assert_eq!(part.graphics.len(), 1);
    assert!(part.seats.is_empty());
    // A seat that names another graphic as its clip and no longer fits
    // takes only its seat when it goes.
    let flask_too = add(&mut doc, "reshiki/rbf-100-15", Point::new(900., 900.));
    let mut json: serde_json::Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    json["seats"][0]["clip"] = serde_json::json!(flask_too);
    json["seats"][0]["a"]["joint"] = serde_json::json!("nowhere");
    let back = Document::from_json(json.to_string().as_bytes()).unwrap();
    assert!(back.seats.is_empty());
    assert!(back.graphics.iter().any(|g| g.id == flask_too));
    assert!(back.graphics.iter().any(|g| g.id == condenser));
}

#[test]
fn a_loose_clip_put_on_a_joint_joins_its_assembly_layer_and_place() {
    let (mut doc, flask, _) = reflux();
    for g in &mut doc.graphics {
        g.layer = 3;
    }
    // A clip placed before the flask, behind the structures.
    let early = add(&mut doc, "reshiki/keck-clip-29", Point::new(900., 900.));
    let clip = doc.graphics.pop().unwrap();
    doc.graphics.insert(0, clip);
    let mouth = port(&doc, flask, "neck").point;
    let before = doc.clone();
    let c = placed::middle(graphic(&doc, early));
    doc.translate(&[early], mouth[0] as f32 - c.x + 2., mouth[1] as f32 - c.y);
    assert!(matches!(
        solve::snap(&mut doc, &[early], 14., false).unwrap().offer,
        Offer::Clip
    ));
    commit(&mut doc, before);
    assert!(doc.graphics.iter().all(|g| g.layer == 3));
    assert_eq!(doc.graphics.last().unwrap().id, early);
}
