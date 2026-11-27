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
            .filter(|note| note.starts_with("Balanced by adding"))
            .count(),
        2
    );
    assert_eq!(plan.species.len(), 5);
    assert_eq!(
        plan.notes,
        [
            "Balanced by adding H₂O to step 1.",
            "Balanced by adding H₂ to step 2."
        ]
    );
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
    assert!(plan.steps.iter().all(Step::has_added_copies));
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
            .any(|note| note == "Balanced by adding HCl to step 2.")
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
fn inferred_scheme_agents_above_and_below_the_shaft_do_not_become_endpoints() {
    let mut doc = Document::default();
    let a = molecule_at(&mut doc, &["C"], &[], -80.);
    let b = molecule_at(&mut doc, &["N"], &[], 180.);
    let c = molecule_at(&mut doc, &["O"], &[], 440.);
    let above = molecule_at(&mut doc, &["O"], &[], 30.);
    let below = molecule_at(&mut doc, &["O"], &[], 70.);
    doc.atom_mut(above[0]).unwrap().position.y = -20.;
    doc.atom_mut(below[0]).unwrap().position.y = 20.;
    let first = step(&mut doc, &[one(&a)], &[one(&b)], 0.);
    let second = step(&mut doc, &[one(&b)], &[one(&c)], 260.);
    doc.arrows[0].end.x = 100.;
    doc.arrows[1].end.x = 360.;
    doc.reactions.clear();
    let shared = reactions::preview(&doc, first).unwrap();
    assert!(shared.reactants.iter().any(|term| term.atoms == above));
    assert!(shared.products.iter().any(|term| term.atoms == below));
    let inferred = preview(&doc, first).unwrap();
    assert_eq!(inferred.reactants[0].atoms, a);
    assert_eq!(inferred.products[0].atoms, b);
    assert_eq!(inferred.agents.len(), 2);
    let plan = Plan::read(&doc, &[second, first]).unwrap();
    assert_eq!(plan.steps[0].reactants.len(), 1);
    assert_eq!(plan.steps[0].products.len(), 1);
    assert_eq!(plan.species.len(), 3);
    assert!(plan.current(&doc));
    let mut explicit = inferred;
    explicit.reactants.push(explicit.agents.remove(0));
    doc.reactions.push(explicit.clone());
    assert_eq!(preview(&doc, first).unwrap(), explicit);
    assert!(!plan.current(&doc));
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
    let first_spectator = molecule_at(&mut doc, &["Cl"], &[], -120.);
    let shared_spectator = molecule_at(&mut doc, &["Cl"], &[], 200.);
    let last_spectator = molecule_at(&mut doc, &["Cl"], &[], 900.);
    doc.reactions[0].reactants.push(Participant {
        atoms: first_spectator,
        coefficient: 2,
    });
    doc.reactions[0].products.push(Participant {
        atoms: shared_spectator.clone(),
        coefficient: 2,
    });
    let before = doc.reactions[1].products[0].atoms.clone();
    let after = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 3)], 720.);
    let third = step(
        &mut doc,
        &[one(&before), (shared_spectator, 2)],
        &[one(&after), (last_spectator, 2)],
        600.,
    );
    reactions::validate(&doc).unwrap();
    let arrows = [plan.arrows[0], plan.arrows[1], third];
    let updated = Plan::read(&doc, &arrows).unwrap();
    assert_eq!(updated.arrows, arrows);
    assert!(!updated.cyclic);
}

#[test]
fn spectator_cancellation_counts_coefficients_and_preserves_an_excess() {
    let drawn = |species, atom, coefficient| DrawnTerm {
        species,
        atoms: vec![atom],
        coefficient,
        label: String::new(),
    };
    let mut source = Step {
        arrow: 10,
        reactants: Vec::new(),
        products: Vec::new(),
        notes: Vec::new(),
        drawn_reactants: vec![drawn(0, 1, 1), drawn(2, 3, 2)],
        drawn_products: vec![drawn(1, 2, 1), drawn(2, 4, 1)],
    };
    let no_neighbors = PathSides::default();
    let (reactants, products) = path_sides(&source, &no_neighbors, &no_neighbors).unwrap();
    assert!(reactants.contains(&vec![3]));
    assert!(!products.contains(&vec![4]));
    source.drawn_products[1].coefficient = 2;
    let (reactants, products) = path_sides(&source, &no_neighbors, &no_neighbors).unwrap();
    assert!(!reactants.contains(&vec![3]));
    assert!(!products.contains(&vec![4]));
    source.drawn_products[0].species = 0;
    let (reactants, products) = path_sides(&source, &no_neighbors, &no_neighbors).unwrap();
    assert_eq!(reactants, BTreeSet::from([vec![1], vec![3]]));
    assert_eq!(products, BTreeSet::from([vec![2], vec![4]]));
}

#[test]
fn spectator_cancellation_keeps_the_link_in_either_copy_creation_order() {
    for shared_created_first in [true, false] {
        let mut doc = Document::default();
        let ether = molecule_at(&mut doc, &["C", "O", "C"], &[(0, 1, 1), (1, 2, 1)], 0.);
        let initial = molecule_at(&mut doc, &["C", "C", "O"], &[(0, 1, 1), (1, 2, 1)], -160.);
        let mut ethanol_at =
            |x| molecule_at(&mut doc, &["C", "C", "O"], &[(0, 1, 1), (1, 2, 1)], x);
        let (shared, other) = if shared_created_first {
            (ethanol_at(240.), ethanol_at(360.))
        } else {
            let other = ethanol_at(360.);
            (ethanol_at(240.), other)
        };
        let product = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 2)], 600.);
        let first = step(
            &mut doc,
            &[one(&ether), one(&initial)],
            &[one(&shared), one(&other)],
            120.,
        );
        let second = step(&mut doc, &[one(&shared)], &[one(&product)], 480.);
        reactions::validate(&doc).unwrap();
        for _ in 0..2 {
            let plan = Plan::read(&doc, &[second, first]).unwrap();
            assert_eq!(plan.arrows, [first, second]);
            assert!(!plan.cyclic);
            doc.reactions[0].products.reverse();
        }
    }
}

#[test]
fn a_fully_canceled_carried_molecule_can_be_the_only_link() {
    let mut doc = Document::default();
    let alcohol = molecule_at(
        &mut doc,
        &["C", "C", "C", "O"],
        &[(0, 1, 1), (1, 2, 1), (2, 3, 1)],
        0.,
    );
    let initial = molecule_at(&mut doc, &["C", "C", "O"], &[(0, 1, 1), (1, 2, 1)], -160.);
    let intermediate = molecule_at(&mut doc, &["C", "C", "C"], &[(0, 1, 1), (1, 2, 2)], 240.);
    let shared = molecule_at(&mut doc, &["C", "C", "O"], &[(0, 1, 1), (1, 2, 1)], 360.);
    let product = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 2)], 600.);
    let first = step(
        &mut doc,
        &[one(&alcohol), one(&initial)],
        &[one(&intermediate), one(&shared)],
        120.,
    );
    let second = step(&mut doc, &[one(&shared)], &[one(&product)], 480.);
    reactions::validate(&doc).unwrap();
    let plan = Plan::read(&doc, &[second, first]).unwrap();
    assert_eq!(plan.arrows, [first, second]);
    assert!(!plan.cyclic);
    assert_eq!(
        plan.notes
            .iter()
            .filter(|note| note.starts_with("Balanced by adding"))
            .count(),
        2
    );
}

#[test]
fn surviving_copy_links_take_precedence_over_fully_canceled_shortcuts() {
    let drawn = |species, atom| DrawnTerm {
        species,
        atoms: vec![atom],
        coefficient: 1,
        label: String::new(),
    };
    let row = |arrow, drawn_reactants, drawn_products| Step {
        arrow,
        drawn_reactants,
        drawn_products,
        reactants: Vec::new(),
        products: Vec::new(),
        notes: Vec::new(),
    };
    let source = row(
        1,
        vec![drawn(0, 10), drawn(2, 20), drawn(3, 30)],
        vec![drawn(1, 11), drawn(2, 21), drawn(2, 22), drawn(3, 31)],
    );
    let middle = row(2, vec![drawn(2, 21)], vec![drawn(4, 40)]);
    let last = row(
        3,
        vec![drawn(4, 40), drawn(3, 31)],
        vec![drawn(5, 50), drawn(3, 32)],
    );
    assert_eq!(
        order(&[source, middle, last]).unwrap(),
        (vec![0, 1, 2], false)
    );
}

#[test]
fn fully_canceled_links_keep_matching_instances_on_both_sides() {
    let drawn = |species, atom| DrawnTerm {
        species,
        atoms: vec![atom],
        coefficient: 1,
        label: String::new(),
    };
    let row = |arrow, drawn_reactants, drawn_products| Step {
        arrow,
        drawn_reactants,
        drawn_products,
        reactants: Vec::new(),
        products: Vec::new(),
        notes: Vec::new(),
    };
    let source = row(
        1,
        vec![drawn(0, 10), drawn(2, 20), drawn(3, 30)],
        vec![drawn(1, 11), drawn(2, 21), drawn(3, 31)],
    );
    let mut target = row(
        2,
        vec![drawn(4, 40), drawn(3, 31), drawn(2, 21)],
        vec![drawn(5, 50), drawn(3, 32), drawn(2, 22)],
    );
    for _ in 0..2 {
        assert_eq!(
            order(&[source.clone(), target.clone()]).unwrap(),
            (vec![0, 1], false)
        );
        target.drawn_reactants.reverse();
    }
}

#[test]
fn fully_canceled_tail_to_head_link_does_not_close_an_open_path() {
    let (mut doc, original) = two_step();
    let shared = molecule_at(&mut doc, &["Cl"], &[], -120.);
    let after_first = molecule_at(&mut doc, &["Cl"], &[], 200.);
    let before_second = molecule_at(&mut doc, &["Cl"], &[], 300.);
    doc.reactions[0].reactants.push(Participant {
        atoms: shared.clone(),
        coefficient: 1,
    });
    doc.reactions[0].products.push(Participant {
        atoms: after_first,
        coefficient: 1,
    });
    doc.reactions[1].reactants.push(Participant {
        atoms: before_second,
        coefficient: 1,
    });
    doc.reactions[1].products.push(Participant {
        atoms: shared,
        coefficient: 1,
    });
    reactions::validate(&doc).unwrap();
    let plan = Plan::read(&doc, &original.arrows).unwrap();
    assert_eq!(plan.arrows, original.arrows);
    assert!(!plan.cyclic);
}

#[test]
fn canceling_one_of_distinct_copies_does_not_leave_a_false_shortcut() {
    let (mut doc, plan) = two_step();
    let initial = molecule_at(&mut doc, &["Cl"], &[], -120.);
    let spectator = molecule_at(&mut doc, &["Cl"], &[], 200.);
    let extra = molecule_at(&mut doc, &["Cl"], &[], 300.);
    doc.reactions[0].reactants.push(Participant {
        atoms: initial,
        coefficient: 1,
    });
    for atoms in [spectator.clone(), extra] {
        doc.reactions[0].products.push(Participant {
            atoms,
            coefficient: 1,
        });
    }
    let before = doc.reactions[1].products[0].atoms.clone();
    let after = molecule_at(&mut doc, &["C", "C"], &[(0, 1, 3)], 720.);
    let third = step(
        &mut doc,
        &[one(&before), one(&spectator)],
        &[one(&after)],
        600.,
    );
    reactions::validate(&doc).unwrap();
    let arrows = [plan.arrows[0], plan.arrows[1], third];
    assert_eq!(Plan::read(&doc, &arrows).unwrap().arrows, arrows);
    doc.reactions[0].products.reverse();
    assert_eq!(Plan::read(&doc, &arrows).unwrap().arrows, arrows);
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
    reshiki::compound_numbers::number(&mut doc, &formaldehyde, Default::default()).unwrap();
    let plan = Plan::read(&doc, &[a, b]).unwrap();
    assert_eq!(
        plan.species.len(),
        2,
        "separate drawn copies share one calculation input"
    );
    assert_eq!(plan.steps[0].reactants[0].coefficient, 2);
    assert!(plan.steps.iter().all(Step::has_added_copies));
    assert!(
        plan.notes
            .iter()
            .any(|note| { note == "Balanced by adding another copy of compound 1 to step 1." })
    );
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
            .any(|note| note.starts_with("Balanced by adding"))
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
    let other_water = molecule_at(&mut doc, &["H", "O", "H"], &[(0, 1, 1), (1, 2, 1)], 800.);
    doc.reactions[1].products.push(Participant {
        atoms: other_water,
        coefficient: 1,
    });
    reactions::validate(&doc).unwrap();
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
            .any(|note| note.starts_with("Balanced by adding"))
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
            .any(|note| note.contains("No single molecule"))
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
        assert!(species.instances.is_empty());
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
    assert_eq!(plan.species[0].atoms, a);
    assert_eq!(plan.species[0].instances, [a, b, c]);
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
        !chemistry::difference(&plan.species, &plan.steps[0])
            .unwrap()
            .is_zero()
    );
    assert!(
        plan.steps[0]
            .notes
            .iter()
            .any(|note| note.contains("No single molecule"))
    );
    assert!(
        !plan.steps[0]
            .notes
            .iter()
            .any(|note| note.starts_with("Balanced by adding"))
    );
}

#[test]
fn whole_scheme_inventory_carries_later_reactants_and_earlier_coproducts() {
    let (_, plan) = two_step();
    let levels = plan.levels().unwrap();
    assert_eq!(
        levels.iter().map(Level::label).collect::<Vec<_>>(),
        ["C2H6O + H₂", "C2H4 + H₂O + H₂", "C2H6 + H₂O"]
    );
    for (level, formulas) in levels.iter().zip([
        vec!["C2H6O", "H2"],
        vec!["C2H4", "H2O", "H2"],
        vec!["C2H6", "H2O"],
    ]) {
        assert_eq!(
            level
                .terms()
                .map(|term| {
                    assert_eq!(term.coefficient, 1);
                    plan.species[term.species].formula.as_str()
                })
                .collect::<Vec<_>>(),
            formulas
        );
    }
}

#[test]
fn inventory_keeps_unused_copies_and_coefficient_weighted_energy() {
    let (mut doc, original) = two_step();
    let water = molecule_at(&mut doc, &["O"], &[], 700.);
    let first = &mut doc.reactions[0];
    first.reactants[0].coefficient = 2;
    first.products[0].coefficient = 2;
    first.products.push(Participant {
        atoms: water,
        coefficient: 2,
    });
    let plan = Plan::read(&doc, &original.arrows).unwrap();
    let levels = plan.levels().unwrap();
    assert_eq!(
        levels.iter().map(Level::label).collect::<Vec<_>>(),
        ["2 C2H6O + H₂", "2 C2H4 + 2 H2O + H₂", "C2H6 + C2H4 + 2 H2O"]
    );
    let energy = |index: usize| {
        Some(match plan.species[index].formula.as_str() {
            "C2H6O" => -1000.,
            "C2H4" => -905.,
            "C2H6" => -958.,
            "H2O" => -100.,
            "H2" => -50.,
            formula => panic!("unexpected formula {formula}"),
        })
    };
    let reference = sum_energy(levels[0].terms(), energy).unwrap();
    assert_eq!(
        sum_energy(levels[2].terms(), energy).unwrap() - reference,
        -13.
    );
    assert_eq!(sum_energy(levels[2].terms(), |_| None), None);
    assert_eq!(sum_energy(levels[2].terms(), |_| Some(f64::INFINITY)), None);
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
