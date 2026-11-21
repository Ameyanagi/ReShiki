//! Holding: clamps on necks, clamps on stand rods, the ground lock, the
//! painted depth of an assembly, and joint clips.
use super::builtin;
use super::eval::Value;
use super::format::{Gender, HoldKind, Joint, PlugFamily};
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

#[test]
fn the_fixed_support_ring_accepts_only_its_bore_and_still_mounts_on_a_rod() {
    let mut doc = Document::default();
    let ring = add(&mut doc, "reshiki/ring", Point::default());
    let tube = add(&mut doc, "reshiki/jy-schlenk-tube", Point::default());
    let stand = add(&mut doc, "reshiki/stand", Point::default());
    let jaw = port(&doc, ring, "ring");
    assert!(!solve::mates(&jaw, &port(&doc, tube, "body")));
    for id in ["reshiki/sep-funnel-15", "reshiki/sep-funnel-29"] {
        let mut assembly = doc.clone();
        let funnel = add(&mut assembly, id, Point::new(80., 100.));
        solve::connect(
            &mut assembly,
            &[funnel],
            &joint(ring, "ring"),
            &joint(funnel, "ring-seat"),
        )
        .unwrap_or_else(|| panic!("{id} fits the support ring"));
        assembly.validate().unwrap();
        assert_eq!(assembly.seats.len(), 1);
    }
    assert!(solve::mates(
        &port(&doc, ring, "boss"),
        &port(&doc, stand, "rod")
    ));
}

#[test]
fn a_stir_bar_seats_in_the_built_in_flasks_that_have_a_stir_port() {
    for definition in stir_flasks() {
        for pt_per_mm in [0.4, 0.2] {
            let (doc, flask, bar) = stir_assembly(definition, pt_per_mm);
            assert_eq!(doc.seats.len(), 1, "{} at {pt_per_mm}", definition.id);
            let (off, turn) = residual(&port(&doc, flask, "stir"), &port(&doc, bar, "seat"));
            assert!(off < 0.01 && turn < 0.01, "{}", definition.id);
            let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
            assert_eq!(back.seats, doc.seats, "{} at {pt_per_mm}", definition.id);
        }
    }
}

fn stir_flasks() -> impl Iterator<Item = &'static std::sync::Arc<super::format::Definition>> {
    builtin::catalogue().iter().filter(|d| {
        d.joints.iter().any(|j| {
            matches!(
                j,
                Joint::Plug {
                    family: PlugFamily::Stir,
                    gender: Gender::F,
                    ..
                }
            )
        })
    })
}

fn stir_assembly(
    definition: &std::sync::Arc<super::format::Definition>,
    pt_per_mm: f32,
) -> (Document, u64, u64) {
    let mut doc = Document::default();
    let scale = placed::world_per_mm(&doc, pt_per_mm);
    let flask = doc.next_id();
    doc.graphics.push(placed::graphic(
        flask,
        definition.clone(),
        Point::new(250., 300.),
        scale,
    ));
    let bar = doc.next_id();
    doc.graphics.push(placed::graphic(
        bar,
        builtin::find("reshiki/stir-bar").unwrap(),
        Point::new(80., 120.),
        scale,
    ));
    solve::connect(&mut doc, &[bar], &joint(flask, "stir"), &joint(bar, "seat"))
        .unwrap_or_else(|| panic!("{} accepts the stir bar", definition.id));
    (doc, flask, bar)
}

#[test]
fn seated_stir_bar_bodies_and_spin_ink_clear_every_registered_flask_shell() {
    use super::format::{Layer, Role};
    use super::geometry::{Affine, Seg, distance_to_outline, flatten, inside, parse_path};
    use crate::graphics::PathCommand;
    use crate::scene::Primitive;

    for definition in stir_flasks() {
        // The main body precedes the necks and valve artwork. Its closed
        // outline is the shell the stir bar must stay inside.
        let body = definition
            .shapes
            .iter()
            .find(|s| s.role == Role::Glass && s.layer == Layer::Body)
            .unwrap();
        let shell = flatten(&parse_path(&body.path).unwrap());
        for pt_per_mm in [0.4, 0.2] {
            let (mut doc, flask, bar) = stir_assembly(definition, pt_per_mm);
            let g = graphic(&doc, flask);
            let local = Affine([
                f64::from(g.axis_x.x),
                f64::from(g.axis_x.y),
                f64::from(g.axis_y.x),
                f64::from(g.axis_y.y),
                f64::from(g.origin.x),
                f64::from(g.origin.y),
            ])
            .inverse()
            .unwrap();
            let point = |p: Point| local.apply([f64::from(p.x), f64::from(p.y)]);
            for spin in ["off", "on"] {
                solve::adjust::set(&mut doc, bar, "spin", Value::State(spin.into())).unwrap();
                for primitive in placed::primitives(graphic(&doc, bar), doc.canvas_theme) {
                    let Primitive::Path {
                        commands, style, ..
                    } = primitive
                    else {
                        panic!("the stir bar paints as paths");
                    };
                    let path: Vec<_> = commands
                        .iter()
                        .map(|c| match *c {
                            PathCommand::Move(p) => Seg::Move(point(p)),
                            PathCommand::Line(p) => Seg::Line(point(p)),
                            PathCommand::Cubic(a, b, p) => Seg::Cubic(point(a), point(b), point(p)),
                            PathCommand::Close => Seg::Close,
                        })
                        .collect();
                    // Native strokes stay the same point width when a setup
                    // shrinks. Check the painted ink, not only path centres.
                    let half_stroke = f64::from(style.width_pt / pt_per_mm) / 2.;
                    for p in flatten(&path).into_iter().flat_map(|(points, _)| points) {
                        let gap = distance_to_outline(p, &shell) - half_stroke;
                        assert!(
                            inside(p, &shell) && gap >= 0.05,
                            "{} at {pt_per_mm}, spin {spin}: {p:?}, ink gap {gap} mm",
                            definition.id
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn an_embedded_stir_seat_keeps_its_saved_height_when_the_catalogue_changes() {
    let current = builtin::find("reshiki/jy-storage-flask-25").unwrap();
    let mut earlier = (*current).clone();
    let Joint::Plug { at, .. } = earlier
        .joints
        .iter_mut()
        .find(|j| j.id() == "stir")
        .unwrap()
    else {
        panic!("the flask has a stir plug");
    };
    at[1] = 16.53916;
    assert_ne!(placed::key(&earlier), placed::key(&current));
    let (doc, flask, bar) = stir_assembly(&std::sync::Arc::new(earlier), 0.4);
    let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(back.seats, doc.seats);
    assert_eq!(graphic(&back, bar).origin, graphic(&doc, bar).origin);
    let saved = graphic(&back, flask)
        .piece
        .as_ref()
        .unwrap()
        .linked
        .as_ref()
        .unwrap();
    assert_eq!(
        saved.joints.iter().find(|j| j.id() == "stir").unwrap().at()[1],
        16.53916
    );
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
fn new_clamp_contacts_fit_inside_supported_necks_at_both_ends() {
    for piece in [
        "reshiki/rbf-50-15",
        "reshiki/rbf-100-15",
        "reshiki/rbf-100-29",
        "reshiki/rbf-300-29",
        "reshiki/rbf-500-29",
        "reshiki/schlenk-flask-100-15",
        "reshiki/jy-schlenk-tube",
    ] {
        let mut doc = Document::default();
        let held = add(&mut doc, piece, Point::new(300., 400.));
        let clamp = add(&mut doc, "reshiki/clamp-3finger", Point::new(100., 100.));
        let name = if piece == "reshiki/jy-schlenk-tube" {
            "neck"
        } else {
            "neck-grip"
        };
        let grip = port(&doc, held, name);
        let held_at = graphic(&doc, held).origin;
        for end in [0., 1.] {
            // A drag near either end must leave room for both jaw contacts.
            let jaw = port(&doc, clamp, "jaw");
            let at = along(&grip, end);
            let centre = along(&jaw, 0.5);
            doc.translate(
                &[clamp],
                (at[0] - centre[0]) as f32 + 2.,
                (at[1] - centre[1]) as f32,
            );
            let snap = solve::snap(&mut doc, &[clamp], 14., false).unwrap();
            assert!(matches!(snap.offer, Offer::Seat), "{piece}");
            assert_eq!(snap.target.at, joint(held, name));
            let jaw = port(&doc, clamp, "jaw");
            assert!((jaw.contact_length / jaw.mm - 8.).abs() < 1e-6);
            let centre = along(&jaw, 0.5);
            let t = ((centre[0] - grip.point[0]) * grip.dir[0]
                + (centre[1] - grip.point[1]) * grip.dir[1])
                / grip.mm;
            assert!(t >= 4. - 1e-4, "{piece}: {t}");
            assert!(t <= grip.length / grip.mm - 4. + 1e-4, "{piece}: {t}");
            assert_eq!(doc.seats.len(), 1);
            assert_eq!(graphic(&doc, held).origin, held_at);
        }
        let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(back.seats, doc.seats, "{piece}");
    }
}

#[test]
fn new_clamp_rejects_short_top_edges_in_snap_connect_and_jaw_closing() {
    for (piece, name) in [
        ("reshiki/liebig-15", "top-neck"),
        ("reshiki/liebig-29", "top-neck"),
        ("reshiki/schlenk-tube-15", "neck-grip"),
        ("reshiki/cow-15", "inlet-neck"),
        ("reshiki/exp-15-29", "top-neck"),
        ("reshiki/red-29-15", "top-neck"),
    ] {
        let mut doc = Document::default();
        let held = add(&mut doc, piece, Point::new(300., 400.));
        let clamp = add(&mut doc, "reshiki/clamp-3finger", Point::new(100., 100.));
        let grip = port(&doc, held, name);
        let jaw = port(&doc, clamp, "jaw");
        assert!(!solve::mates(&grip, &jaw), "{piece}");
        assert!(
            solve::options(&doc, &[clamp])
                .iter()
                .all(|(s, _, _)| s.at != grip.at)
        );
        assert!(solve::connect(&mut doc, &[clamp], &grip.at, &jaw.at).is_none());
        let at = along(&grip, 0.5);
        bring(&mut doc, clamp, "jaw", at, (0., 0.));
        let mut snapped = doc.clone();
        solve::snap(&mut snapped, &[clamp], 1., false);
        assert!(solve::partner(&snapped, &grip.at).is_none(), "{piece}");
        solve::adjust::set(&mut doc, clamp, "jaw", Value::State("open".into())).unwrap();
        solve::adjust::set(&mut doc, clamp, "jaw", Value::State("closed".into())).unwrap();
        assert!(solve::partner(&doc, &grip.at).is_none(), "{piece}");
    }
}

#[test]
fn legacy_clamps_and_authored_jaws_keep_their_saved_hold_semantics() {
    let original =
        super::format::parse(include_str!("fixtures/clamp-3finger.reshiki-piece")).unwrap();
    for (id, revision) in [
        ("reshiki/clamp-3finger", 1),
        ("reshiki/clamp-3finger", 2),
        ("lab/clamp", 3),
    ] {
        let mut doc = Document::default();
        let held = add(&mut doc, "reshiki/liebig-29", Point::new(300., 400.));
        let mut definition = original.clone();
        definition.id = id.into();
        definition.revision = revision;
        let clamp = doc.next_id();
        let scale = placed::world_per_mm(&doc, placed::DEFAULT_PT_PER_MM);
        doc.graphics.push(placed::graphic(
            clamp,
            std::sync::Arc::new(definition),
            Point::new(100., 100.),
            scale,
        ));
        let grip = port(&doc, held, "top-neck");
        let jaw = port(&doc, clamp, "jaw");
        assert_eq!(jaw.contact_length, 0.);
        solve::connect(&mut doc, &[clamp], &grip.at, &jaw.at).unwrap();
        assert_eq!(doc.seats.len(), 1);
        let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
        assert_eq!(back.seats, doc.seats, "{id}@{revision}");
    }
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
    // Another stand and a loose flask selected with them still move.
    let other = add(&mut doc, "reshiki/stand", Point::new(-900., 0.));
    let loose = add(&mut doc, "reshiki/rbf-100-15", Point::new(900., 900.));
    let mut both = solve::carried(&doc, &[flask, other, loose], &[flask]);
    both.sort();
    let mut want = vec![clamp, flask, other, loose];
    want.sort();
    assert_eq!(both, want);
    let mut both = solve::carried(&doc, &[loose, other], &[loose]);
    both.sort();
    assert_eq!(both, [other, loose]);
    doc.delete(&[other, loose]);
    // Two clicked pieces each carry what they hold.
    assert_eq!(solve::carried(&doc, &all, &[flask, stand]).len(), 3);
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
    assert!(placed::on_top(&doc, clamp, at(3.), 0.05));
    assert!(!placed::on_top(&doc, flask, at(3.), 0.05));
    assert!(placed::on_top(&doc, flask, at(-3.), 0.05));
    assert!(!placed::on_top(&doc, clamp, at(-3.), 0.05));
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
    // Moving and turning only the two pieces brings the clip along.
    let before = doc.clone();
    doc.translate(&[flask, condenser], 50., 20.);
    crate::editing::transform_about(
        &mut doc,
        &[flask, condenser],
        Point::new(300., 300.),
        1.,
        90.,
    );
    commit(&mut doc, before);
    assert!(on_mouth(&doc));
    let (c, f) = (graphic(&doc, clip), graphic(&doc, flask));
    for (a, b) in [(c.axis_x, f.axis_x), (c.axis_y, f.axis_y)] {
        assert!(a.distance(b) < 1e-4, "{a:?} {b:?}");
    }
    assert!(f.axis_x.y.abs() > 0.5, "the flask turned");
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
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(300., -200.));
    let (s, m, _) = solve::options(&doc, &[condenser]).remove(0);
    solve::connect(&mut doc, &[condenser], &s.at, &m.at).unwrap();
    let seats = doc.seats.len();
    let to = along(&port(&doc, condenser, "jacket"), 0.5);
    bring(&mut doc, held, "jaw", to, (2., 0.));
    let both = solve::closure(&doc, &[other]);
    let other_at = graphic(&doc, other).origin;
    assert!(solve::snap(&mut doc, &both, 14., false).is_none());
    assert_eq!(doc.seats.len(), seats);
    assert_eq!(graphic(&doc, other).origin, other_at);
}

#[test]
fn a_bad_clip_reference_deletes_nothing_and_a_clip_copied_alone_stays_a_clip() {
    let (mut doc, flask, _) = reflux();
    let ids = solve::closure(&doc, &[flask]);
    solve::add_clips(&mut doc, &ids);
    let clip = doc.seats[0].clip.unwrap();
    // A clip copied without its joint comes as a loose clip.
    let part = crate::editing::selection(&doc, &[clip]);
    assert_eq!(part.graphics.len(), 1);
    assert!(part.seats.is_empty());
    // A seat that no longer fits takes with it only a clip of its size.
    let flask_too = add(&mut doc, "reshiki/rbf-100-15", Point::new(900., 900.));
    let small = add(&mut doc, "reshiki/keck-clip-15", Point::new(900., 0.));
    let file: serde_json::Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    let reopen = |named: u64| {
        let mut json = file.clone();
        json["seats"][0]["clip"] = serde_json::json!(named);
        json["seats"][0]["a"]["joint"] = serde_json::json!("nowhere");
        let back = Document::from_json(json.to_string().as_bytes()).unwrap();
        assert!(back.seats.is_empty());
        back.graphics.iter().any(|g| g.id == named)
    };
    assert!(reopen(flask_too));
    assert!(reopen(small));
    assert!(!reopen(clip));
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

#[test]
fn two_stands_are_never_offered_to_each_other_or_joined_by_a_group() {
    // Stand A with an empty clamp; stand B with a clamp on a condenser's jacket.
    let mut doc = Document::default();
    let a = add(&mut doc, "reshiki/stand", Point::new(0., 0.));
    let clamp_a = add(&mut doc, "reshiki/clamp-3finger", Point::new(300., 0.));
    let b = add(&mut doc, "reshiki/stand", Point::new(-800., 0.));
    let clamp_b = add(&mut doc, "reshiki/clamp-3finger", Point::new(-500., 0.));
    let condenser = add(&mut doc, "reshiki/liebig-15", Point::new(-300., 0.));
    for (clamp, stand) in [(clamp_a, a), (clamp_b, b)] {
        let rod = along(&port(&doc, stand, "rod"), 0.5);
        bring(&mut doc, clamp, "boss", rod, (0., 0.));
        solve::snap(&mut doc, &[clamp], 14., false).unwrap();
    }
    let jaw = along(&port(&doc, clamp_b, "jaw"), 0.5);
    bring(&mut doc, condenser, "jacket", jaw, (0., 0.));
    solve::snap(&mut doc, &[condenser], 14., false).unwrap();
    assert_eq!(doc.seats.len(), 3);
    let side_a = solve::closure(&doc, &[a]);
    assert!(
        solve::options(&doc, &side_a)
            .iter()
            .all(|(s, ..)| s.at.piece != condenser)
    );
    // A loose flask grouped with stand A does not come to clamp A, since
    // the group would bring the stand along.
    let flask = add(&mut doc, "reshiki/rbf-100-15", Point::new(900., 0.));
    doc.groups.push(crate::grouping::Group {
        id: doc.next_id(),
        members: vec![a, flask],
        integral: false,
        layout: None,
    });
    let jaw = along(&port(&doc, clamp_a, "jaw"), 0.5);
    bring(&mut doc, flask, "neck-grip", jaw, (6., 0.));
    let a_at = graphic(&doc, a).origin;
    let held = solve::carried(&doc, &[clamp_a], &[clamp_a]);
    assert!(solve::snap(&mut doc, &held, 14., false).is_none());
    assert_eq!(graphic(&doc, a).origin, a_at);
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
fn table_generated_clips_inherit_the_seats_cell_and_detach_prunes_membership() {
    use crate::graphics::table::Membership;
    for on_canvas in [false, true] {
        let (mut doc, flask, condenser) = reflux();
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
        assert_eq!(solve::add_clips(&mut doc, &[flask, condenser]), 1);
        commit(&mut doc, before);
        let clip = doc.seats[0].clip.unwrap();
        let ownership = Membership::new(&doc).unwrap();
        assert_eq!(ownership.cell_of(clip), ownership.cell_of(flask));
        assert_eq!(ownership.cell_of(clip).is_none(), on_canvas);
        let mut doc = Document::from_json(&doc.file_json().unwrap()).unwrap();
        let before = doc.clone();
        assert!(solve::detach(&mut doc, &joint(flask, "neck")));
        commit(&mut doc, before);
        assert!(doc.graphics.iter().all(|g| g.id != clip));
        assert_eq!(Membership::new(&doc).unwrap().cell_of(clip), None);
        doc.validate().unwrap();
    }
}

#[test]
fn table_clip_batch_rejects_cross_cell_seats_without_adding_any() {
    let (mut doc, flask, condenser) = reflux();
    let other_flask = add(&mut doc, "reshiki/rbf-300-29", Point::new(2000., 400.));
    let other_condenser = add(&mut doc, "reshiki/liebig-29", Point::new(2000., 100.));
    solve::connect(
        &mut doc,
        &[other_condenser],
        &joint(other_flask, "neck"),
        &joint(other_condenser, "bottom"),
    )
    .unwrap();
    table_cells(
        &mut doc,
        [vec![flask, condenser, other_flask], vec![other_condenser]],
    );
    let before = doc.clone();
    assert_eq!(
        solve::add_clips(&mut doc, &[flask, condenser, other_flask, other_condenser]),
        0
    );
    assert_eq!(doc, before);
}

#[test]
fn table_ground_locked_adapter_uses_the_swapped_static_target() {
    use crate::graphics::table::Membership;
    let (mut doc, flask, clamp) = clamped();
    let stand = add(&mut doc, "reshiki/stand", Point::new(0., 0.));
    solve::connect(
        &mut doc,
        &[clamp],
        &joint(stand, "rod"),
        &joint(clamp, "boss"),
    )
    .unwrap();
    let condenser = add(&mut doc, "reshiki/liebig-29", Point::new(800., 100.));
    table_cells(&mut doc, [vec![], vec![flask, clamp, stand, condenser]]);
    doc.validate().unwrap();
    let origins: Vec<_> = [flask, clamp, stand]
        .into_iter()
        .map(|id| (id, graphic(&doc, id).origin))
        .collect();
    let before = doc.clone();
    // The selected grounded side stays put; the nominal target comes to it.
    let snap = solve::connect(
        &mut doc,
        &[stand],
        &joint(condenser, "bottom"),
        &joint(flask, "neck"),
    )
    .unwrap();
    assert_eq!(snap.target.at, joint(flask, "neck"));
    assert_eq!(snap.moving.at, joint(condenser, "bottom"));
    assert!(matches!(snap.offer, Offer::Adapter(_)));
    let adapter = snap.added.unwrap();
    let ownership = Membership::new(&doc).unwrap();
    assert_eq!(ownership.cell_of(adapter), ownership.cell_of(flask));
    assert_eq!(ownership.cell_of(adapter).unwrap().cell, 2);
    for (id, origin) in origins {
        assert_eq!(graphic(&doc, id).origin, origin);
    }
    commit(&mut doc, before);
    Document::from_json(&doc.file_json().unwrap())
        .unwrap()
        .validate()
        .unwrap();
}
