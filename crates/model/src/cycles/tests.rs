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

#[test]
fn tidy_spaces_intermediates_evenly_and_lines_up_side_parts() {
    let (mut doc, id, [pd0, oa, tm]) = suzuki();
    let (center, radius) = graphic(&doc, id).cycle_circle();
    let bond = doc.drawing_style.bond_length_world;
    let angle = |doc: &Document, t: u64| {
        let c = center_of(ink(doc, t));
        (c.y - center.y).atan2(c.x - center.x)
    };
    let put = |doc: &mut Document, t: u64, a: f32, r: f32| {
        let c = center_of(ink(doc, t));
        let to = center.offset(r * a.cos(), r * a.sin());
        doc.translate(&[t], to.x - c.x, to.y - c.y);
    };
    // Crowd the second intermediate toward the first, off the circle.
    let (a0, a1) = (angle(&doc, pd0), angle(&doc, oa));
    put(&mut doc, oa, a0 + (a1 - a0) * 0.4, radius + 0.4 * bond);
    // A reagent and a product of step 1, both badly placed; a product of
    // step 2 inside the circle.
    let add = |doc: &mut Document, role: Role, label: &str, a: f32, r: f32| {
        let t = text(doc, Point::default(), label);
        put(doc, t, a, r);
        let target = Target {
            cycle: id,
            role,
            part: vec![t],
            ink: ink(doc, t),
            arc: vec![],
            tag: String::new(),
        };
        super::assign(doc, &target);
        t
    };
    let reagent = add(
        &mut doc,
        Role::Reagent(0),
        "Ar–X",
        a0 + 0.1,
        radius + 3. * bond,
    );
    let product = add(
        &mut doc,
        Role::Product(0),
        "X–",
        a0 + 0.2,
        radius + 1.5 * bond,
    );
    let inside = add(&mut doc, Role::Product(1), "B(OH)3", a1 + 0.3, radius * 0.6);
    // Step 1's arc starts well after its first intermediate.
    if let Some(c) = doc
        .graphics
        .iter_mut()
        .find(|g| g.id == id)
        .and_then(|g| g.cycle.as_deref_mut())
    {
        c.steps[0].lead_pt = 40.;
    }
    let before = doc.clone();
    assert!(super::tidy(&mut doc, id));
    doc.validate().unwrap();
    // Evenly round, in order, each centred on the circle.
    let third = TAU / 3.;
    let angles = [pd0, oa, tm].map(|t| angle(&doc, t));
    for (k, t) in [pd0, oa, tm].into_iter().enumerate() {
        let d = center_of(ink(&doc, t)).distance(center);
        assert!(
            (d - radius).abs() < 0.5,
            "intermediate {k} at {d}, not {radius}"
        );
        let step = (angles[(k + 1) % 3] - angles[k]).rem_euclid(TAU);
        assert!((step - third).abs() < 1e-3, "step {k} spans {step}");
    }
    // Side parts sit beside their step's drawn arc, which starts after the
    // step's extra space: reagents in its first half, products in its
    // second, on the side they were on and still within the cycle's reach.
    let drawn = drawn(&doc, graphic(&doc, id));
    let arc = |s: usize| &drawn.arrows[drawn.arcs.iter().position(|a| *a == Some(s)).unwrap()];
    let beside = |t: u64, s: usize, along: f32| {
        let on = arc(s).point(along);
        let a = (on.y - center.y).atan2(on.x - center.x);
        let c = center_of(ink(&doc, t));
        let d = c.distance(center);
        assert!(((c.y - center.y).atan2(c.x - center.x) - a).abs() < 1e-3);
        assert!(super::holds(&doc, id, &[t]), "{t} out of reach");
        d - on.distance(center)
    };
    assert!(beside(reagent, 0, 0.3) > bond);
    assert!(beside(product, 0, 0.7) > bond);
    assert!(beside(inside, 1, 0.7) < -bond);
    // Everything keeps its role, and a second tidy moves nothing.
    assert_eq!(cycle(&doc, id), cycle(&before, id));
    assert!(!super::tidy(&mut doc, id));
}

#[test]
fn tidy_keeps_side_parts_on_their_side_of_bowed_arcs() {
    for (bow, inside) in [(72., true), (-72., false)] {
        let mut doc = Document::default();
        let centered_text = |doc: &mut Document, at: Point, label: &str| {
            let id = text(doc, Point::default(), label);
            let from = center_of(ink(doc, id));
            doc.translate(&[id], at.x - from.x, at.y - from.y);
            id
        };
        let parts = [
            Point::new(0., -200.),
            Point::new(200., 0.),
            Point::new(0., 200.),
            Point::new(-200., 0.),
        ]
        .into_iter()
        .map(|at| centered_text(&mut doc, at, "Pd"))
        .collect::<Vec<_>>();
        let (id, _) = make(&mut doc, &parts).unwrap();
        let (center, radius) = graphic(&doc, id).cycle_circle();
        let r = if inside { radius * 0.6 } else { radius * 1.3 };
        let side = centered_text(&mut doc, super::polar(center, r, -0.8), "ArX");
        let target = Target {
            cycle: id,
            role: Role::Reagent(0),
            part: vec![side],
            ink: ink(&doc, side),
            arc: vec![],
            tag: String::new(),
        };
        super::assign(&mut doc, &target);
        doc.graphics
            .iter_mut()
            .find(|g| g.id == id)
            .unwrap()
            .cycle
            .as_deref_mut()
            .unwrap()
            .steps[0]
            .bow_pt = bow;
        let roles = cycle(&doc, id).clone();

        assert!(super::tidy(&mut doc, id));
        let placed = center_of(ink(&doc, side)).distance(center);
        assert_eq!(placed < radius, inside, "bow {bow}: placed at {placed}");
        assert!(super::holds(&doc, id, &[side]));
        assert_eq!(cycle(&doc, id), &roles);
        assert!(!super::tidy(&mut doc, id), "bow {bow}: second tidy moved");
        doc.validate().unwrap();
    }
}

#[test]
fn tidy_leaves_an_empty_cycle_and_a_plain_circle_alone() {
    let mut doc = Document::default();
    let id = super::insert(&mut doc, Point::default(), 100.);
    let plain = super::insert(&mut doc, Point::new(400., 0.), 100.);
    if let Some(g) = doc.graphics.iter_mut().find(|g| g.id == plain) {
        g.cycle = None;
    }
    let before = doc.clone();
    assert!(!super::tidy(&mut doc, id));
    assert!(!super::tidy(&mut doc, plain));
    assert_eq!(doc, before);
}
