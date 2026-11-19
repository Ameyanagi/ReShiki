use super::*;
use crate::{
    document::{Arrow, Point},
    reactions::{Participant, Reaction},
};

fn participant(id: u64) -> Participant {
    Participant {
        atoms: vec![id],
        coefficient: 1,
    }
}

fn add_reaction(doc: &mut Document, reactants: &[u64], products: &[u64]) {
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(60., 0.),
        Point::new(130., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants = reactants.iter().copied().map(participant).collect();
    reaction.products = products.iter().copied().map(participant).collect();
    doc.reactions.push(reaction);
}

fn proposal(doc: &Document, assignments: Vec<(u64, u32)>) -> Proposal {
    Proposal {
        source: snapshot(doc),
        arrow: 0,
        assignments,
        pairs: vec![],
        unmatched_reactants: 0,
        unmatched_products: 0,
        complete: true,
        multiple_best: false,
        score: Score::default(),
        states: 0,
    }
}

#[test]
fn proposal_validation_reports_the_first_invalid_assignment_in_either_order() {
    for (element, isotope) in [("O", 0), ("C", 13)] {
        let mut doc = Document::default();
        let duplicate = doc.add_atom("C", Point::new(0., 0.));
        let incompatible = doc.add_atom("C", Point::new(0., 90.));
        let same_side = doc.add_atom("C", Point::new(0., 180.));
        let opposite = doc.add_atom(element, Point::new(200., 90.));
        doc.atom_mut(same_side).unwrap().map_num = 11;
        doc.atom_mut(opposite).unwrap().map_num = 22;
        doc.atom_mut(opposite).unwrap().isotope = isotope;
        // Reaction order opposes the first assignment's error to catch a
        // validator that scans reactions before scanning assignments.
        add_reaction(&mut doc, &[incompatible], &[opposite]);
        add_reaction(&mut doc, &[duplicate, same_side], &[]);
        doc.validate().unwrap();
        let before = doc.clone();

        assert_eq!(
            proposal(&doc, vec![(duplicate, 11), (incompatible, 22)])
                .apply(&doc, true)
                .unwrap_err(),
            "Atom map 11 occurs twice on one reaction side; edit these labels first",
        );
        assert_eq!(
            proposal(&doc, vec![(incompatible, 22), (duplicate, 11)])
                .apply(&doc, true)
                .unwrap_err(),
            "Matching atom maps must have the same element and isotope",
        );
        assert_eq!(doc, before);
    }
}

#[test]
fn proposal_validation_checks_coefficient_then_uniqueness_then_cross_side_compatibility() {
    let mut doc = Document::default();
    let edited = doc.add_atom("C", Point::new(0., 0.));
    let same_side = doc.add_atom("C", Point::new(0., 90.));
    let opposite = doc.add_atom("O", Point::new(200., 0.));
    doc.atom_mut(same_side).unwrap().map_num = 11;
    doc.atom_mut(opposite).unwrap().map_num = 11;
    add_reaction(&mut doc, &[edited, same_side], &[opposite]);
    doc.reactions[0].reactants[0].coefficient = 2;
    doc.validate().unwrap();
    assert_eq!(
        proposal(&doc, vec![(edited, 11)])
            .apply(&doc, true)
            .unwrap_err(),
        "Atom maps require one drawn molecule per participant; expand coefficients in the affected reaction first",
    );

    doc.reactions[0].reactants[0].coefficient = 1;
    assert_eq!(
        proposal(&doc, vec![(edited, 11)])
            .apply(&doc, true)
            .unwrap_err(),
        "Atom map 11 occurs twice on one reaction side; edit these labels first",
    );

    doc.atom_mut(same_side).unwrap().map_num = 0;
    assert_eq!(
        proposal(&doc, vec![(edited, 11)])
            .apply(&doc, true)
            .unwrap_err(),
        "Matching atom maps must have the same element and isotope",
    );

    // Reapplying an unchanged map skips the coefficient check.
    doc.atom_mut(edited).unwrap().map_num = 11;
    doc.atom_mut(same_side).unwrap().map_num = 11;
    doc.reactions[0].reactants[0].coefficient = 2;
    assert_eq!(
        proposal(&doc, vec![(edited, 11)])
            .apply(&doc, true)
            .unwrap_err(),
        "Atom map 11 occurs twice on one reaction side; edit these labels first",
    );
}

fn atom(id: u64) -> Atom {
    Atom {
        id,
        element: "Fe".into(),
        isotope: 0,
        map: id as u32,
        charge: 0,
        h: 0,
        radical: 0,
    }
}
#[test]
fn dative_direction_is_compared_after_endpoint_mapping() {
    let source = Graph {
        atoms: vec![atom(1), atom(2)],
        edges: BTreeMap::from([(
            (0, 1),
            BondClass {
                order: 5,
                donor: Some(0),
                ring: false,
            },
        )]),
        strict: false,
    };
    let target = Graph {
        atoms: vec![atom(3), atom(4)],
        edges: BTreeMap::from([(
            (0, 1),
            BondClass {
                order: 5,
                donor: Some(1),
                ring: false,
            },
        )]),
        strict: false,
    };
    let fixed = HashMap::from([(0, 0), (1, 1)]);
    let changed = score(&source, &target, &fixed);
    assert_eq!(changed.adjacency, 1);
    assert_eq!(changed.unchanged_bonds, 0);
    assert_eq!(changed.changed_atoms, 2);
    let swapped = HashMap::from([(0, 1), (1, 0)]);
    let same = score(&source, &target, &swapped);
    assert_eq!(same.unchanged_bonds, 1);
    assert_eq!(same.changed_atoms, 0);
}
#[test]
fn augment_budget_is_checked_before_scanning_domains() {
    let domains = vec![vec![0, 1], vec![0, 1]];
    let mut work = 0;
    assert!(
        augment(
            0,
            &domains,
            &HashSet::new(),
            &mut HashSet::new(),
            &mut HashMap::new(),
            &mut work,
            Budget {
                states: 100,
                work: 0
            }
        )
        .is_err()
    );
    assert_eq!(work, 1);
}

/// A chain, or with `ring` a ring, of `n` carbons drawn from x = `x`.
fn carbons(doc: &mut Document, n: usize, ring: bool, x: f32) -> Vec<u64> {
    let ids: Vec<u64> = (0..n)
        .map(|i| doc.add_atom("C", Point::new(x + 40. * i as f32, 0.)))
        .collect();
    for pair in ids.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    if ring {
        doc.add_bond(ids[n - 1], ids[0], 1, "plain");
    }
    ids
}

#[test]
fn compare_keeps_only_bonds_of_the_same_order_and_ring_membership() {
    let mut doc = Document::default();
    let chain = carbons(&mut doc, 4, false, 0.);
    let ring = carbons(&mut doc, 4, true, 300.);
    let other = carbons(&mut doc, 4, true, 600.);
    let graph = |ids: &[u64]| Graph::compared(&doc, ids).unwrap();
    let kept = |a: &[u64], b: &[u64]| correspond(&graph(a), &graph(b), Budget::default(), |_, _| 0);
    assert_eq!(kept(&chain, &ring).kept, 0);
    let rings = kept(&ring, &other);
    assert_eq!(
        (rings.kept, rings.complete, rings.pairs.len()),
        (4, true, 4)
    );
}

#[test]
fn compare_breaks_ties_by_the_lowest_cost_not_by_id() {
    let mut doc = Document::default();
    let left = carbons(&mut doc, 6, true, 0.);
    let right = carbons(&mut doc, 6, true, 300.);
    let at = |ids: &[u64], id: u64| ids.iter().position(|x| *x == id).unwrap();
    // Every rotation and reflection keeps all six bonds; the cheapest pairs
    // each atom with the one three places on, reflected.
    let wanted = |l: u64| right[(6 + 3 - at(&left, l)) % 6];
    let found = correspond(
        &Graph::compared(&doc, &left).unwrap(),
        &Graph::compared(&doc, &right).unwrap(),
        Budget::default(),
        |l, r| u64::from(r != wanted(l)) * 100,
    );
    assert_eq!(found.kept, 6);
    assert!(found.complete);
    let expected: Vec<(u64, u64)> = left.iter().map(|l| (*l, wanted(*l))).collect();
    assert_eq!(found.pairs, expected);
    // Out of budget: unfinished, and no more work than allowed.
    let budget = Budget {
        states: 20_000,
        work: 50,
    };
    let short = correspond(
        &Graph::compared(&doc, &left).unwrap(),
        &Graph::compared(&doc, &right).unwrap(),
        budget,
        |_, _| 0,
    );
    assert!(!short.complete);
    assert!(short.work <= budget.work);
}
