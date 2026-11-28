use super::*;

#[test]
fn generated_names_refresh_added_copy_notes_without_changing_calculation_or_snapshot() {
    let mut doc = Document::default();
    let formaldehyde = molecule_at(&mut doc, &["C", "O"], &[(0, 1, 2)], 0.);
    let dimer = molecule_at(
        &mut doc,
        &["O", "C", "C", "O"],
        &[(0, 1, 2), (1, 2, 1), (2, 3, 1)],
        240.,
    );
    let another = molecule_at(&mut doc, &["C", "O"], &[(0, 1, 2)], 550.);
    let first = step(&mut doc, &[one(&formaldehyde)], &[one(&dimer)], 120.);
    let second = step(&mut doc, &[one(&dimer)], &[(another, 2)], 450.);
    let mut plan = Plan::read(&doc, &[first, second]).unwrap();
    let snapshot = plan.snapshot.clone();
    let keys: Vec<_> = plan
        .species
        .iter()
        .map(|species| species.key.clone())
        .collect();
    assert_eq!(
        plan.notes[0],
        "Balanced by adding another copy of CH2O to step 1."
    );
    plan.resolve_names(&doc, |atoms| {
        Some(
            if atoms == dimer.as_slice() {
                "2-hydroxyethanal"
            } else {
                "methanal"
            }
            .into(),
        )
    });
    assert_eq!(plan.step_label(0), "2 methanal → 2-hydroxyethanal");
    assert_eq!(
        plan.notes[0],
        "Balanced by adding another copy of methanal to step 1."
    );
    assert_eq!(plan.steps[0].notes, plan.notes);
    assert_eq!(plan.snapshot, snapshot);
    assert_eq!(
        plan.species
            .iter()
            .map(|species| species.key.clone())
            .collect::<Vec<_>>(),
        keys
    );
    assert!(plan.current(&doc));
    assert_eq!(
        plan.review_document(0, &doc).unwrap().reactions[0]
            .reactants
            .len(),
        2
    );
}

#[test]
fn ambiguous_balance_notes_name_structural_isomers_without_changing_the_choice() {
    let mut doc = Document::default();
    let initial = molecule_at(&mut doc, &["C", "O"], &[(0, 1, 2)], 0.);
    let middle = molecule_at(
        &mut doc,
        &["C", "C", "O", "C", "O"],
        &[(0, 1, 1), (1, 2, 1), (2, 3, 1), (3, 4, 1)],
        240.,
    );
    let product = molecule_at(
        &mut doc,
        &["C", "C", "O", "C", "O"],
        &[(0, 1, 1), (1, 2, 1), (2, 3, 1), (3, 4, 1)],
        550.,
    );
    let ethanol = molecule_at(&mut doc, &["C", "C", "O"], &[(0, 1, 1), (1, 2, 1)], -200.);
    let ether = molecule_at(&mut doc, &["C", "O", "C"], &[(0, 1, 1), (1, 2, 1)], -400.);
    let first = step(&mut doc, &[one(&initial)], &[one(&middle)], 120.);
    let second = step(
        &mut doc,
        &[one(&middle), one(&ethanol), one(&ether)],
        &[one(&product), one(&ethanol), one(&ether)],
        450.,
    );
    let mut plan = Plan::read(&doc, &[first, second]).unwrap();
    let before = plan.steps[0].reactants.clone();
    plan.resolve_names(&doc, |atoms| {
        if atoms == ethanol.as_slice() {
            Some("ethanol".into())
        } else if atoms == ether.as_slice() {
            Some("methoxymethane".into())
        } else {
            None
        }
    });
    let note = &plan.steps[0].notes[0];
    assert!(note.contains("ethanol"), "{note}");
    assert!(note.contains("methoxymethane"), "{note}");
    assert!(!note.contains("C2H6O"), "{note}");
    assert!(note.ends_with("continuing as drawn."), "{note}");
    assert_eq!(plan.steps[0].reactants, before);
    assert!(plan.notes.contains(note));
    assert!(plan.current(&doc));
}
