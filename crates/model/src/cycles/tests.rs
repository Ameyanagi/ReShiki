use super::{Role, Target, center_of, drawn, drop_target, make, remove, reverse, set_radius};
use crate::{
    arrows::Head,
    document::{Annotation, DRAWING_ELEMENTS, Document, Point},
    typography::TextFormat,
};
use std::f32::consts::TAU;

fn text(doc: &mut Document, at: Point, text: &str) -> u64 {
    let id = doc.next_id();
    doc.annotations.push(Annotation {
        angle: 0.,
        id,
        position: at,
        text: text.into(),
        format: TextFormat {
            style: doc.drawing_style.text_style(),
            ..Default::default()
        },
    });
    id
}

fn ink(doc: &Document, id: u64) -> (Point, Point) {
    crate::scene::selection_bounds(doc, &[id]).unwrap()
}

/// The Suzuki cycle's three palladium intermediates, made into a cycle.
fn suzuki() -> (Document, u64, [u64; 3]) {
    let mut doc = Document::default();
    let pd0 = text(&mut doc, Point::new(0., -200.), "Pd(0)L2");
    let oa = text(&mut doc, Point::new(170., 90.), "Ar–Pd(II)L2–X");
    let tm = text(&mut doc, Point::new(-230., 90.), "Ar–Pd(II)L2–Ar′");
    let (id, _) = make(&mut doc, &[tm, oa, pd0]).unwrap();
    doc.validate().unwrap();
    (doc, id, [pd0, oa, tm])
}

fn cycle(doc: &Document, id: u64) -> &super::Cycle {
    doc.graphics
        .iter()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref())
        .unwrap()
}

fn graphic(doc: &Document, id: u64) -> &crate::graphics::Graphic {
    doc.graphics.iter().find(|g| g.id == id).unwrap()
}

#[test]
fn make_orders_intermediates_clockwise_from_the_top_and_groups_them() {
    let (doc, id, [pd0, oa, tm]) = suzuki();
    let c = cycle(&doc, id);
    assert_eq!(c.intermediates, vec![vec![pd0], vec![oa], vec![tm]]);
    assert_eq!(c.steps.len(), 3);
    // They sit on the circle now, each as far from its center.
    let (center, radius) = graphic(&doc, id).cycle_circle();
    for t in [pd0, oa, tm] {
        let (lo, hi) = ink(&doc, t);
        assert!((lo.lerp(hi, 0.5).distance(center) - radius).abs() < 0.5);
    }
    let group = doc.groups.iter().find(|g| g.members.contains(&id)).unwrap();
    assert!([pd0, oa, tm].iter().all(|t| group.members.contains(t)));
    assert_eq!(doc.file_version(), DRAWING_ELEMENTS);
}

#[test]
fn a_saved_cycle_opens_unchanged() {
    let (doc, id, _) = suzuki();
    let back = Document::from_json(&doc.file_json().unwrap()).unwrap();
    assert_eq!(cycle(&back, id), cycle(&doc, id));
    // Its arrows are drawn, never stored.
    assert!(back.arrows.is_empty());
}

#[test]
fn arcs_leave_a_gap_around_every_intermediate_and_follow_a_move() {
    let (mut doc, id, parts) = suzuki();
    let check = |doc: &Document| {
        let arrows = drawn(doc, graphic(doc, id)).arrows;
        assert_eq!(arrows.len(), 3);
        let gap = crate::style::DEFAULT.world(super::GAP_PT) * 0.9;
        for a in &arrows {
            assert!(a.arc.is_some_and(|sweep| sweep > 0.), "clockwise arcs");
            for p in [a.start, a.end] {
                for t in parts {
                    let (lo, hi) = ink(doc, t);
                    let inside = p.x > lo.x - gap
                        && p.x < hi.x + gap
                        && p.y > lo.y - gap
                        && p.y < hi.y + gap;
                    assert!(!inside, "arc end {p:?} runs into {t}");
                }
            }
        }
        arrows
    };
    let before = check(&doc);
    doc.translate(&[parts[1]], 0., 60.);
    let after = check(&doc);
    assert_ne!(before[0].end, after[0].end);
    assert_ne!(before[1].start, after[1].start);
    assert_eq!(before[2].start, after[2].start);
}

#[test]
fn a_reversible_step_draws_two_half_headed_arcs_and_a_hidden_one_none() {
    let (mut doc, id, _) = suzuki();
    if let Some(c) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        c.steps[0].reversible = true;
        c.steps[1].hidden = true;
    }
    let arrows = drawn(&doc, graphic(&doc, id)).arrows;
    assert_eq!(arrows.len(), 3);
    let (a, b) = (&arrows[0], &arrows[1]);
    assert!(a.arc.unwrap() > 0. && b.arc.unwrap() < 0.);
    assert_eq!(a.appearance().head, Head::Left);
    assert_eq!(b.appearance().head, Head::Left);
    // The forward arc runs outside the backward one.
    let (center, _) = graphic(&doc, id).cycle_circle();
    assert!(a.start.distance(center) > b.end.distance(center));
}

#[test]
fn drop_targets_name_the_role_by_place() {
    let (mut doc, id, [pd0, oa, _]) = suzuki();
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let free = text(&mut doc, Point::default(), "Ar–X");
    let place = |doc: &mut Document, angle: f32, distance: f32| {
        let (lo, hi) = ink(doc, free);
        let c = lo.lerp(hi, 0.5);
        let to = center.offset(distance * angle.cos(), distance * angle.sin());
        doc.translate(&[free], to.x - c.x, to.y - c.y);
        drop_target(doc, &[free]).map(|t| t.role)
    };
    let angle = |t: u64, doc: &Document| {
        let (lo, hi) = ink(doc, t);
        let c = lo.lerp(hi, 0.5);
        (c.y - center.y).atan2(c.x - center.x)
    };
    let (a0, a1) = (angle(pd0, &doc), angle(oa, &doc));
    let bond = doc.drawing_style.bond_length_world;
    // On the circle between the first two: the new second intermediate.
    assert_eq!(
        place(&mut doc, (a0 + a1) * 0.5, radius),
        Some(Role::Intermediate(1))
    );
    assert_eq!(
        Role::Intermediate(1).to_string(),
        "Intermediate 2",
        "the label counts from 1"
    );
    let early = a0 + (a1 - a0) * 0.3;
    let late = a0 + (a1 - a0) * 0.7;
    assert_eq!(
        place(&mut doc, early, radius + 2. * bond),
        Some(Role::Reagent(0))
    );
    assert_eq!(
        place(&mut doc, late, radius + 2. * bond),
        Some(Role::Product(0))
    );
    assert_eq!(place(&mut doc, late, radius + 9. * bond), None);
    // A part already in the cycle never changes role by moving.
    assert_eq!(drop_target(&doc, &[oa]), None);
}

#[test]
fn a_drop_on_an_arc_names_its_neighbours_and_shows_that_arc() {
    let (mut doc, id, [pd0, oa, tm]) = suzuki();
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let angle = |t: u64, doc: &Document| {
        let c = center_of(ink(doc, t));
        (c.y - center.y).atan2(c.x - center.x)
    };
    let (a0, a1) = (angle(pd0, &doc), angle(oa, &doc));
    let free = text(&mut doc, Point::default(), "Ar–X");
    let drop_at = |doc: &mut Document, angle: f32, distance: f32| {
        let c = center_of(ink(doc, free));
        let to = center.offset(distance * angle.cos(), distance * angle.sin());
        doc.translate(&[free], to.x - c.x, to.y - c.y);
        drop_target(doc, &[free]).unwrap()
    };
    // Between the first two intermediates, on the circle.
    let target = drop_at(&mut doc, (a0 + a1) * 0.5, radius);
    assert_eq!(target.tag, "Insert between 1 and 2");
    // The highlight is step 1's arc: on the circle, between its ends.
    let [arc] = target.arc.as_slice() else {
        panic!("one arc arrow, got {}", target.arc.len());
    };
    for p in arc {
        assert!((p.distance(center) - radius).abs() < 0.5);
        let a = (p.y - center.y).atan2(p.x - center.x);
        assert!(a > a0 && a < a1, "{a} outside {a0}..{a1}");
    }
    // Between the last and the first, the cycle closes through 1.
    let (a2, a3) = (angle(tm, &doc), a0 + TAU);
    let target = drop_at(&mut doc, (a2 + a3) * 0.5, radius);
    assert_eq!(target.tag, "Insert between 3 and 1");
    // Beside an arc the tag names the role, and that step's arc lights up.
    let bond = doc.drawing_style.bond_length_world;
    let target = drop_at(&mut doc, a0 + (a1 - a0) * 0.3, radius + 2. * bond);
    assert_eq!(target.tag, "Reagent of step 1");
    assert_eq!(target.arc.len(), 1);
}

#[test]
fn a_drop_on_an_empty_cycle_lights_up_its_whole_circle() {
    let mut doc = Document::default();
    let id = super::insert(&mut doc, Point::default(), 100.);
    let free = text(&mut doc, Point::new(0., -100.), "Pd(0)L2");
    let c = center_of(ink(&doc, free));
    doc.translate(&[free], -c.x, -100. - c.y);
    let target = drop_target(&doc, &[free]).unwrap();
    assert_eq!((target.cycle, target.tag.as_str()), (id, "Intermediate 1"));
    // The whole circle: its points go all the way round.
    let [circle] = target.arc.as_slice() else {
        panic!("one circle arrow");
    };
    assert!(
        circle
            .iter()
            .all(|p| (p.distance(Point::default()) - 100.).abs() < 0.5)
    );
    assert!(circle[0].distance(circle[circle.len() / 2]) > 190.);
}

#[test]
fn a_drop_on_a_bowed_arc_splits_its_step() {
    let (mut doc, id, [pd0, oa, _]) = suzuki();
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let bond = doc.drawing_style.bond_length_world;
    if let Some(c) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        c.steps[0].bow_pt = 60.;
    }
    let angle = |t: u64, doc: &Document| {
        let c = center_of(ink(doc, t));
        (c.y - center.y).atan2(c.x - center.x)
    };
    let mid = (angle(pd0, &doc) + angle(oa, &doc)) * 0.5;
    // The visible arc's middle, well off the circle.
    let drawn = drawn(&doc, graphic(&doc, id));
    let on_arc = drawn.arrows[drawn.arcs.iter().position(|s| *s == Some(0)).unwrap()].point(0.5);
    assert!(on_arc.distance(center) - radius > bond);
    let free = text(&mut doc, Point::default(), "Ar–X");
    let c = center_of(ink(&doc, free));
    doc.translate(&[free], on_arc.x - c.x, on_arc.y - c.y);
    let target = drop_target(&doc, &[free]).unwrap();
    assert_eq!(target.role, Role::Intermediate(1), "at {mid}");
    assert_eq!(target.tag, "Insert between 1 and 2");
}

/// Insert beyond the ordinary ownership disk, using the actual painted arc.
fn far_bowed_insert(backwards: bool, reversible: bool) -> (Document, u64, u64, Point) {
    let (mut doc, id, _) = suzuki();
    set_radius(&mut doc, id, 1000.);
    let c = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
        .unwrap();
    if backwards {
        reverse(c);
    }
    c.steps[0].bow_pt = 720.;
    c.steps[0].reversible = reversible;
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let drawn = drawn(&doc, graphic(&doc, id));
    let on_arc = drawn.arrows[drawn.arcs.iter().position(|s| *s == Some(0)).unwrap()].point(0.5);
    assert!(on_arc.distance(center) > radius + 5. * doc.drawing_style.bond_length_world);
    let free = text(&mut doc, Point::default(), "Ar–X");
    let c = center_of(ink(&doc, free));
    doc.translate(&[free], on_arc.x - c.x, on_arc.y - c.y);
    let target = drop_target(&doc, &[free]).unwrap();
    assert_eq!(target.role, Role::Intermediate(1));
    super::assign(&mut doc, &target);
    doc.validate().unwrap();
    (doc, id, free, on_arc)
}

#[test]
fn a_far_bowed_insert_stays_owned_after_the_next_move_and_save() {
    for backwards in [false, true] {
        for reversible in [false, true] {
            let (mut doc, id, free, _) = far_bowed_insert(backwards, reversible);
            let c = doc
                .graphics
                .iter_mut()
                .find(|g| g.id == id)
                .and_then(|g| g.cycle.as_deref_mut())
                .unwrap();
            c.steps[1].name = "Keep the inserted step's oxidative addition".into();
            c.steps[1].lead_pt = 12.;
            c.steps[1].trail_pt = 7.;
            c.steps[1].reversible = true;
            let steps = c.steps.clone();
            // Splitting retains Bow on the incoming step; neither new arc
            // necessarily passes through the old arc's middle any more.
            assert!(super::holds(&doc, id, &[free]));
            let delta = Point::new(0.5, -0.25);
            doc.translate(&[free], delta.x, delta.y);
            assert!(doc.take_out(&[free], delta).is_empty());
            assert_eq!(
                super::role_of(&doc, free),
                Some((id, Role::Intermediate(1)))
            );
            assert_eq!(cycle(&doc, id).steps, steps);
            assert!(
                doc.groups
                    .iter()
                    .any(|g| { g.members.contains(&id) && g.members.contains(&free) })
            );
            let saved = Document::from_json(&doc.file_json().unwrap()).unwrap();
            assert!(super::holds(&saved, id, &[free]));
            assert_eq!(cycle(&saved, id).steps, steps);
        }
    }
}

#[test]
fn a_far_bow_does_not_keep_an_insert_on_the_opposite_side() {
    let (mut doc, id, free, on_arc) = far_bowed_insert(false, false);
    let (center, _) = graphic(&doc, id).cycle_circle();
    let opposite = center.offset(center.x - on_arc.x, center.y - on_arc.y);
    assert!((opposite.distance(center) - on_arc.distance(center)).abs() < 0.01);
    let p = center_of(ink(&doc, free));
    let delta = Point::new(opposite.x - p.x, opposite.y - p.y);
    doc.translate(&[free], delta.x, delta.y);
    assert!(!super::holds(&doc, id, &[free]));
    assert!(
        doc.take_out(&[free], delta)
            .contains(&crate::grouping::Container::Cycle)
    );
    assert_eq!(super::role_of(&doc, free), None);
    assert!(drop_target(&doc, &[free]).is_none());
}

#[test]
fn removing_bow_restores_the_normal_circle_ownership_boundary() {
    let (mut doc, id, free, _) = far_bowed_insert(false, false);
    let c = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
        .unwrap();
    c.steps[0].bow_pt = 0.;
    assert!(!super::holds(&doc, id, &[free]));
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let p = center_of(ink(&doc, free));
    let at = center.offset(radius + 3.5 * doc.drawing_style.bond_length_world, 0.);
    doc.translate(&[free], at.x - p.x, at.y - p.y);
    assert!(super::holds(&doc, id, &[free]));
}

#[test]
fn side_arrows_touch_the_arc_and_only_products_have_heads() {
    let (mut doc, id, [pd0, oa, _]) = suzuki();
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let bond = doc.drawing_style.bond_length_world;
    let mid = |t: u64, doc: &Document| {
        let (lo, hi) = ink(doc, t);
        let c = lo.lerp(hi, 0.5);
        (c.y - center.y).atan2(c.x - center.x)
    };
    let (a0, a1) = (mid(pd0, &doc), mid(oa, &doc));
    for (fraction, label) in [(0.3, "Ar–X"), (0.75, "X–")] {
        let t = text(&mut doc, Point::default(), label);
        let (lo, hi) = ink(&doc, t);
        let c = lo.lerp(hi, 0.5);
        let angle = a0 + (a1 - a0) * fraction;
        let r = radius + 2. * bond;
        doc.translate(
            &[t],
            center.x + r * angle.cos() - c.x,
            center.y + r * angle.sin() - c.y,
        );
        let target = drop_target(&doc, &[t]).unwrap();
        super::assign(&mut doc, &target);
    }
    doc.validate().unwrap();
    let c = cycle(&doc, id);
    assert_eq!(
        (c.steps[0].reagents.len(), c.steps[0].products.len()),
        (1, 1)
    );
    let arrows = drawn(&doc, graphic(&doc, id)).arrows;
    assert_eq!(arrows.len(), 5);
    let (reagent, product) = (&arrows[1], &arrows[2]);
    assert!((reagent.end.distance(center) - radius).abs() < 0.5);
    assert!((product.start.distance(center) - radius).abs() < 0.5);
    assert_eq!(reagent.appearance().head, Head::None);
    assert_eq!(product.appearance().head, Head::Full);
    // Both are grouped with the cycle.
    let group = doc.groups.iter().find(|g| g.members.contains(&id)).unwrap();
    assert_eq!(group.members.len(), 6);
}

#[test]
fn deleting_an_intermediate_joins_its_steps() {
    let (mut doc, id, [_, oa, _]) = suzuki();
    let reagent = text(&mut doc, Point::new(150., -150.), "Ar–X");
    let target = Target {
        cycle: id,
        role: Role::Reagent(1),
        part: vec![reagent],
        ink: ink(&doc, reagent),
        arc: vec![],
        tag: String::new(),
    };
    super::assign(&mut doc, &target);
    if let Some(c) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        c.steps[0].name = "Oxidative addition".into();
    }
    let before = doc.clone();
    doc.delete(&[oa]);
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    let c = cycle(&doc, id);
    assert_eq!(c.intermediates.len(), 2);
    assert_eq!(c.steps[0].name, "Oxidative addition");
    assert_eq!(c.steps[0].reagents, vec![vec![reagent]]);
}

#[test]
fn a_molecule_intermediate_grows_with_its_molecule() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::new(0., -120.));
    let b = doc.add_atom("C", Point::new(40., -120.));
    doc.add_bond(a, b, 1, "plain");
    let t1 = text(&mut doc, Point::new(100., 80.), "B");
    let t2 = text(&mut doc, Point::new(-140., 80.), "C");
    let (id, _) = make(&mut doc, &[a, b, t1, t2]).unwrap();
    let before = doc.clone();
    let c = doc.add_atom("O", Point::new(60., -150.));
    let b_now = doc.atoms.iter().find(|x| x.id == b).unwrap().id;
    doc.add_bond(b_now, c, 1, "plain");
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    assert!(cycle(&doc, id).intermediates[0].contains(&c));
}

#[test]
fn pasting_the_whole_cycle_keeps_it_and_a_part_alone_drops_it() {
    let (mut doc, id, parts) = suzuki();
    let mut all = parts.to_vec();
    all.push(id);
    let copy = crate::editing::selection(&doc, &all);
    let pasted = crate::editing::append(&mut doc, &copy, Point::new(800., 0.));
    assert!(!pasted.is_empty());
    let cycles: Vec<_> = doc
        .graphics
        .iter()
        .filter_map(|g| g.cycle.as_deref())
        .collect();
    assert_eq!(cycles.len(), 2);
    assert_eq!(cycles[1].intermediates.len(), 3);
    assert!(
        cycles[1]
            .intermediates
            .iter()
            .flatten()
            .all(|t| pasted.contains(t))
    );
    let lone = crate::editing::selection(&doc, &[id]);
    let mut other = Document::default();
    crate::editing::append(&mut other, &lone, Point::default());
    let before = other.clone();
    other.version = before.version;
    crate::cycles::reconcile(&mut other);
    let kept = other
        .graphics
        .iter()
        .find_map(|g| g.cycle.as_deref())
        .unwrap();
    assert!(kept.intermediates.is_empty());
}

#[test]
fn reversing_runs_the_steps_the_other_way() {
    let (mut doc, id, [pd0, oa, tm]) = suzuki();
    let reagent = text(&mut doc, Point::new(150., -150.), "Ar–X");
    let target = Target {
        cycle: id,
        role: Role::Reagent(0),
        part: vec![reagent],
        ink: ink(&doc, reagent),
        arc: vec![],
        tag: String::new(),
    };
    super::assign(&mut doc, &target);
    let g = doc.graphics.iter_mut().find(|g| g.id == id).unwrap();
    let c = g.cycle.as_deref_mut().unwrap();
    reverse(c);
    assert!(c.counterclockwise);
    assert_eq!(c.intermediates, vec![vec![pd0], vec![tm], vec![oa]]);
    // The step between Pd(0) and the oxidative addition product now runs
    // back to Pd(0), and what came in goes out.
    assert_eq!(c.steps[2].products, vec![vec![reagent]]);
    let arrows = drawn(&doc, graphic(&doc, id)).arrows;
    assert!(arrows.iter().take(3).all(|a| a.arc.unwrap() < 0.));
}

#[test]
fn resizing_slides_the_parts_with_the_circle() {
    let (mut doc, id, parts) = suzuki();
    let (center, radius) = graphic(&doc, id).cycle_circle();
    set_radius(&mut doc, id, radius * 1.5);
    let (_, now) = graphic(&doc, id).cycle_circle();
    assert!((now - radius * 1.5).abs() < 1e-3);
    for t in parts {
        let (lo, hi) = ink(&doc, t);
        assert!((lo.lerp(hi, 0.5).distance(center) - now).abs() < 0.5);
    }
}

#[test]
fn removing_a_part_frees_it_from_the_cycle_and_group() {
    let (mut doc, id, [_, oa, _]) = suzuki();
    assert_eq!(remove(&mut doc, &[oa]), 1);
    assert_eq!(cycle(&doc, id).intermediates.len(), 2);
    assert!(doc.groups.iter().all(|g| !g.members.contains(&oa)));
    doc.validate().unwrap();
}

#[test]
fn a_scheme_beside_a_cycle_never_takes_its_parts() {
    let mut doc = Document::default();
    let atom = |doc: &mut Document, x: f32, y: f32| {
        let a = doc.add_atom("C", Point::new(x, y));
        let b = doc.add_atom("O", Point::new(x + 40., y));
        doc.add_bond(a, b, 1, "plain");
        a
    };
    let left = atom(&mut doc, -200., 0.);
    let right = atom(&mut doc, 200., 0.);
    let top = atom(&mut doc, 0., -200.);
    let (id, _) = make(&mut doc, &[left, right, top]).unwrap();
    // A plain reaction to the right of the cycle, on the same row.
    let reactant = atom(&mut doc, 600., 0.);
    let product = atom(&mut doc, 900., 0.);
    let arrow = doc.next_id();
    doc.arrows.push(crate::document::Arrow::new(
        arrow,
        Point::new(700., 20.),
        Point::new(860., 20.),
        crate::arrows::Preset::Forward,
        Default::default(),
    ));
    let reaction = crate::reactions::preview(&doc, arrow).unwrap();
    let held = super::part_ids(&doc);
    assert!(cycle(&doc, id).intermediates.len() == 3);
    for p in reaction.reactants.iter().chain(&reaction.products) {
        assert!(p.atoms.iter().all(|a| !held.contains(a)));
    }
    assert!(
        reaction
            .reactants
            .iter()
            .any(|p| p.atoms.contains(&reactant))
    );
    assert!(reaction.products.iter().any(|p| p.atoms.contains(&product)));
    assert!(drawn(&doc, graphic(&doc, id)).arrows.len() == 3);
    // Copying the scheme with a cycle part selected too leaves that part out.
    let whole = |doc: &Document, ids: &[u64]| crate::reactions::molecules(doc, ids).concat();
    let mut picked = whole(&doc, &[top, reactant, product]);
    picked.push(arrow);
    if let Some(a) = doc.arrows.iter_mut().find(|a| a.id == arrow) {
        a.style = Some(crate::arrows::ArrowStyle::preset(
            crate::arrows::Preset::Forward,
        ));
    }
    let snapshot = crate::editing::selection(&doc, &picked);
    let copied = crate::reactions::copy_reaction(&doc, &snapshot)
        .unwrap()
        .unwrap();
    assert!(copied.ids().iter().all(|a| !held.contains(a)));
    assert_eq!((copied.reactants.len(), copied.products.len()), (1, 1));
}

#[test]
fn an_empty_cycle_is_one_whole_circle_with_its_head_at_the_top() {
    let mut doc = Document::default();
    let id = super::insert(&mut doc, Point::new(0., 0.), 100.);
    doc.validate().unwrap();
    let arrows = drawn(&doc, graphic(&doc, id)).arrows;
    assert_eq!(arrows.len(), 1);
    assert!(arrows[0].arc.unwrap() > 350.);
    assert!(arrows[0].end.y < -99.);
}

#[test]
fn step_names_sit_inside_the_arc_and_long_ones_break() {
    let (mut doc, id, _) = suzuki();
    if let Some(c) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        c.steps[0].name = "Oxidative addition".into();
        c.label = "Pd".into();
    }
    let labels = drawn(&doc, graphic(&doc, id)).labels;
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].1, "Oxidative\naddition");
    let (center, radius) = graphic(&doc, id).cycle_circle();
    assert!(labels[0].0.distance(center) < radius);
}

#[test]
fn step_names_keep_clear_of_each_other_and_the_parts() {
    let (mut doc, id, [pd0, oa, tm]) = suzuki();
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let bond = doc.drawing_style.bond_length_world;
    let angle = |t: u64, doc: &Document| {
        let (lo, hi) = ink(doc, t);
        let c = lo.lerp(hi, 0.5);
        (c.y - center.y).atan2(c.x - center.x)
    };
    let a = [angle(pd0, &doc), angle(oa, &doc), angle(tm, &doc)];
    for (step, label, product) in [
        (0, "Ar–X", false),
        (1, "Ar′B(OH)2, base", false),
        (2, "Ar–Ar′", true),
    ] {
        let (a0, mut a1) = (a[step], a[(step + 1) % 3]);
        while a1 < a0 {
            a1 += std::f32::consts::TAU;
        }
        let at = a0 + (a1 - a0) * if product { 0.7 } else { 0.3 };
        let t = text(&mut doc, Point::default(), label);
        let (lo, hi) = ink(&doc, t);
        let c = lo.lerp(hi, 0.5);
        let r = radius + 2.5 * bond;
        doc.translate(
            &[t],
            center.x + r * at.cos() - c.x,
            center.y + r * at.sin() - c.y,
        );
        let target = drop_target(&doc, &[t]).unwrap();
        super::assign(&mut doc, &target);
    }
    if let Some(c) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        for (step, name) in [
            "Oxidative addition",
            "Transmetalation",
            "Reductive elimination",
        ]
        .into_iter()
        .enumerate()
        {
            c.steps[step].name = name.into();
        }
    }
    let drawn = drawn(&doc, graphic(&doc, id));
    let boxes: Vec<(Point, Point)> = drawn
        .labels
        .iter()
        .map(|(at, text, format)| {
            let l = crate::typography::layout(text, format);
            (*at, at.offset(l.width, l.height))
        })
        .collect();
    assert_eq!(boxes.len(), 3);
    let hit = |a: (Point, Point), b: (Point, Point)| {
        a.0.x < b.1.x && b.0.x < a.1.x && a.0.y < b.1.y && b.0.y < a.1.y
    };
    let parts: Vec<_> = super::part_ids(&doc)
        .into_iter()
        .map(|t| ink(&doc, t))
        .collect();
    for (i, b) in boxes.iter().enumerate() {
        for other in boxes.iter().skip(i + 1) {
            assert!(!hit(*b, *other), "names overlap");
        }
        for p in &parts {
            assert!(!hit(*b, *p), "a name covers a part");
        }
    }
}

#[test]
fn an_intermediate_moved_off_the_circle_keeps_an_opening() {
    let (mut doc, id, [_, oa, _]) = suzuki();
    let (center, _) = graphic(&doc, id).cycle_circle();
    let (lo, hi) = ink(&doc, oa);
    let c = lo.lerp(hi, 0.5);
    let bond = doc.drawing_style.bond_length_world;
    let out = 4. * bond / c.distance(center);
    doc.translate(&[oa], (c.x - center.x) * out, (c.y - center.y) * out);
    let arrows = drawn(&doc, graphic(&doc, id)).arrows;
    // Step 1 ends and step 2 starts either side of the moved intermediate.
    assert!(arrows[0].end.distance(arrows[1].start) > bond);
}

#[test]
fn a_flipped_cycle_runs_the_other_way_and_keeps_its_steps() {
    let (mut doc, id, _) = suzuki();
    let before = cycle(&doc, id).clone();
    let (center, _) = graphic(&doc, id).cycle_circle();
    let mirror = |p: Point| Point::new(2. * center.x - p.x, p.y);
    for a in &mut doc.annotations {
        let (w, _) = a.size();
        let p = mirror(a.position);
        a.position = Point::new(p.x - w, p.y);
    }
    if let Some(g) = doc.graphics.iter_mut().find(|g| g.id == id) {
        g.map_positions(mirror);
    }
    assert!(graphic(&doc, id).cycle_counterclockwise());
    assert_eq!(cycle(&doc, id), &before);
    // Three short arcs, not three that run the long way round.
    let arrows = drawn(&doc, graphic(&doc, id)).arrows;
    assert!(
        arrows
            .iter()
            .all(|a| a.arc.unwrap() < 0. && a.arc.unwrap() > -150.)
    );
    // Resizing keeps the mirror.
    let (_, radius) = graphic(&doc, id).cycle_circle();
    set_radius(&mut doc, id, radius * 1.2);
    assert!(graphic(&doc, id).cycle_counterclockwise());
}

#[test]
fn copying_the_circle_alone_keeps_no_phantom_parts() {
    let (doc, id, _) = suzuki();
    let part = crate::editing::selection(&doc, &[id]);
    let c = part
        .graphics
        .iter()
        .find_map(|g| g.cycle.as_deref())
        .unwrap();
    assert!(c.intermediates.is_empty() && c.steps.is_empty());
    part.validate().unwrap();
}

#[test]
fn a_degenerate_circle_draws_nothing_and_does_not_validate() {
    let (mut doc, id, _) = suzuki();
    if let Some(g) = doc.graphics.iter_mut().find(|g| g.id == id) {
        g.axis_x = Point::default();
        g.axis_y = Point::default();
    }
    assert!(doc.validate().is_err());
    assert!(drawn(&doc, graphic(&doc, id)).arrows.is_empty());
}

#[test]
fn an_empty_cycle_reverses_without_parts() {
    let mut doc = Document::default();
    let id = super::insert(&mut doc, Point::new(0., 0.), 100.);
    let g = doc.graphics.iter_mut().find(|g| g.id == id).unwrap();
    reverse(g.cycle.as_deref_mut().unwrap());
    assert!(graphic(&doc, id).cycle_counterclockwise());
    doc.validate().unwrap();
}

#[test]
fn a_grouped_text_is_never_a_drop_target() {
    let (mut doc, id, [pd0, oa, _]) = suzuki();
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let free = text(&mut doc, Point::default(), "Ar–X");
    let other = text(&mut doc, Point::new(2000., 2000.), "note");
    doc.group_selection(&[free, other]).unwrap();
    let angle = |t: u64, doc: &Document| {
        let (lo, hi) = ink(doc, t);
        let c = lo.lerp(hi, 0.5);
        (c.y - center.y).atan2(c.x - center.x)
    };
    let at = angle(pd0, &doc) + (angle(oa, &doc) - angle(pd0, &doc)) * 0.3;
    let r = radius + 2. * doc.drawing_style.bond_length_world;
    let (lo, hi) = ink(&doc, free);
    let c = lo.lerp(hi, 0.5);
    doc.translate(
        &[free],
        center.x + r * at.cos() - c.x,
        center.y + r * at.sin() - c.y,
    );
    assert_eq!(drop_target(&doc, &[free]), None);
}

#[test]
fn a_reagent_whose_step_goes_leaves_the_cycle_group_and_keeps_names() {
    let mut doc = Document::default();
    let id = super::insert(&mut doc, Point::new(0., 0.), 200.);
    let only = text(&mut doc, Point::new(-20., -210.), "Pd");
    let reagent = text(&mut doc, Point::new(150., -250.), "ArX");
    for (role, part) in [(Role::Intermediate(0), only), (Role::Reagent(0), reagent)] {
        let target = Target {
            cycle: id,
            role,
            part: vec![part],
            ink: ink(&doc, part),
            arc: vec![],
            tag: String::new(),
        };
        super::assign(&mut doc, &target);
    }
    let before = doc.clone();
    doc.delete(&[only]);
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    assert!(cycle(&doc, id).steps.is_empty());
    assert!(doc.groups.iter().all(|g| !g.members.contains(&reagent)));

    // Joining two steps keeps the name of the one that went if the other
    // had none.
    let (mut doc, id, [_, oa, _]) = suzuki();
    if let Some(c) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        c.steps[1].name = "Transmetalation".into();
    }
    remove(&mut doc, &[oa]);
    assert_eq!(cycle(&doc, id).steps[0].name, "Transmetalation");
}

#[test]
fn a_part_in_its_own_group_can_leave_and_full_steps_can_join() {
    let mut doc = Document::default();
    let a = text(&mut doc, Point::new(0., -200.), "A");
    let b = text(&mut doc, Point::new(170., 90.), "B");
    let c = text(&mut doc, Point::new(-230., 90.), "C");
    doc.group_selection(&[a, b]).unwrap();
    let (id, _) = make(&mut doc, &[a, b, c]).unwrap();
    doc.validate().unwrap();
    let before = doc.clone();
    remove(&mut doc, &[a]);
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    assert_eq!(cycle(&doc, id).intermediates.len(), 2);

    // Two steps with 40 reagents each join into one valid step.
    let (mut doc, id, [_, oa, _]) = suzuki();
    for step in 0..2 {
        for k in 0..40 {
            let t = text(
                &mut doc,
                Point::new(3000. + k as f32 * 50., step as f32 * 50.),
                "R",
            );
            if let Some(c) = doc
                .graphics
                .iter_mut()
                .find(|g| g.id == id)
                .and_then(|g| g.cycle.as_deref_mut())
            {
                c.steps[step].reagents.push(vec![t]);
            }
        }
    }
    let before = doc.clone();
    doc.delete(&[oa]);
    let _ = crate::transaction::reconcile(&mut doc, before).unwrap();
    assert_eq!(cycle(&doc, id).steps[0].reagents.len(), 80);
}

fn bowed_insertion() -> (Document, Target) {
    let mut doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/cycles/bowed-insertion.rsk"
    ))
    .unwrap();
    // The actual drag captured in the owner walkthrough.
    let part: Vec<_> = (40..=85).collect();
    doc.translate(&part, -234.75783, 388.1741);
    let target = drop_target(&doc, &part).unwrap();
    assert_eq!((target.cycle, target.role), (191, Role::Intermediate(1)));
    (doc, target)
}

#[test]
fn splitting_a_bowed_step_only_brings_its_stranded_side_parts_along() {
    let (mut doc, target) = bowed_insertion();
    let before = doc.clone();
    let old_step = cycle(&before, 191).steps[0].clone();
    super::assign(&mut doc, &target);
    if let Some(dir) = std::env::var_os("RESHIKI_CYCLE_SPLIT_QA_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("after.rsk"), doc.file_json().unwrap()).unwrap();
        let svg = crate::scene::svg_part(
            &crate::scene::primitives(&doc),
            Point::new(-800., -650.),
            Point::new(800., 800.),
        );
        std::fs::write(dir.join("after.svg"), &svg).unwrap();
        let mut options = resvg::usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let tree = resvg::usvg::Tree::from_str(&svg, &options).unwrap();
        let mut picture = resvg::tiny_skia::Pixmap::new(1200, 1088).unwrap();
        picture.fill(resvg::tiny_skia::Color::WHITE);
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(0.75, 0.75),
            &mut picture.as_mut(),
        );
        picture.save_png(dir.join("after.png")).unwrap();
        let mut spans = vec![];
        super::drawn_spans(&before, graphic(&before, 191), |i, span| {
            spans.push(format!("old step {i}: {span:?}"))
        });
        super::drawn_spans(&doc, graphic(&doc, 191), |i, span| {
            spans.push(format!("step {i}: {span:?}"))
        });
        std::fs::write(dir.join("spans.txt"), spans.join("\n")).unwrap();
    }
    let kept = cycle(&doc, 191);
    assert_eq!(
        kept.steps[0], old_step,
        "roles and user settings stay exact"
    );
    assert_eq!(kept.steps[1], super::Step::default());
    assert_eq!(kept.intermediates[1], target.part);
    for id in [156, 160] {
        assert_ne!(
            doc.atoms.iter().find(|a| a.id == id).unwrap().position,
            before.atoms.iter().find(|a| a.id == id).unwrap().position,
            "the inherited side part is stranded past the shortened step"
        );
    }
    for atom in &before.atoms {
        if !(156..=168).contains(&atom.id) {
            assert_eq!(doc.atoms.iter().find(|a| a.id == atom.id), Some(atom));
        }
    }
    assert_eq!(doc.bonds, before.bonds);
    assert_eq!(doc.abbreviations, before.abbreviations);
    let drawing = drawn(&doc, graphic(&doc, 191));
    let sides: Vec<_> = drawing
        .arrows
        .iter()
        .zip(&drawing.arcs)
        .filter(|(_, step)| step.is_none())
        .take(3)
        .map(|(a, _)| a)
        .collect();
    let bond = doc.drawing_style.bond_length_world;
    for side in &sides[1..] {
        let touch = if side.appearance().head == Head::None {
            side.end
        } else {
            side.start
        };
        for (arc, step) in drawing
            .arrows
            .iter()
            .zip(&drawing.arcs)
            .filter(|(_, step)| step.is_some())
        {
            for k in 0..=1024 {
                let p = side.point(k as f32 / 1024.);
                if *step == Some(0) && p.distance(touch) <= bond * 2. {
                    continue;
                }
                assert!(!arc.hit(p, 1.), "side curve crosses step {step:?} at {p:?}");
            }
        }
    }
    doc.validate().unwrap();
}

#[test]
fn split_placement_keeps_one_history_frame_and_round_trips() {
    let initial = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/cycles/bowed-insertion.rsk"
    ))
    .unwrap();
    let (mut doc, target) = bowed_insertion();
    super::assign(&mut doc, &target);
    let mut history = crate::document::History::default();
    crate::transaction::apply(&mut doc, &mut history, initial.clone(), false).unwrap();
    let after = doc.clone();
    assert_eq!(history.undo_frames().len(), 1);
    assert!(history.undo(&mut doc));
    assert_eq!(doc, initial);
    assert!(!history.can_undo());
    assert!(history.redo(&mut doc));
    assert_eq!(doc, after);
    let reopened = Document::from_json(&after.file_json().unwrap()).unwrap();
    assert_eq!(reopened.atoms, after.atoms);
    assert_eq!(reopened.annotations, after.annotations);
    assert_eq!(cycle(&reopened, 191), cycle(&after, 191));
}

#[test]
fn a_wraparound_split_keeps_the_same_physical_step_and_placement() {
    let (mut normal, target) = bowed_insertion();
    let mut wrapped = normal.clone();
    let c = wrapped
        .graphics
        .iter_mut()
        .find(|g| g.id == 191)
        .unwrap()
        .cycle
        .as_deref_mut()
        .unwrap();
    c.intermediates.rotate_left(1);
    c.steps.rotate_left(1);
    let mut wrap_target = target.clone();
    wrap_target.role = Role::Intermediate(0);
    super::assign(&mut normal, &target);
    super::assign(&mut wrapped, &wrap_target);
    assert_eq!(wrapped.atoms, normal.atoms);
    assert_eq!(cycle(&wrapped, 191).steps[2], cycle(&normal, 191).steps[0]);
    wrapped.validate().unwrap();
}

#[test]
fn an_ordinary_side_assignment_keeps_every_position() {
    let (mut doc, mut target) = bowed_insertion();
    target.role = Role::Reagent(0);
    let before = doc.clone();
    super::assign(&mut doc, &target);
    assert_eq!(doc.atoms, before.atoms);
    assert_eq!(doc.annotations, before.annotations);
    assert_eq!(
        cycle(&doc, 191).steps[0].reagents.last(),
        Some(&target.part)
    );
}

#[test]
fn split_placements_stay_finite_when_mirrored_reversed_hidden_or_reversible() {
    for mode in 0..4 {
        let (mut doc, target) = bowed_insertion();
        if mode == 0 {
            let ids = doc.all_ids();
            crate::editing::transform(&mut doc, &ids, crate::editing::Transform::FlipHorizontal);
        } else {
            let c = doc
                .graphics
                .iter_mut()
                .find(|g| g.id == 191)
                .unwrap()
                .cycle
                .as_deref_mut()
                .unwrap();
            if mode == 1 {
                super::reverse(c);
            } else if mode == 2 {
                c.steps[0].hidden = true;
            } else {
                c.steps[0].reversible = true;
            }
        }
        let target = if mode == 2 {
            target
        } else {
            drop_target(&doc, &target.part).unwrap()
        };
        let old_steps = cycle(&doc, 191).steps.clone();
        super::assign(&mut doc, &target);
        let Role::Intermediate(i) = target.role else {
            panic!("an intermediate");
        };
        let mut expected = old_steps;
        expected.insert(i, super::Step::default());
        assert_eq!(cycle(&doc, 191).steps, expected);
        assert!(doc.atoms.iter().all(|a| a.position.is_finite()));
        doc.validate().unwrap();
    }
}

#[test]
fn a_stranded_molecule_keeps_its_linked_caption() {
    let (mut doc, target) = bowed_insertion();
    let atoms: Vec<_> = (160..=168).collect();
    let identity = crate::molecule_names::identity(&doc, &atoms).unwrap();
    let caption = crate::molecule_names::show(
        &mut doc,
        &atoms,
        "phenylboronic acid".into(),
        identity,
        TextFormat::default(),
        false,
    )
    .unwrap();
    let from = doc.atoms.iter().find(|a| a.id == 160).unwrap().position;
    let label_from = doc
        .annotations
        .iter()
        .find(|a| a.id == caption)
        .unwrap()
        .position;
    super::assign(&mut doc, &target);
    let to = doc.atoms.iter().find(|a| a.id == 160).unwrap().position;
    let label_to = doc
        .annotations
        .iter()
        .find(|a| a.id == caption)
        .unwrap()
        .position;
    assert!(
        from.distance(to) > 1.,
        "caption does not prevent the repair"
    );
    let delta = Point::new(to.x - from.x, to.y - from.y);
    assert!(label_from.offset(delta.x, delta.y).distance(label_to) < 1e-3);
    assert_eq!(
        crate::molecule_names::include_annotations(&doc, &atoms).last(),
        Some(&caption)
    );
    doc.validate().unwrap();
}
