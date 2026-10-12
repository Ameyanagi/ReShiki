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
            },
        )]),
    };
    let target = Graph {
        atoms: vec![atom(3), atom(4)],
        edges: BTreeMap::from([(
            (0, 1),
            BondClass {
                order: 5,
                donor: Some(1),
            },
        )]),
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
