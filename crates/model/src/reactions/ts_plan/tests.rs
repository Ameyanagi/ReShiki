use super::*;
use crate::{
    document::{Arrow, Point},
    reactions::Reaction,
};

fn molecule(
    doc: &mut Document,
    atoms: &[(&str, u32)],
    bonds: &[(u32, u32, u8)],
) -> BTreeMap<u32, u64> {
    let map: BTreeMap<_, _> = atoms
        .iter()
        .map(|&(element, number)| {
            let id = doc.add_atom(element, Point::new(doc.atoms.len() as f32 * 28., 0.));
            doc.atom_mut(id).unwrap().map_num = number;
            (number, id)
        })
        .collect();
    for &(a, b, order) in bonds {
        doc.add_bond(map[&a], map[&b], order, "plain");
    }
    map
}

fn assign(
    doc: &mut Document,
    reactants: &[&BTreeMap<u32, u64>],
    products: &[&BTreeMap<u32, u64>],
) -> u64 {
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::default(),
        Point::new(100., 0.),
        Default::default(),
        Default::default(),
    ));
    let participants = |molecules: &[&BTreeMap<u32, u64>]| {
        molecules
            .iter()
            .map(|mol| Participant {
                atoms: mol.values().copied().collect(),
                coefficient: 1,
            })
            .collect()
    };
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = participants(reactants);
    reaction.products = participants(products);
    doc.reactions.push(reaction);
    doc.validate().unwrap();
    arrow
}

fn claisen() -> (Document, u64, BTreeMap<u32, u64>) {
    let mut doc = Document::default();
    let atoms = [("C", 1), ("C", 2), ("O", 3), ("C", 4), ("C", 5), ("C", 6)];
    let left = molecule(
        &mut doc,
        &atoms,
        &[(1, 2, 2), (2, 3, 1), (3, 4, 1), (4, 5, 1), (5, 6, 2)],
    );
    let right = molecule(
        &mut doc,
        &atoms,
        &[(2, 3, 2), (1, 2, 1), (1, 6, 1), (5, 6, 1), (4, 5, 2)],
    );
    let arrow = assign(&mut doc, &[&left], &[&right]);
    (doc, arrow, left)
}

#[test]
fn claisen_starts_on_reactants_and_ignores_order_changes_and_agents() {
    let (mut doc, arrow, left) = claisen();
    let agent = doc.add_atom("*", Point::new(50., -50.));
    doc.reactions[0].agents.push(Participant {
        atoms: vec![agent],
        coefficient: 3,
    });
    doc.atom_mut(agent).unwrap().map_num = 1;
    let before = doc.clone();
    let result = plan(&doc, arrow).unwrap();
    assert_eq!(
        result,
        TsPlan {
            start: Side::Reactant,
            start_atoms: left.values().copied().collect(),
            mapped: vec![1, 2, 3, 4, 5, 6],
            mapped_hydrogens: vec![],
            form: vec![(left[&1], left[&6])],
            break_: vec![(left[&3], left[&4])],
            hydrogen_moves: vec![],
            shape: (1, 1),
            drawn_charges: (0, 0),
            atoms: 14,
        }
    );
    assert_eq!(doc, before);
}

#[test]
fn diels_alder_starts_on_product_in_both_arrow_directions() {
    let mut doc = Document::default();
    let diene = molecule(
        &mut doc,
        &[("C", 1), ("C", 2), ("C", 3), ("C", 4)],
        &[(1, 2, 2), (2, 3, 1), (3, 4, 2)],
    );
    let ethylene = molecule(&mut doc, &[("C", 5), ("C", 6)], &[(5, 6, 2)]);
    let ring_atoms: Vec<_> = (1..=6).map(|number| ("C", number)).collect();
    let ring = molecule(
        &mut doc,
        &ring_atoms,
        &[
            (1, 2, 1),
            (2, 3, 2),
            (3, 4, 1),
            (4, 5, 1),
            (5, 6, 1),
            (6, 1, 1),
        ],
    );
    let arrow = assign(&mut doc, &[&diene, &ethylene], &[&ring]);
    let forward = plan(&doc, arrow).unwrap();
    assert_eq!(forward.start, Side::Product);
    assert_eq!(forward.shape, (2, 1));
    assert_eq!(
        forward.start_atoms,
        ring.values().copied().collect::<Vec<_>>()
    );
    assert_eq!(forward.break_, [(ring[&1], ring[&6]), (ring[&4], ring[&5])]);
    assert!(forward.form.is_empty());
    assert!(forward.hydrogen_moves.is_empty());
    assert_eq!(forward.atoms, 16);

    let reaction = &mut doc.reactions[0];
    std::mem::swap(&mut reaction.reactants, &mut reaction.products);
    assert_eq!(
        plan(&doc, arrow).unwrap(),
        TsPlan {
            start: Side::Reactant,
            shape: (1, 2),
            ..forward
        }
    );
}

fn tautomer(drawn_h: bool) -> (Document, u64, BTreeMap<u32, u64>, BTreeMap<u32, u64>) {
    let mut doc = Document::default();
    let mut atoms = vec![("C", 1), ("C", 2), ("O", 3), ("C", 4)];
    let mut keto = vec![(1, 2, 1), (2, 3, 2), (2, 4, 1)];
    let mut enol = vec![(1, 2, 2), (2, 3, 1), (2, 4, 1)];
    if drawn_h {
        atoms.push(("H", 5));
        keto.push((1, 5, 1));
        enol.push((3, 5, 1));
    }
    let left = molecule(&mut doc, &atoms, &keto);
    let right = molecule(&mut doc, &atoms, &enol);
    let arrow = assign(&mut doc, &[&left], &[&right]);
    (doc, arrow, left, right)
}

#[test]
fn keto_enol_counts_implicit_explicit_and_unmapped_drawn_hydrogens() {
    for drawn_h in [false, true] {
        let (mut doc, arrow, left, right) = tautomer(drawn_h);
        if drawn_h {
            for id in [left[&5], right[&5]] {
                doc.atom_mut(id).unwrap().map_num = 0;
            }
        }
        let expected = plan(&doc, arrow).unwrap();
        assert_eq!(expected.start, Side::Reactant);
        assert_eq!(
            expected.start_atoms,
            left.values().copied().collect::<Vec<_>>()
        );
        assert_eq!(expected.shape, (1, 1));
        assert_eq!(expected.atoms, 10);
        assert_eq!(expected.mapped, [1, 2, 3, 4]);
        assert!(expected.form.is_empty());
        assert!(expected.break_.is_empty());
        assert_eq!(expected.hydrogen_moves, [(left[&1], left[&3])]);

        let donor = doc.atom_mut(left[&1]).unwrap();
        donor.explicit_h = if drawn_h { 2 } else { 3 };
        donor.no_implicit = true;
        donor.label_h = 99;
        assert_eq!(plan(&doc, arrow).unwrap(), expected);
    }
}

#[test]
fn mapped_drawn_hydrogen_changes_are_bonds() {
    let (doc, arrow, left, _) = tautomer(true);
    let result = plan(&doc, arrow).unwrap();
    assert_eq!(result.atoms, 10);
    assert_eq!(result.mapped, [1, 2, 3, 4, 5]);
    assert_eq!(result.mapped_hydrogens, [left[&5]]);
    assert_eq!(result.form, [(left[&3], left[&5])]);
    assert_eq!(result.break_, [(left[&1], left[&5])]);
    assert!(result.hydrogen_moves.is_empty());
}

#[test]
fn inferred_transfer_requires_mapping_an_isotope_labelled_donor_hydrogen() {
    let (mut doc, arrow, left, right) = tautomer(true);
    for id in [left[&5], right[&5]] {
        let atom = doc.atom_mut(id).unwrap();
        atom.isotope = 2;
        atom.map_num = 0;
    }
    assert_eq!(
        plan(&doc, arrow),
        Err(TsPlanError::Unsupported(
            "Map isotope-labelled hydrogens before searching".into()
        ))
    );
    for id in [left[&5], right[&5]] {
        doc.atom_mut(id).unwrap().map_num = 5;
    }
    let plan = plan(&doc, arrow).unwrap();
    assert!(plan.hydrogen_moves.is_empty());
    assert_eq!(plan.mapped_hydrogens, [left[&5]]);
    assert_eq!(plan.break_, [(left[&1], left[&5])]);
    assert_eq!(plan.form, [(left[&3], left[&5])]);
}

#[test]
fn hydrogen_moves_use_nearest_start_positions_then_map_order() {
    let mut doc = Document::default();
    let atoms = [("C", 1), ("C", 2), ("O", 3), ("O", 4), ("C", 5), ("C", 6)];
    let left = molecule(
        &mut doc,
        &atoms,
        &[(1, 5, 1), (5, 3, 2), (5, 6, 1), (6, 4, 2), (6, 2, 1)],
    );
    let right = molecule(
        &mut doc,
        &atoms,
        &[(1, 5, 2), (5, 3, 1), (5, 6, 1), (6, 4, 1), (6, 2, 2)],
    );
    let arrow = assign(&mut doc, &[&left], &[&right]);
    for (map, x, y) in [(1, 0., 0.), (2, 2., 0.), (3, 2., 1.), (4, 0., -1.)] {
        doc.atom_mut(left[&map]).unwrap().position = Point::new(x, y);
    }
    let result = plan(&doc, arrow).unwrap();
    assert_eq!(
        result.hydrogen_moves,
        [(left[&1], left[&4]), (left[&2], left[&3])]
    );

    doc.atom_mut(left[&3]).unwrap().position = Point::new(0., 1.);
    let result = plan(&doc, arrow).unwrap();
    assert_eq!(
        result.hydrogen_moves,
        [(left[&1], left[&3]), (left[&2], left[&4])]
    );
}

#[test]
fn one_pair_is_emitted_per_hydrogen() {
    let mut doc = Document::default();
    let atoms = [("C", 1), ("C", 2), ("C", 3), ("C", 4)];
    let left = molecule(&mut doc, &atoms, &[(1, 2, 1), (2, 3, 3), (3, 4, 1)]);
    let right = molecule(&mut doc, &atoms, &[(1, 2, 3), (2, 3, 1), (3, 4, 1)]);
    let arrow = assign(&mut doc, &[&left], &[&right]);
    assert_eq!(
        plan(&doc, arrow).unwrap().hydrogen_moves,
        [(left[&1], left[&3]); 2]
    );
}

#[test]
fn missing_or_incomplete_roles_require_assignment() {
    let (mut doc, arrow, _) = claisen();
    assert_eq!(plan(&doc, 0), Err(TsPlanError::NotReaction));
    doc.reactions[0].products.clear();
    assert_eq!(plan(&doc, arrow), Err(TsPlanError::NotReaction));
}

#[test]
fn unmapped_counts_cover_both_sides() {
    let (mut doc, arrow, left) = claisen();
    doc.atom_mut(left[&1]).unwrap().map_num = 0;
    assert_eq!(
        plan(&doc, arrow),
        Err(TsPlanError::Unmapped {
            mapped: 11,
            heavy: 12
        })
    );
    for atom in &mut doc.atoms {
        atom.map_num = 0;
    }
    assert_eq!(
        plan(&doc, arrow),
        Err(TsPlanError::Unmapped {
            mapped: 0,
            heavy: 12
        })
    );
}

#[test]
fn two_to_two_is_unsupported_even_when_components_share_one_participant() {
    let mut doc = Document::default();
    let a = molecule(&mut doc, &[("C", 1)], &[]);
    let b = molecule(&mut doc, &[("O", 2)], &[]);
    let c = molecule(&mut doc, &[("C", 1)], &[]);
    let d = molecule(&mut doc, &[("O", 2)], &[]);
    let arrow = assign(&mut doc, &[&a, &b], &[&c, &d]);
    let expected = Err(TsPlanError::Shape {
        reactants: 2,
        products: 2,
    });
    assert_eq!(plan(&doc, arrow), expected);
    let grouped = doc.reactions[0].products.pop().unwrap();
    doc.reactions[0].products[0].atoms.extend(grouped.atoms);
    assert_eq!(plan(&doc, arrow), expected);
}

#[test]
fn atom_identity_and_map_count_mismatches_name_the_map() {
    let (doc, arrow, left) = claisen();
    for (map, edit) in [
        (1, (|a: &mut Atom| a.element = "N".into()) as fn(&mut Atom)),
        (1, |a: &mut Atom| a.isotope = 13),
        (7, |a: &mut Atom| a.map_num = 7),
        (2, |a: &mut Atom| a.map_num = 2),
    ] {
        let mut edited = doc.clone();
        edit(edited.atom_mut(left[&1]).unwrap());
        let Err(TsPlanError::Mismatch(message)) = plan(&edited, arrow) else {
            panic!("expected an atom map mismatch");
        };
        assert!(message.contains(&format!("Atom map {map} ")), "{message}");
    }
}

#[test]
fn hydrogen_and_total_charge_balance_are_checked() {
    let (mut doc, arrow, left, _) = tautomer(false);
    let donor = doc.atom_mut(left[&1]).unwrap();
    donor.explicit_h = 2;
    donor.no_implicit = true;
    assert_eq!(plan(&doc, arrow), Err(TsPlanError::HydrogenBalance));
    doc.atom_mut(left[&1]).unwrap().charge = -1;
    let Err(TsPlanError::Mismatch(message)) = plan(&doc, arrow) else {
        panic!("expected a charge mismatch");
    };
    assert!(message.contains("Total charge"), "{message}");
}

#[test]
fn atom_limit_includes_implicit_hydrogens() {
    let mut doc = Document::default();
    let atoms: Vec<_> = (1..=40).map(|n| ("C", n)).collect();
    let bonds: Vec<_> = (1..40).map(|n| (n, n + 1, 1)).collect();
    let left = molecule(&mut doc, &atoms, &bonds);
    let right = molecule(&mut doc, &atoms, &bonds);
    let arrow = assign(&mut doc, &[&left], &[&right]);
    assert_eq!(plan(&doc, arrow), Err(TsPlanError::TooLarge { atoms: 122 }));
    doc.add_bond(left[&1], left[&2], 2, "plain");
    doc.add_bond(right[&1], right[&2], 2, "plain");
    assert_eq!(plan(&doc, arrow), Err(TsPlanError::NoChange));
}

#[test]
fn identical_sides_and_bond_order_changes_alone_have_no_search_targets() {
    let mut doc = Document::default();
    let atoms = [("C", 1), ("O", 2), ("O", 3), ("C", 4)];
    let bonds = [(1, 2, 2), (1, 3, 1), (1, 4, 1)];
    let left = molecule(&mut doc, &atoms, &bonds);
    let right = molecule(&mut doc, &atoms, &bonds);
    for id in [left[&3], right[&3]] {
        doc.atom_mut(id).unwrap().charge = -1;
    }
    let arrow = assign(&mut doc, &[&left], &[&right]);
    assert_eq!(plan(&doc, arrow), Err(TsPlanError::NoChange));
    doc.add_bond(right[&1], right[&2], 1, "plain");
    doc.add_bond(right[&1], right[&3], 2, "plain");
    doc.atom_mut(right[&2]).unwrap().charge = -1;
    doc.atom_mut(right[&3]).unwrap().charge = 0;
    assert_eq!(plan(&doc, arrow), Err(TsPlanError::NoChange));
}

#[test]
fn unsupported_drawing_features_fail_before_planning() {
    let (doc, arrow, left) = claisen();
    let mut query = doc.clone();
    query.atom_mut(left[&1]).unwrap().element = "*".into();
    assert!(matches!(
        plan(&query, arrow),
        Err(TsPlanError::Unsupported(_))
    ));
    let mut contracted = doc.clone();
    contracted
        .contract(&[left[&1], left[&2]], "Vinyl", "")
        .unwrap();
    assert!(matches!(
        plan(&contracted, arrow),
        Err(TsPlanError::Unsupported(_))
    ));
    for order in [0, 5] {
        let mut modified = doc.clone();
        modified.bonds[0].order = order;
        assert!(matches!(
            plan(&modified, arrow),
            Err(TsPlanError::Unsupported(_))
        ));
    }
}
