use super::*;
use crate::{
    document::{Annotation, Arrow, Point},
    editing,
};

fn drawing(start: Point, end: Point) -> (Document, Vec<u64>, Vec<u64>) {
    let mut doc = Document::default();
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let mut rings = Vec::new();
    for center in [-0.5, 1.5] {
        let atoms: Vec<_> = [
            (0.15, 0.),
            (0.075, 0.13),
            (-0.075, 0.13),
            (-0.15, 0.),
            (-0.075, -0.13),
            (0.075, -0.13),
        ]
        .into_iter()
        .map(|(x, y)| {
            doc.add_atom(
                "C",
                Point::new(
                    start.x + dx * (center + x) - dy * y,
                    start.y + dy * (center + x) + dx * y,
                ),
            )
        })
        .collect();
        for i in 0..6 {
            let order = if center < 0. && i % 2 == 0 { 2 } else { 1 };
            doc.add_bond(atoms[i], atoms[(i + 1) % 6], order, "plain");
        }
        rings.push(atoms);
    }
    doc.arrows.push(Arrow::new(
        doc.next_id(),
        start,
        end,
        Default::default(),
        Default::default(),
    ));
    doc.validate().unwrap();
    (doc, rings.remove(0), rings.remove(0))
}

fn horizontal() -> (Document, Vec<u64>, Vec<u64>) {
    drawing(Point::default(), Point::new(100., 0.))
}

fn inferred(doc: &Document) -> Reaction {
    copy_reaction(doc, &editing::selection(doc, &doc.all_ids()))
        .unwrap()
        .unwrap()
}

#[test]
fn ring_roles_follow_arrow_start_and_end_in_every_direction_without_mutation() {
    for end in [
        Point::new(100., 0.),
        Point::new(-100., 0.),
        Point::new(0., 100.),
        Point::new(0., -100.),
        Point::new(80., 60.),
        Point::new(-80., -60.),
    ] {
        let start = Point::new(1000., -200.);
        let (doc, reactants, products) = drawing(start, end.offset(start.x, start.y));
        let before = doc.clone();
        let snapshot = editing::selection(&doc, &doc.all_ids());
        let snapshot_before = snapshot.clone();
        let reaction = copy_reaction(&doc, &snapshot).unwrap().unwrap();
        assert_eq!(reaction.reactants[0].atoms, reactants);
        assert_eq!(reaction.products[0].atoms, products);
        assert_eq!(reaction.reactants[0].coefficient, 1);
        assert_eq!(reaction.products[0].coefficient, 1);
        assert!(reaction.agents.is_empty());
        let mut chemical_copy = snapshot.clone();
        chemical_copy.reactions = vec![reaction];
        chemical_copy.validate().unwrap();
        assert_eq!(doc, before);
        assert_eq!(snapshot, snapshot_before);
    }
}

#[test]
fn every_disconnected_component_becomes_a_participant_in_source_order() {
    let (mut doc, reactants, products) = horizontal();
    let extra_product = doc.add_atom("O", Point::new(140., -100.));
    let extra_reactant = doc.add_atom("N", Point::new(-40., 100.));
    let reaction = inferred(&doc);
    assert_eq!(reaction.reactants.len(), 2);
    assert_eq!(reaction.products.len(), 2);
    assert_eq!(reaction.reactants[0].atoms, reactants);
    assert_eq!(reaction.reactants[1].atoms, [extra_reactant]);
    assert_eq!(reaction.products[0].atoms, products);
    assert_eq!(reaction.products[1].atoms, [extra_product]);
    let mut copied: Vec<_> = Role::ALL
        .into_iter()
        .flat_map(|role| reaction.participants(role))
        .flat_map(|p| p.atoms.iter().copied())
        .collect();
    copied.sort_unstable();
    assert_eq!(copied, doc.atoms.iter().map(|a| a.id).collect::<Vec<_>>());
}

#[test]
fn explicit_roles_coefficients_agents_and_annotations_override_geometry() {
    let (mut doc, left, right) = horizontal();
    let agent = doc.add_atom("O", Point::new(50., -60.));
    let caption = doc.next_id();
    doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(50., -30.),
        text: "conditions".into(),
        format: Default::default(),
    });
    let mut explicit = Reaction::new(doc.arrows[0].id);
    explicit.reactants.push(Participant {
        atoms: right,
        coefficient: 2,
    });
    explicit.products.push(Participant {
        atoms: left,
        coefficient: 1,
    });
    explicit.agents.push(Participant {
        atoms: vec![agent],
        coefficient: 1,
    });
    explicit.annotations.push(caption);
    doc.reactions.push(explicit.clone());
    let before = doc.clone();
    assert_eq!(inferred(&doc), explicit);
    assert_eq!(doc, before);

    doc.add_atom("C", Point::new(-120., 0.));
    assert!(
        copy_reaction(&doc, &doc)
            .unwrap_err()
            .contains("only the defined reaction")
    );
}

#[test]
fn pruned_partial_explicit_roles_never_trigger_geometric_inference() {
    let (mut doc, left, right) = horizontal();
    let arrow = doc.arrows[0].id;
    assign(&mut doc, arrow, &right, Role::Reactant).unwrap();
    assign(&mut doc, arrow, &left, Role::Product).unwrap();
    let mut ids = left.clone();
    ids.push(arrow);
    let snapshot = editing::selection(&doc, &ids);
    assert!(snapshot.reactions.is_empty());
    assert!(
        copy_reaction(&doc, &snapshot)
            .unwrap_err()
            .contains("complete defined reaction")
    );

    let caption = doc.next_id();
    doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(50., -30.),
        text: "conditions".into(),
        format: Default::default(),
    });
    doc.reactions[0].annotations.push(caption);
    let ids: Vec<_> = doc
        .all_ids()
        .into_iter()
        .filter(|id| *id != caption)
        .collect();
    let snapshot = editing::selection(&doc, &ids);
    assert!(snapshot.reactions.is_empty());
    assert!(
        copy_reaction(&doc, &snapshot)
            .unwrap_err()
            .contains("complete defined reaction")
    );
}

#[test]
fn no_arrow_copy_keeps_one_complete_explicit_participant_but_not_a_reaction_mixture() {
    let (mut doc, left, right) = horizontal();
    let arrow = doc.arrows[0].id;
    assign(&mut doc, arrow, &left, Role::Reactant).unwrap();
    assign(&mut doc, arrow, &right, Role::Product).unwrap();
    let other = doc.add_atom("O", Point::new(-120., 0.));
    assign(&mut doc, arrow, &[other], Role::Reactant).unwrap();
    let before = doc.clone();
    for ids in [left.clone(), right.clone(), vec![other]] {
        let snapshot = editing::selection(&doc, &ids);
        assert!(snapshot.reactions.is_empty());
        assert_eq!(copy_reaction(&doc, &snapshot), Ok(None));
    }
    for ids in [
        left.iter().chain(&right).copied().collect(),
        left.iter().copied().chain([other]).collect(),
        vec![left[0]],
    ] {
        let snapshot = editing::selection(&doc, &ids);
        let snapshot_before = snapshot.clone();
        assert!(snapshot.reactions.is_empty());
        assert!(
            copy_reaction(&doc, &snapshot)
                .unwrap_err()
                .contains("one complete reaction participant")
        );
        assert_eq!(snapshot, snapshot_before);
    }
    assert_eq!(doc, before);

    let unassigned = doc.add_atom("N", Point::new(300., 0.));
    let snapshot = editing::selection(&doc, &[unassigned]);
    assert_eq!(copy_reaction(&doc, &snapshot), Ok(None));
    let mut mixed = left;
    mixed.push(unassigned);
    let snapshot = editing::selection(&doc, &mixed);
    assert!(copy_reaction(&doc, &snapshot).is_err());
}

#[test]
fn no_arrow_unassigned_molecules_still_copy_as_an_ordinary_mixture() {
    let (doc, left, right) = horizontal();
    let ids: Vec<_> = left.into_iter().chain(right).collect();
    let snapshot = editing::selection(&doc, &ids);
    assert!(snapshot.arrows.is_empty());
    assert_eq!(copy_reaction(&doc, &snapshot), Ok(None));
    assert_eq!(snapshot.atoms.len(), 12);
    assert_eq!(snapshot.bonds.len(), 12);
}

#[test]
fn partial_molecules_are_rejected_without_adding_unselected_atoms() {
    let (doc, left, right) = horizontal();
    let mut ids = right;
    ids.extend([left[0], doc.arrows[0].id]);
    let snapshot = editing::selection(&doc, &ids);
    let before = snapshot.clone();
    assert!(
        copy_reaction(&doc, &snapshot)
            .unwrap_err()
            .contains("complete molecules")
    );
    assert_eq!(snapshot, before);
}

#[test]
fn midpoint_ties_straddling_and_unexplained_components_are_ambiguous() {
    let (original, left, _) = horizontal();
    for crossing in [false, true] {
        let mut doc = original.clone();
        doc.atom_mut(left[0]).unwrap().position = Point::new(if crossing { 80. } else { 50. }, 0.);
        assert!(copy_reaction(&doc, &doc).unwrap_err().contains("midpoint"));
    }
    let mut doc = original;
    doc.add_atom("O", Point::new(50., -100.));
    assert!(copy_reaction(&doc, &doc).unwrap_err().contains("midpoint"));
}

#[test]
fn absent_multiple_degenerate_unsupported_arrows_and_missing_sides_are_distinct() {
    let (original, left, _) = horizontal();
    let snapshot = editing::selection(&original, &left);
    assert_eq!(copy_reaction(&original, &snapshot), Ok(None));
    let mut doc = original.clone();
    let mut extra = doc.arrows[0].clone();
    extra.id = doc.next_id();
    doc.arrows.push(extra);
    assert!(
        copy_reaction(&doc, &doc)
            .unwrap_err()
            .contains("one reaction arrow")
    );
    let mut doc = original.clone();
    doc.arrows[0].end = doc.arrows[0].start;
    assert!(
        copy_reaction(&doc, &doc)
            .unwrap_err()
            .contains("distinct start and end")
    );
    for preset in crate::arrows::Preset::ALL
        .iter()
        .copied()
        .filter(|p| *p != crate::arrows::Preset::Forward)
    {
        let mut doc = original.clone();
        doc.arrows[0].kind = preset.kind().into();
        doc.arrows[0].style = Some(crate::arrows::ArrowStyle::preset(preset));
        assert!(
            copy_reaction(&doc, &doc)
                .unwrap_err()
                .contains("straight forward")
        );
    }
    let mut doc = original.clone();
    doc.arrows[0].style.as_mut().unwrap().tail = crate::arrows::Head::Full;
    assert!(
        copy_reaction(&doc, &doc)
            .unwrap_err()
            .contains("straight forward")
    );
    let mut ids = left;
    ids.push(original.arrows[0].id);
    let snapshot = editing::selection(&original, &ids);
    assert!(
        copy_reaction(&original, &snapshot)
            .unwrap_err()
            .contains("starting materials")
    );
}

#[test]
fn collapsed_members_and_semantic_attachment_members_remain_complete() {
    let (mut doc, left, right) = horizontal();
    doc.contract(&left, "Ph", "Ph").unwrap();
    let anchor = doc.abbreviations[0].anchor;
    let hidden = *left.iter().find(|id| **id != anchor).unwrap();
    doc.atom_mut(hidden).unwrap().position = Point::new(80., 0.);
    let mut ids = right;
    ids.extend([anchor, doc.arrows[0].id]);
    let snapshot = editing::selection(&doc, &ids);
    let reaction = copy_reaction(&doc, &snapshot).unwrap().unwrap();
    assert_eq!(reaction.reactants[0].atoms, left);

    let (mut doc, mut left, _) = horizontal();
    let attachment =
        crate::attachments::add(&mut doc, &left[..2], crate::attachments::Kind::MultiCenter)
            .unwrap();
    left.push(attachment);
    assert_eq!(inferred(&doc).reactants[0].atoms, left);
}
