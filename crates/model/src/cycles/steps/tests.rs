use super::{balance, color, copy_maps, reactions, step};
use crate::{
    chemistry,
    cycles::{Role, Target, assign, make, step_at},
    document::{Annotation, Arrow, Document, Point},
    palette::Color,
    reaction_mapping::{Budget, propose},
    reactions::{Participant, Reaction},
    typography::TextFormat,
};

/// A molecule from SMILES at `(x, y)`; with `maps`, atom i takes map maps[i].
fn molecule(doc: &mut Document, smiles: &str, x: f32, y: f32, maps: &[u32]) -> Vec<u64> {
    let parsed = chemistry::smiles::parse(smiles).unwrap();
    let ids: Vec<_> = parsed
        .graph
        .atoms
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let symbol = chemistry::ELEMENTS[usize::from(input.atomic_number)].symbol;
            let id = doc.add_atom(symbol, Point::new(x + 30. * i as f32, y));
            let atom = doc.atom_mut(id).unwrap();
            atom.charge = i32::from(input.charge);
            atom.explicit_h = u32::from(input.explicit_hydrogens);
            atom.no_implicit = input.no_implicit;
            atom.map_num = maps.get(i).copied().unwrap_or(0);
            id
        })
        .collect();
    for bond in parsed.graph.bonds {
        doc.add_bond(ids[bond.a], ids[bond.b], bond.order, "plain");
    }
    ids
}

fn join(doc: &mut Document, id: u64, role: Role, part: &[u64]) {
    let ink = crate::scene::selection_bounds(doc, part).unwrap();
    assign(
        doc,
        &Target {
            cycle: id,
            role,
            part: part.to_vec(),
            ink,
            arc: vec![],
            tag: String::new(),
        },
    );
}

struct Toy {
    doc: Document,
    id: u64,
    /// Pd, CH3Br, CH3PdBr, CH3Li, LiBr, CH3PdCH3, ethane.
    pd: Vec<u64>,
    mebr: Vec<u64>,
    oa: Vec<u64>,
    meli: Vec<u64>,
    libr: Vec<u64>,
    tm: Vec<u64>,
    ethane: Vec<u64>,
}

/// A balanced three-step cross-coupling: Pd + CH3Br → CH3PdBr; + CH3Li →
/// CH3PdCH3 + LiBr; → Pd + ethane. With `mapped`, every atom carries the
/// map of the atom it becomes.
fn toy(mapped: bool) -> Toy {
    let m = |maps: &'static [u32]| if mapped { maps } else { &[] };
    let mut doc = Document::default();
    let pd = molecule(&mut doc, "[Pd]", 0., -200., m(&[1]));
    let oa = molecule(&mut doc, "C[Pd]Br", 170., 90., m(&[2, 1, 3]));
    let tm = molecule(&mut doc, "C[Pd]C", -230., 90., m(&[2, 1, 4]));
    let ids: Vec<u64> = [&pd, &oa, &tm].into_iter().flatten().copied().collect();
    let (id, left) = make(&mut doc, &ids).unwrap();
    assert!(!left);
    let mebr = molecule(&mut doc, "CBr", 260., -160., m(&[2, 3]));
    join(&mut doc, id, Role::Reagent(0), &mebr);
    let meli = molecule(&mut doc, "C[Li]", 60., 330., m(&[4, 5]));
    join(&mut doc, id, Role::Reagent(1), &meli);
    let libr = molecule(&mut doc, "[Li]Br", -120., 330., m(&[5, 3]));
    join(&mut doc, id, Role::Product(1), &libr);
    let ethane = molecule(&mut doc, "CC", -330., -160., m(&[2, 4]));
    join(&mut doc, id, Role::Product(2), &ethane);
    doc.validate().unwrap();
    Toy {
        doc,
        id,
        pd,
        mebr,
        oa,
        meli,
        libr,
        tm,
        ethane,
    }
}

fn label_color(doc: &Document, id: u64) -> Option<Color> {
    doc.atom(id)?.text_style.as_ref().map(|s| s.color)
}

#[test]
fn a_step_is_a_reaction_of_its_parts_on_a_copy() {
    let t = toy(false);
    let mut doc = t.doc.clone();
    let note = doc.next_id();
    doc.annotations.push(Annotation {
        id: note,
        position: Point::new(240., -60.),
        text: "THF".into(),
        format: TextFormat::default(),
    });
    join(&mut doc, t.id, Role::Reagent(0), &[note]);
    let (copy, arrow) = step(&doc, t.id, 0).unwrap();
    let part = |ids: &Vec<u64>| Participant {
        atoms: {
            let mut ids = ids.clone();
            ids.sort_unstable();
            ids
        },
        coefficient: 1,
    };
    let mut expected = Reaction::new(arrow);
    expected.reactants = vec![part(&t.pd), part(&t.mebr)];
    expected.products = vec![part(&t.oa)];
    expected.annotations = vec![note];
    assert_eq!(copy.reactions, vec![expected]);
    // Only the step's parts, with a forward arrow no real object uses.
    let mut atoms: Vec<u64> = copy.atoms.iter().map(|a| a.id).collect();
    atoms.sort_unstable();
    let mut held: Vec<u64> = [&t.pd, &t.mebr, &t.oa]
        .into_iter()
        .flatten()
        .copied()
        .collect();
    held.sort_unstable();
    assert_eq!(atoms, held);
    assert!(arrow >= doc.next_id() && copy.arrows.len() == 1);
    assert!(copy.graphics.is_empty());
    // The drawing itself holds no reaction and no arrow.
    assert!(doc.reactions.is_empty() && doc.arrows.is_empty());
    // Every step, for color across the cycle, each with its own arrow.
    let (all, arrows) = reactions(&doc, t.id).unwrap();
    assert_eq!(arrows.len(), 3);
    assert_eq!(all.reactions.len(), 3);
    assert_eq!(all.reactions[2].products[0], part(&t.pd));
}

#[test]
fn a_step_with_text_on_one_side_says_what_it_needs() {
    let mut doc = Document::default();
    let text = |doc: &mut Document, x: f32, y: f32, value: &str| {
        let id = doc.next_id();
        doc.annotations.push(Annotation {
            id,
            position: Point::new(x, y),
            text: value.into(),
            format: TextFormat::default(),
        });
        id
    };
    let pd0 = text(&mut doc, 0., -200., "Pd(0)L2");
    let oa = molecule(&mut doc, "C[Pd]Br", 170., 90., &[]);
    let tm = text(&mut doc, -230., 90., "Ar–Pd–Ar′");
    let mut ids = oa.clone();
    ids.extend([pd0, tm]);
    let (id, _) = make(&mut doc, &ids).unwrap();
    let error = step(&doc, id, 1).unwrap_err();
    assert_eq!(error, "Step 2 needs a structure on each side");
    // One intermediate alone has no step to run.
    let lone = crate::cycles::insert(&mut doc, Point::new(900., 0.), 100.);
    assert!(step(&doc, lone, 0).is_err());
}

#[test]
fn auto_map_on_a_step_copy_lands_on_the_same_atoms() {
    let t = toy(false);
    let mut doc = t.doc.clone();
    // Ethane already carries maps 1 and 2 from an earlier step's mapping.
    for (id, n) in t.ethane.iter().zip([1, 2]) {
        doc.atom_mut(*id).unwrap().map_num = n;
    }
    let before = doc.clone();
    // The whole cycle's copy, so new numbers stay clear of the other steps.
    let (copy, arrows) = reactions(&doc, t.id).unwrap();
    let proposal = propose(&copy, arrows[0], Budget::default()).unwrap();
    let mapped = proposal.apply(&copy, true).unwrap();
    copy_maps(&mut doc, &mapped);
    let map = |id: u64| doc.atom(id).unwrap().map_num;
    // Pd, carbon and bromine keep one number each across the step.
    assert_eq!(map(t.pd[0]), map(t.oa[1]));
    assert_eq!(map(t.mebr[0]), map(t.oa[0]));
    assert_eq!(map(t.mebr[1]), map(t.oa[2]));
    assert!(map(t.pd[0]) != 0 && map(t.mebr[0]) != 0);
    let step1: Vec<u64> = [&t.pd, &t.mebr].into_iter().flatten().copied().collect();
    assert!(step1.iter().all(|id| ![1, 2].contains(&map(*id))));
    // Nothing else changed.
    assert!(t.tm.iter().all(|id| map(*id) == 0));
    let mut unmapped = doc.clone();
    for atom in &mut unmapped.atoms {
        atom.map_num = before.atom(atom.id).unwrap().map_num;
    }
    assert_eq!(unmapped, before);
}

#[test]
fn colors_carry_from_one_step_into_the_next() {
    let t = toy(true);
    let mut from_second = t.doc.clone();
    color(&mut from_second, t.id, 1).unwrap();
    // Bromine came in with CH3Br: it keeps that color in CH3PdBr, a product
    // of step 1 and a reactant of step 2, and in LiBr.
    let br = label_color(&from_second, t.mebr[1]).unwrap();
    assert_eq!(label_color(&from_second, t.oa[2]), Some(br));
    assert_eq!(label_color(&from_second, t.libr[1]), Some(br));
    assert_ne!(label_color(&from_second, t.meli[1]), Some(br));
    // Whichever step it starts from, the cycle gets the same colors.
    let mut from_first = t.doc.clone();
    color(&mut from_first, t.id, 0).unwrap();
    assert_eq!(from_first, from_second);
    assert!(from_first.reactions.is_empty() && from_first.arrows.is_empty());
}

#[test]
fn a_step_without_maps_asks_for_them() {
    let t = toy(false);
    let mut doc = t.doc.clone();
    assert_eq!(
        color(&mut doc, t.id, 0),
        Err(crate::reaction_colors::Refusal::Maps)
    );
    assert_eq!(doc, t.doc);
}

#[test]
fn balance_counts_atoms_hydrogens_and_charge() {
    let t = toy(false);
    for s in 0..3 {
        let b = balance(&t.doc, t.id, s).unwrap();
        assert!(b.balanced(), "step {s}: {b:?}");
    }
    // Without LiBr, the transmetalation loses lithium and bromine.
    let mut doc = t.doc.clone();
    crate::cycles::remove(&mut doc, &t.libr);
    let b = balance(&doc, t.id, 1).unwrap();
    assert_eq!(b.before, vec![("Br".into(), 1), ("Li".into(), 1)]);
    assert!(b.after.is_empty() && !b.balanced());
}

#[test]
fn joining_a_cycle_takes_a_part_out_of_its_ordinary_reaction() {
    let t = toy(false);
    let mut doc = t.doc.clone();
    // An ordinary scheme to the right: CH3I → CH3OH with a condition text.
    let a = molecule(&mut doc, "CI", 900., 0., &[]);
    let b = molecule(&mut doc, "CO", 1200., 0., &[]);
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(1000., 0.),
        Point::new(1150., 0.),
        crate::arrows::Preset::Forward,
        Default::default(),
    ));
    let note = doc.next_id();
    doc.annotations.push(Annotation {
        id: note,
        position: Point::new(1050., -40.),
        text: "NaOH".into(),
        format: TextFormat::default(),
    });
    crate::reactions::assign(&mut doc, arrow, &a, crate::reactions::Role::Reactant).unwrap();
    crate::reactions::assign(&mut doc, arrow, &b, crate::reactions::Role::Product).unwrap();
    doc.reactions[0].annotations.push(note);
    doc.validate().unwrap();
    // CH3I joins step 1 as a reagent: it leaves the scheme's reactants.
    join(&mut doc, t.id, Role::Reagent(0), &a);
    assert!(doc.reactions[0].reactants.is_empty());
    assert_eq!(doc.reactions[0].products.len(), 1);
    // The condition text too.
    join(&mut doc, t.id, Role::Product(0), &[note]);
    assert!(doc.reactions[0].annotations.is_empty());
    doc.validate().unwrap();
    // Leaving the cycle does not bring the role back.
    crate::cycles::remove(&mut doc, &a);
    assert!(doc.reactions[0].reactants.is_empty());
    // Making a cycle from a molecule with a role says so too.
    let mut other = t.doc.clone();
    let c = molecule(&mut other, "CI", 900., 0., &[]);
    let d = molecule(&mut other, "CO", 1200., 0., &[]);
    let e = molecule(&mut other, "CN", 1000., 300., &[]);
    other.arrows.push(Arrow::new(
        other.next_id(),
        Point::new(1000., 0.),
        Point::new(1150., 0.),
        crate::arrows::Preset::Forward,
        Default::default(),
    ));
    let arrow = other.arrows[0].id;
    crate::reactions::assign(&mut other, arrow, &c, crate::reactions::Role::Reactant).unwrap();
    let mut ids = c.clone();
    ids.extend(&d);
    ids.extend(&e);
    let (_, left) = make(&mut other, &ids).unwrap();
    assert!(left && other.reactions[0].reactants.is_empty());
}

#[test]
fn the_step_toward_a_point_is_the_one_whose_arc_is_there() {
    let t = toy(false);
    let g = t.doc.graphics.iter().find(|g| g.id == t.id).unwrap();
    let (center, radius) = g.cycle_circle();
    let toward = |ids: &[u64]| {
        let (lo, hi) = crate::scene::selection_bounds(&t.doc, ids).unwrap();
        let c = lo.lerp(hi, 0.5);
        let d = c.distance(center);
        Point::new((c.x - center.x) / d, (c.y - center.y) / d)
    };
    // Halfway round from Pd to CH3PdBr, just outside the circle: step 1.
    let (a, b) = (toward(&t.pd), toward(&t.oa));
    let (x, y) = (a.x + b.x, a.y + b.y);
    let k = 1.2 * radius / x.hypot(y);
    let p = center.offset(x * k, y * k);
    assert_eq!(step_at(&t.doc, g, p), Some(0));
    // And from CH3PdBr to CH3PdCH3: step 2.
    let c = toward(&t.tm);
    let (x, y) = (b.x + c.x, b.y + c.y);
    let k = 0.8 * radius / x.hypot(y);
    assert_eq!(step_at(&t.doc, g, center.offset(x * k, y * k)), Some(1));
}

#[test]
fn a_reference_reagent_stays_out_of_the_step_s_copy() {
    let t = toy(false);
    let mut doc = t.doc.clone();
    crate::purpose::set(&mut doc, &t.meli, crate::purpose::Purpose::Reference);
    let (copy, _) = step(&doc, t.id, 1).unwrap();
    assert!(t.meli.iter().all(|id| copy.atom(*id).is_none()));
    assert_eq!(copy.reactions[0].reactants.len(), 1);
}

#[test]
fn recoloring_clears_colors_of_steps_that_lost_their_maps() {
    let t = toy(true);
    let mut doc = t.doc.clone();
    color(&mut doc, t.id, 0).unwrap();
    assert!(label_color(&doc, t.libr[1]).is_some_and(|c| c != Color::Ink));
    // Step 2's products lose their maps; coloring again leaves them plain.
    for id in t.libr.iter().chain(&t.tm).chain(&t.ethane) {
        doc.atom_mut(*id).unwrap().map_num = 0;
    }
    color(&mut doc, t.id, 0).unwrap();
    assert!(label_color(&doc, t.libr[1]).is_none_or(|c| c == Color::Ink));
    assert!(
        doc.bonds
            .iter()
            .filter(|b| t.ethane.contains(&b.a))
            .all(|b| b.color == Color::Ink && b.highlight.is_none())
    );
}

#[test]
fn coloring_one_chain_keeps_the_colors_of_a_chain_cut_off_by_text() {
    let mut doc = Document::default();
    let text = |doc: &mut Document, x: f32, y: f32| {
        let id = doc.next_id();
        doc.annotations.push(Annotation {
            id,
            position: Point::new(x, y),
            text: "[M]".into(),
            format: TextFormat::default(),
        });
        id
    };
    let s0 = molecule(&mut doc, "CO", 0., -300., &[1, 2]);
    let t1 = text(&mut doc, 300., 0.);
    let s2 = molecule(&mut doc, "CN", 0., 300., &[3, 4]);
    let t3 = text(&mut doc, -300., 0.);
    let mut ids = s0.clone();
    ids.extend(&s2);
    ids.extend([t1, t3]);
    let (id, _) = make(&mut doc, &ids).unwrap();
    // Steps: s0 → [M] → s2 → [M] → s0; each structure has a partner that
    // shares its maps on the other side of its step.
    let p0 = molecule(&mut doc, "CO", 300., -300., &[1, 2]);
    join(&mut doc, id, Role::Product(0), &p0);
    let r1 = molecule(&mut doc, "CN", 300., 300., &[3, 4]);
    join(&mut doc, id, Role::Reagent(1), &r1);
    let p2 = molecule(&mut doc, "CN", -300., 300., &[3, 4]);
    join(&mut doc, id, Role::Product(2), &p2);
    let r3 = molecule(&mut doc, "CO", -300., -300., &[1, 2]);
    join(&mut doc, id, Role::Reagent(3), &r3);
    color(&mut doc, id, 0).unwrap();
    let oxygen = label_color(&doc, s0[1]);
    assert!(oxygen.is_some_and(|c| c != Color::Ink));
    color(&mut doc, id, 1).unwrap();
    assert!(label_color(&doc, s2[1]).is_some_and(|c| c != Color::Ink));
    assert_eq!(label_color(&doc, s0[1]), oxygen);
}

#[test]
fn colors_go_back_on_their_rings_in_place() {
    let mut doc = Document::default();
    let ring = molecule(&mut doc, "C1CCCCC1", 0., 0., &[]);
    let other = molecule(&mut doc, "C1CCCCC1", 400., 0., &[]);
    let fill = |atoms: &[u64], hue| crate::ring_fills::RingFill {
        atoms: atoms.to_vec(),
        color: Color::Palette(hue, crate::palette::Row::Tint),
        effect: Default::default(),
    };
    use crate::palette::Hue;
    doc.ring_fills = vec![fill(&ring, Hue::Blue), fill(&other, Hue::Green)];
    let mut from = doc.clone();
    let mut reaction = Reaction::new(doc.next_id());
    reaction.reactants.push(Participant {
        atoms: ring.clone(),
        coefficient: 1,
    });
    from.reactions = vec![reaction];
    // The same ring, walked from another atom the other way round.
    let mut walked = ring.clone();
    walked.rotate_left(2);
    walked.reverse();
    from.ring_fills = vec![fill(&walked, Hue::Red), fill(&other, Hue::Teal)];
    super::copy_colors(&mut doc, &from);
    assert_eq!(doc.ring_fills.len(), 2);
    assert_eq!(
        doc.ring_fills[0].color,
        Color::Palette(Hue::Red, crate::palette::Row::Tint)
    );
    // A ring outside the copy's reactions keeps its fill.
    assert_eq!(doc.ring_fills[1], fill(&other, Hue::Green));
}

#[test]
fn every_step_maps_in_turn_and_the_maps_agree_around_the_cycle() {
    let t = toy(false);
    let mut doc = t.doc.clone();
    for s in 0..3 {
        let (copy, arrows) = reactions(&doc, t.id).unwrap();
        let proposal = propose(&copy, arrows[s], Budget::default()).unwrap();
        copy_maps(&mut doc, &proposal.apply(&copy, true).unwrap());
    }
    let map = |id: u64| doc.atom(id).unwrap().map_num;
    // Pd keeps one number through every intermediate.
    assert!(map(t.pd[0]) != 0);
    assert_eq!(map(t.oa[1]), map(t.pd[0]));
    assert_eq!(map(t.tm[1]), map(t.pd[0]));
    // Each methyl ends up in ethane.
    let ethane = [map(t.ethane[0]), map(t.ethane[1])];
    assert!(ethane.contains(&map(t.mebr[0])) && ethane.contains(&map(t.meli[0])));
}
