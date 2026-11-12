use super::{tests::en3, *};
use crate::{
    document::Point,
    generic::{Alternative, AlternativeSite},
    joining::{JoinPlan, join_plan},
};

#[test]
fn coordination_plan_en_ignores_carbons_and_non_atom_ids() {
    let (doc, co, donors) = en3();
    let mut selected: Vec<_> = doc.atoms[..5].iter().map(|atom| atom.id).collect();
    selected.push(u64::MAX);
    let before = doc.clone();
    let plan = coordination_plan(&doc, &selected).unwrap();
    assert_eq!(plan.pairs, vec![(donors[0], co), (donors[1], co)]);
    assert!(plan.skipped.is_empty());
    assert_eq!(join_plan(&doc, &selected), Some(JoinPlan::Coordinate(plan)));
    assert_eq!(doc, before);
}

#[test]
fn coordination_plan_en3_orders_all_six_donors() {
    let (doc, co, donors) = en3();
    assert_eq!(doc.atoms.len(), 13);
    let selected: Vec<_> = doc.atoms.iter().rev().map(|atom| atom.id).collect();
    let plan = coordination_plan(&doc, &selected).unwrap();
    assert_eq!(
        plan.pairs,
        donors.iter().map(|&id| (id, co)).collect::<Vec<_>>()
    );
    assert!(plan.skipped.is_empty());
}

#[test]
fn coordination_plan_reach_includes_boundary_and_keeps_skipped_metal() {
    let mut doc = Document::default();
    let co = doc.add_atom("Co", Point::default());
    let reach = 2. * doc.drawing_style.bond_length_world;
    let boundary = doc.add_atom("N", Point::new(0., reach));
    let far = doc.add_atom("O", Point::new(0., reach + 1.));
    let plan = coordination_plan(&doc, &[far, co, boundary]).unwrap();
    assert_eq!(plan.pairs, vec![(boundary, co)]);
    assert_eq!(plan.skipped, vec![(far, co, Skip::TooFar)]);
}

#[test]
fn coordination_plan_chooses_nearest_metal_and_breaks_ties_by_id() {
    let mut doc = Document::default();
    let co = doc.add_atom("Co", Point::default());
    let ni = doc.add_atom("Ni", Point::new(100., 0.));
    let right = doc.add_atom("N", Point::new(90., 0.));
    let tie = doc.add_atom("O", Point::new(50., 30.));
    let left = doc.add_atom("S", Point::new(10., 0.));
    let plan = coordination_plan(&doc, &[ni, right, left, tie, co]).unwrap();
    assert_eq!(plan.pairs, vec![(tie, co), (left, co), (right, ni)]);
    assert!(plan.skipped.is_empty());
}

#[test]
fn coordination_plan_without_metals_preserves_join_routes() {
    let mut doc = Document::default();
    let ids: Vec<_> = (0..4)
        .map(|i| doc.add_atom("C", Point::new(i as f32 * 42., 0.)))
        .collect();
    for count in 0..=4 {
        assert_eq!(coordination_plan(&doc, &ids[..count]), None);
    }
    assert_eq!(join_plan(&doc, &[]), None);
    assert_eq!(join_plan(&doc, &ids[..1]), None);
    assert_eq!(
        join_plan(&doc, &ids[..2]),
        Some(JoinPlan::ShareAtom {
            source: ids[0],
            target: ids[1],
        })
    );
    assert_eq!(join_plan(&doc, &ids[..3]), Some(JoinPlan::Merge));
    assert_eq!(join_plan(&doc, &ids), Some(JoinPlan::Merge));
    doc.add_bond(ids[2], ids[3], 1, "plain");
    doc.add_bond(ids[0], ids[1], 1, "plain");
    assert_eq!(
        join_plan(&doc, &ids),
        Some(JoinPlan::FuseBond {
            source: (ids[2], ids[3]),
            target: (ids[0], ids[1]),
        })
    );
    doc.add_bond(ids[1], ids[2], 1, "plain");
    assert_eq!(join_plan(&doc, &ids), Some(JoinPlan::Merge));
}

#[test]
fn coordination_plan_with_metals_but_no_donors_is_empty() {
    let mut doc = Document::default();
    let co = doc.add_atom("Co", Point::default());
    let a = doc.add_atom("C", Point::new(20., 0.));
    let b = doc.add_atom("C", Point::new(40., 0.));
    let ni = doc.add_atom("Ni", Point::new(60., 0.));
    let fe = doc.add_atom("Fe", Point::new(80., 0.));
    for selected in [[co, a, b], [co, ni, fe]] {
        let plan = coordination_plan(&doc, &selected).unwrap();
        assert!(plan.pairs.is_empty());
        assert!(plan.skipped.is_empty());
        assert_eq!(join_plan(&doc, &selected), Some(JoinPlan::Coordinate(plan)));
    }
    assert_eq!(coordination_plan(&doc, &[co]), None);
}

#[test]
fn coordination_plan_explicit_pair_ignores_reach_and_defers_donor_validation() {
    let mut doc = Document::default();
    let co = doc.add_atom("Co", Point::default());
    let donor = doc.add_atom("N", Point::new(1000., 0.));
    for selected in [[co, donor], [donor, co]] {
        let plan = coordination_plan(&doc, &selected).unwrap();
        assert_eq!(plan.pairs, vec![(donor, co)]);
        assert!(plan.skipped.is_empty());
        assert_eq!(
            coordinate_many(&doc, &plan).unwrap().added,
            vec![(donor, co)]
        );
    }
    doc.atom_mut(donor).unwrap().element = "C".into();
    let plan = coordination_plan(&doc, &[co, donor]).unwrap();
    let message = coordinate_atoms(&doc, donor, co).unwrap_err();
    let error = coordinate_many(&doc, &plan).unwrap_err();
    assert_eq!(error.message, message);
    assert_eq!(error.skipped, vec![(donor, co, Skip::Rejected(message))]);
}

#[test]
fn coordination_plan_metal_pair_uses_share_atom() {
    for element in ["Co", "Ni"] {
        let mut doc = Document::default();
        let source = doc.add_atom("Co", Point::default());
        let target = doc.add_atom(element, Point::new(100., 0.));
        assert_eq!(coordination_plan(&doc, &[source, target]), None);
        assert_eq!(
            join_plan(&doc, &[source, target]),
            Some(JoinPlan::ShareAtom { source, target })
        );
    }
}

#[test]
fn coordinate_join_generic_admission_checks_every_paired_endpoint() {
    let mut doc = Document::default();
    let co = doc.add_atom("Co", Point::default());
    let ni = doc.add_atom("Ni", Point::new(100., 0.));
    let first = doc.add_atom("N", Point::new(10., 0.));
    let second = doc.add_atom("N", Point::new(90., 0.));
    let selected = [co, ni, first, second];
    let plan = join_plan(&doc, &selected).unwrap();
    assert!(!plan.generic_blocked(&doc, &selected));
    for atom in selected {
        let mut generic = doc.clone();
        generic.generic.alternatives.push(AlternativeSite {
            atom,
            choices: vec![Alternative::Unresolved { label: "R1".into() }],
            excluded: false,
        });
        assert!(plan.generic_blocked(&generic, &selected), "endpoint {atom}");
    }
    for element in ["*", "N"] {
        let atom = doc.add_atom(element, Point::new(1000., 0.));
        doc.generic.alternatives.push(AlternativeSite {
            atom,
            choices: vec![Alternative::Unresolved { label: "R1".into() }],
            excluded: false,
        });
    }
    let selected: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
    let plan = join_plan(&doc, &selected).unwrap();
    assert!(!plan.generic_blocked(&doc, &selected));
}

#[test]
fn coordinate_many_en3_matches_sequential_contacts_and_preserves_atoms_and_skeleton() {
    let (doc, co, donors) = en3();
    let before = doc.clone();
    let selected: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
    let plan = coordination_plan(&doc, &selected).unwrap();
    let mut sequential = doc.clone();
    for &donor in &donors {
        sequential = coordinate_atoms(&sequential, donor, co)
            .unwrap()
            .into_document(&sequential);
    }
    let outcome = coordinate_many(&doc, &plan).unwrap();
    assert_eq!(outcome.document, sequential);
    assert_eq!(outcome.document.atoms, before.atoms);
    assert_eq!(&outcome.document.bonds[..9], before.bonds.as_slice());
    assert_eq!(outcome.document.bonds.len(), 15);
    assert_eq!(
        outcome.document.bonds[9..]
            .iter()
            .map(|bond| (bond.a, bond.b, bond.order))
            .collect::<Vec<_>>(),
        donors.iter().map(|&id| (id, co, 5)).collect::<Vec<_>>()
    );
    assert_eq!(outcome.added, plan.pairs);
    assert!(outcome.present.is_empty());
    assert!(outcome.skipped.is_empty());
    assert_eq!(doc, before);
    outcome.document.validate().unwrap();
}

#[test]
fn coordinate_many_skips_a_conflicting_bond_and_continues() {
    let (mut doc, co, donors) = en3();
    doc.add_bond(donors[0], co, 1, "plain");
    let before = doc.clone();
    let message = coordinate_atoms(&doc, donors[0], co).unwrap_err();
    assert!(message.contains("already have a different bond"));
    let selected: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
    let plan = coordination_plan(&doc, &selected).unwrap();
    let outcome = coordinate_many(&doc, &plan).unwrap();
    assert_eq!(outcome.added, plan.pairs[1..]);
    assert_eq!(
        outcome.skipped,
        vec![(donors[0], co, Skip::Rejected(message))]
    );
    assert!(outcome.present.is_empty());
    assert_eq!(outcome.document.atoms, before.atoms);
    assert_eq!(&outcome.document.bonds[..10], before.bonds.as_slice());
    assert_eq!(doc, before);
}

#[test]
fn coordinate_many_all_present_returns_an_unchanged_document() {
    let (doc, _, _) = en3();
    let selected: Vec<_> = doc.atoms.iter().map(|atom| atom.id).collect();
    let plan = coordination_plan(&doc, &selected).unwrap();
    let connected = coordinate_many(&doc, &plan).unwrap().document;
    let outcome = coordinate_many(&connected, &plan).unwrap();
    assert_eq!(outcome.document, connected);
    assert_eq!(outcome.present, plan.pairs);
    assert!(outcome.added.is_empty());
    assert!(outcome.skipped.is_empty());
}

#[test]
fn coordinate_many_nothing_admissible_returns_reasons_and_ids() {
    let (mut doc, co, donors) = en3();
    doc.add_bond(donors[0], co, 1, "plain");
    let message = coordinate_atoms(&doc, donors[0], co).unwrap_err();
    let plan = coordination_plan(&doc, &[donors[0], co]).unwrap();
    let error = coordinate_many(&doc, &plan).unwrap_err();
    assert_eq!(error.message, message);
    assert_eq!(
        error.skipped,
        vec![(donors[0], co, Skip::Rejected(message))]
    );

    let far = doc.add_atom("N", Point::new(1000., 0.));
    let carbon = doc
        .atoms
        .iter()
        .find(|atom| atom.element == "C")
        .unwrap()
        .id;
    let plan = coordination_plan(&doc, &[co, far, carbon]).unwrap();
    let error = coordinate_many(&doc, &plan).unwrap_err();
    assert_eq!(error.skipped, vec![(far, co, Skip::TooFar)]);

    let plan = coordination_plan(&doc, &[co, carbon, u64::MAX]).unwrap();
    let error = coordinate_many(&doc, &plan).unwrap_err();
    assert!(!error.message.is_empty());
    assert!(error.skipped.is_empty());
}

#[test]
fn coordinate_many_planned_skips_survive_success_and_prevent_present_only_success() {
    let (mut doc, co, donors) = en3();
    let far = doc.add_atom("O", Point::new(1000., 0.));
    let plan = coordination_plan(&doc, &[far, co, donors[0]]).unwrap();
    let outcome = coordinate_many(&doc, &plan).unwrap();
    assert_eq!(outcome.added, vec![(donors[0], co)]);
    assert_eq!(outcome.skipped, vec![(far, co, Skip::TooFar)]);
    let error = coordinate_many(&outcome.document, &plan).unwrap_err();
    assert_eq!(error.skipped, outcome.skipped);
}
