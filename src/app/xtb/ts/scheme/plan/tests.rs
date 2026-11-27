use super::*;
use reshiki::arrows::{ArrowStyle, Preset};
use reshiki::document::Arrow;

fn molecule_at(
    doc: &mut Document,
    elements: &[&str],
    bonds: &[(usize, usize, u8)],
    x: f32,
) -> Vec<u64> {
    let atoms: Vec<_> = elements
        .iter()
        .enumerate()
        .map(|(index, element)| doc.add_atom(element, Point::new(x + index as f32 * 36., 0.)))
        .collect();
    for &(a, b, order) in bonds {
        doc.add_bond(atoms[a], atoms[b], order, "plain");
    }
    atoms
}

fn step(doc: &mut Document, left: &[(Vec<u64>, u16)], right: &[(Vec<u64>, u16)], x: f32) -> u64 {
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(x, 0.),
        Point::new(x + 80., 0.),
        Preset::Forward,
        ArrowStyle::preset(Preset::Forward),
    ));
    let participants = |side: &[(Vec<u64>, u16)]| {
        side.iter()
            .map(|(atoms, coefficient)| Participant {
                atoms: atoms.clone(),
                coefficient: *coefficient,
            })
            .collect()
    };
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = participants(left);
    reaction.products = participants(right);
    doc.reactions.push(reaction);
    arrow
}

fn one(atoms: &[u64]) -> (Vec<u64>, u16) {
    (atoms.to_vec(), 1)
}

pub(super) fn two_step() -> (Document, Plan) {
    let mut doc = Document::default();
    let ethanol = molecule_at(&mut doc, &["C", "C", "O"], &[(0, 1, 1), (1, 2, 1)], 0.);
    let ethene = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 2)], 260.);
    let ethane = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 1)], 480.);
    let first = step(&mut doc, &[one(&ethanol)], &[one(&ethene)], 150.);
    let second = step(&mut doc, &[one(&ethene)], &[one(&ethane)], 390.);
    let plan = Plan::read(&doc, &[second, first]).unwrap();
    (doc, plan)
}

#[test]
fn order_comes_from_shared_drawn_species_and_balance_is_calculation_only() {
    let (doc, plan) = two_step();
    let before = doc.clone();
    assert_eq!(
        plan.arrows,
        doc.reactions
            .iter()
            .map(|reaction| reaction.arrow)
            .collect::<Vec<_>>()
    );
    assert!(!plan.cyclic);
    let water = plan
        .species
        .iter()
        .position(|species| species.formula == "H2O")
        .unwrap();
    let hydrogen = plan
        .species
        .iter()
        .position(|species| species.formula == "H2")
        .unwrap();
    assert!(
        plan.steps[0]
            .products
            .iter()
            .any(|term| term.species == water)
    );
    assert!(
        plan.steps[1]
            .reactants
            .iter()
            .any(|term| term.species == hydrogen)
    );
    assert_eq!(
        plan.notes
            .iter()
            .filter(|note| note.contains("Added one"))
            .count(),
        2
    );
    assert_eq!(plan.species.len(), 5);
    for species in &plan.species {
        assert_eq!(
            reactions::molecules(
                &species.calculation,
                &species
                    .calculation
                    .atoms
                    .iter()
                    .map(|atom| atom.id)
                    .collect::<Vec<_>>()
            )
            .len(),
            1
        );
        assert!(species.calculation.arrows.is_empty());
        assert!(species.calculation.reactions.is_empty());
    }
    let review = plan.review_document(0, &doc).unwrap();
    assert_eq!(review.reactions[0].products.len(), 2);
    assert_eq!(doc, before);
    assert!(plan.current(&doc));
}

#[test]
fn document_arrow_order_does_not_override_open_path_order() {
    let (mut doc, original) = two_step();
    doc.arrows.reverse();
    let plan = Plan::read(&doc, &original.arrows).unwrap();
    assert_eq!(plan.arrows, original.arrows);
}

#[test]
fn a_three_step_chain_reuses_hydrogen_chloride_on_opposite_sides() {
    let mut doc = Document::default();
    let alcohol = molecule_at(&mut doc, &["C", "C", "O"], &[(0, 1, 1), (1, 2, 1)], 0.);
    let alkene = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 2)], 240.);
    let chloride = molecule_at(&mut doc, &["C", "C", "Cl"], &[(0, 1, 1), (1, 2, 1)], 480.);
    let product = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 2)], 720.);
    let first = step(&mut doc, &[one(&alcohol)], &[one(&alkene)], 120.);
    let second = step(&mut doc, &[one(&alkene)], &[one(&chloride)], 360.);
    let third = step(&mut doc, &[one(&chloride)], &[one(&product)], 600.);
    let plan = Plan::read(&doc, &[third, first, second]).unwrap();
    assert_eq!(plan.arrows, [first, second, third]);
    assert!(!plan.cyclic);
    let hcl = plan.species.iter().position(|s| s.label == "HCl").unwrap();
    for side in [&plan.steps[1].reactants, &plan.steps[2].products] {
        assert!(
            side.iter()
                .any(|term| term.species == hcl && term.coefficient == 1)
        );
    }
    assert!(
        plan.notes
            .iter()
            .any(|note| note.starts_with("Step 2") && note.contains("HCl"))
    );
}

#[test]
fn branches_and_merges_are_explained() {
    let (mut doc, plan) = two_step();
    let intermediate = doc.reactions[0].products[0].atoms.clone();
    let other = molecule_at(&mut doc, &["C"], &[], 700.);
    let branch = step(&mut doc, &[one(&intermediate)], &[one(&other)], 600.);
    let mut arrows = plan.arrows.clone();
    arrows.push(branch);
    assert!(
        Plan::read(&doc, &arrows)
            .unwrap_err()
            .contains("branches or merges")
    );
}

#[test]
fn chemical_copies_do_not_connect_disconnected_drawings() {
    let (mut doc, plan) = two_step();
    let replacement = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 2)], 300.);
    doc.reactions[1].reactants[0].atoms = replacement;
    assert!(
        Plan::read(&doc, &plan.arrows)
            .unwrap_err()
            .contains("share a drawn")
    );
}

#[test]
fn an_unbranched_cycle_starts_at_the_first_document_arrow() {
    let mut doc = Document::default();
    let a = molecule_at(&mut doc, &["C"], &[], 0.);
    let b = molecule_at(&mut doc, &["N"], &[], 240.);
    let c = molecule_at(&mut doc, &["O"], &[], 480.);
    let second = step(&mut doc, &[one(&b)], &[one(&c)], 320.);
    let third = step(&mut doc, &[one(&c)], &[one(&a)], 560.);
    let first = step(&mut doc, &[one(&a)], &[one(&b)], 80.);
    let plan = Plan::read(&doc, &[first, second, third]).unwrap();
    assert!(plan.cyclic);
    assert_eq!(plan.arrows, [second, third, first]);
}

#[test]
fn a_shared_spectator_does_not_create_a_branch() {
    let (mut doc, plan) = two_step();
    let spectator = molecule_at(&mut doc, &["Cl"], &[], -120.);
    for reaction in &mut doc.reactions {
        reaction.reactants.push(Participant {
            atoms: spectator.clone(),
            coefficient: 1,
        });
        reaction.products.push(Participant {
            atoms: spectator.clone(),
            coefficient: 1,
        });
    }
    let updated = Plan::read(&doc, &plan.arrows).unwrap();
    assert_eq!(updated.arrows, plan.arrows);
    assert!(!updated.cyclic);
}

#[test]
fn exactly_one_copy_of_a_drawn_species_can_balance_and_keeps_original_maps() {
    let mut doc = Document::default();
    let formaldehyde = molecule_at(&mut doc, &["C", "O"], &[(0, 1, 2)], 0.);
    doc.atom_mut(formaldehyde[0]).unwrap().map_num = 1;
    let dimer = molecule_at(
        &mut doc,
        &["O", "C", "C", "O"],
        &[(0, 1, 2), (1, 2, 1), (2, 3, 1)],
        240.,
    );
    let another = molecule_at(&mut doc, &["C", "O"], &[(0, 1, 2)], 550.);
    let a = step(&mut doc, &[one(&formaldehyde)], &[one(&dimer)], 120.);
    let b = step(&mut doc, &[one(&dimer)], &[(another, 2)], 450.);
    let plan = Plan::read(&doc, &[a, b]).unwrap();
    assert_eq!(
        plan.species.len(),
        2,
        "separate drawn copies share one calculation input"
    );
    assert_eq!(plan.steps[0].reactants[0].coefficient, 2);
    assert!(plan.notes.iter().any(|note| note.contains("Added one")));
    let review = plan.review_document(0, &doc).unwrap();
    assert_eq!(review.reactions[0].reactants.len(), 2);
    assert_eq!(review.atom(formaldehyde[0]).unwrap().map_num, 1);
    let copied = &review.reactions[0].reactants[1].atoms;
    assert!(
        copied
            .iter()
            .all(|id| review.atom(*id).unwrap().map_num == 0)
    );
}

#[test]
fn equally_balancing_structural_isomers_are_ambiguous() {
    let mut doc = Document::default();
    let a = molecule_at(&mut doc, &["C", "O"], &[(0, 1, 2)], 0.);
    let b = molecule_at(
        &mut doc,
        &["C", "C", "O", "C", "O"],
        &[(0, 1, 1), (1, 2, 1), (2, 3, 1), (3, 4, 1)],
        240.,
    );
    let c = molecule_at(
        &mut doc,
        &["C", "C", "O", "C", "O"],
        &[(0, 1, 1), (1, 2, 1), (2, 3, 1), (3, 4, 1)],
        550.,
    );
    let ethanol = molecule_at(&mut doc, &["C", "C", "O"], &[(0, 1, 1), (1, 2, 1)], -200.);
    let ether = molecule_at(&mut doc, &["C", "O", "C"], &[(0, 1, 1), (1, 2, 1)], -400.);
    let first = step(&mut doc, &[one(&a)], &[one(&b)], 120.);
    let second = step(
        &mut doc,
        &[one(&b), one(&ethanol), one(&ether)],
        &[one(&c), one(&ethanol), one(&ether)],
        450.,
    );
    let plan = Plan::read(&doc, &[first, second]).unwrap();
    assert_eq!(plan.steps[0].reactants.len(), 1);
    assert_eq!(plan.steps[0].products.len(), 1);
    assert!(
        plan.steps[0]
            .notes
            .iter()
            .any(|note| note.contains("More than one molecule"))
    );
    assert!(
        !plan.steps[0]
            .notes
            .iter()
            .any(|note| note.contains("Added one"))
    );
}

#[test]
fn explicit_water_and_builtin_water_are_one_balancing_candidate() {
    let (mut doc, original) = two_step();
    let water = molecule_at(&mut doc, &["H", "O", "H"], &[(0, 1, 1), (1, 2, 1)], -200.);
    doc.reactions[1].reactants.push(Participant {
        atoms: water.clone(),
        coefficient: 1,
    });
    doc.reactions[1].products.push(Participant {
        atoms: water.clone(),
        coefficient: 1,
    });
    let plan = Plan::read(&doc, &original.arrows).unwrap();
    let waters: Vec<_> = plan
        .species
        .iter()
        .filter(|species| species.formula == "H2O")
        .collect();
    assert_eq!(waters.len(), 1);
    assert_eq!(waters[0].atoms, water);
    assert!(
        plan.steps[0]
            .notes
            .iter()
            .any(|note| note.contains("Added one"))
    );
}

#[test]
fn isotopes_are_not_balanced_by_unlabelled_small_molecules() {
    let mut doc = Document::default();
    let a = molecule_at(&mut doc, &["O"], &[], 0.);
    let b = molecule_at(&mut doc, &["O"], &[], 240.);
    let c = molecule_at(&mut doc, &["O"], &[], 480.);
    doc.atom_mut(b[0]).unwrap().isotope = 18;
    doc.atom_mut(c[0]).unwrap().isotope = 18;
    let first = step(&mut doc, &[one(&a)], &[one(&b)], 80.);
    let second = step(&mut doc, &[one(&b)], &[one(&c)], 320.);
    let plan = Plan::read(&doc, &[first, second]).unwrap();
    assert!(
        plan.steps[0]
            .notes
            .iter()
            .any(|note| note.contains("No single supported molecule"))
    );
    assert_eq!(plan.species.len(), 2);
}

#[test]
fn source_snapshot_ignores_calculation_models_and_detects_chemistry_and_map_edits() {
    let (mut doc, plan) = two_step();
    let id = doc.reactions[0].reactants[0].atoms[0];
    doc.atom_mut(id).unwrap().position.x += 10.;
    assert!(
        plan.current(&doc),
        "explicit roles do not change when moved"
    );
    doc.atom_mut(id).unwrap().map_num = 12;
    assert!(!plan.current(&doc));
    assert!(plan.review_document(0, &doc).is_err());
    doc.atom_mut(id).unwrap().map_num = 0;
    assert!(plan.current(&doc));
    doc.atom_mut(id).unwrap().isotope = 13;
    assert!(!plan.current(&doc));
}

#[test]
fn every_supported_builtin_is_a_single_ready_molecule() {
    let candidates = chemistry::candidates().unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|species| species.label.as_str())
            .collect::<Vec<_>>(),
        ["H₂O", "HCl", "NH₃", "MeOH", "HBr", "N₂", "CO₂", "H₂"]
    );
    assert_eq!(
        candidates
            .iter()
            .map(|species| species.formula.as_str())
            .collect::<Vec<_>>(),
        ["H2O", "ClH", "H3N", "CH4O", "BrH", "N2", "CO2", "H2"]
    );
    for species in candidates {
        assert!(species.atoms.is_empty());
        assert_eq!((species.charge, species.multiplicity), (0, 1));
        let ids: Vec<_> = species
            .calculation
            .atoms
            .iter()
            .map(|atom| atom.id)
            .collect();
        assert_eq!(reactions::molecules(&species.calculation, &ids).len(), 1);
    }
}

#[test]
fn compound_numbers_belong_to_drawn_instances_while_the_calculation_is_shared() {
    let mut doc = Document::default();
    let a = molecule_at(&mut doc, &["C"], &[], 0.);
    let b = molecule_at(&mut doc, &["C"], &[], 240.);
    let c = molecule_at(&mut doc, &["C"], &[], 480.);
    for atoms in [&a, &b, &c] {
        reshiki::compound_numbers::number(&mut doc, atoms, Default::default()).unwrap();
    }
    let first = step(&mut doc, &[one(&a)], &[one(&b)], 80.);
    let second = step(&mut doc, &[one(&b)], &[one(&c)], 320.);
    let plan = Plan::read(&doc, &[first, second]).unwrap();
    assert_eq!(plan.species.len(), 1);
    assert_eq!(plan.endpoint_label(0, false), "1");
    assert_eq!(plan.endpoint_label(0, true), "2");
    assert_eq!(plan.endpoint_label(1, true), "3");
    assert_eq!(plan.step_label(1), "2 → 3");
    assert_eq!(plan.title(), "1 → 2 → 3");
}

#[test]
fn neutral_hydrogen_chloride_cannot_fix_a_charge_difference() {
    let mut doc = Document::default();
    let ammonia = molecule_at(&mut doc, &["N"], &[], 0.);
    let ammonium = molecule_at(&mut doc, &["N"], &[], 240.);
    let another = molecule_at(&mut doc, &["N"], &[], 480.);
    for &id in ammonium.iter().chain(&another) {
        let atom = doc.atom_mut(id).unwrap();
        atom.charge = 1;
        atom.explicit_h = 4;
        atom.no_implicit = true;
    }
    let chlorine = molecule_at(&mut doc, &["Cl"], &[], -150.);
    let atom = doc.atom_mut(chlorine[0]).unwrap();
    atom.no_implicit = true;
    atom.radical_electrons = 1;
    let first = step(
        &mut doc,
        &[one(&ammonia)],
        &[one(&ammonium), one(&chlorine)],
        80.,
    );
    let second = step(
        &mut doc,
        &[one(&ammonium), one(&chlorine)],
        &[one(&another), one(&chlorine)],
        320.,
    );
    let plan = Plan::read(&doc, &[first, second]).unwrap();
    assert!(
        plan.steps[0]
            .notes
            .iter()
            .any(|note| note.contains("charge difference of +1"))
    );
    assert!(
        plan.steps[0]
            .notes
            .iter()
            .any(|note| note.contains("No single supported molecule"))
    );
    assert!(
        !plan.steps[0]
            .notes
            .iter()
            .any(|note| note.contains("Added one"))
    );
}

#[test]
fn minimum_labels_include_only_the_next_reactants_coefficient_excess() {
    let (_, plan) = two_step();
    assert_eq!(plan.minimum_label(0), "C2H4 + H₂O + H₂");
    assert_eq!(plan.minimum_label(1), "C2H6");
}

#[test]
fn cached_card_names_keep_drawn_number_precedence_and_do_not_change_the_snapshot() {
    let (mut doc, original) = two_step();
    let intermediate = doc.reactions[0].products[0].atoms.clone();
    reshiki::compound_numbers::number(&mut doc, &intermediate, Default::default()).unwrap();
    let mut plan = Plan::read(&doc, &original.arrows).unwrap();
    let snapshot = plan.snapshot.clone();
    let reactant = doc.reactions[0].reactants[0].atoms.clone();
    let product = doc.reactions[1].products[0].atoms.clone();
    plan.resolve_names(&doc, |atoms| {
        if atoms == reactant.as_slice() {
            Some("ethanol".into())
        } else if atoms == intermediate.as_slice() {
            Some("ethene".into())
        } else if atoms == product.as_slice() {
            Some("ethane".into())
        } else {
            None
        }
    });
    assert_eq!(plan.endpoint_label(0, false), "ethanol");
    assert_eq!(plan.endpoint_label(0, true), "1 + H₂O");
    assert_eq!(plan.endpoint_label(1, true), "ethane");
    assert_eq!(plan.snapshot, snapshot);
    assert!(plan.current(&doc));
}
