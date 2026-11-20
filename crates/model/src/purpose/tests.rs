use super::*;
use crate::document::{Annotation, Arrow, Point};
use crate::reactions::{self, Role};

fn ring(doc: &mut Document, x: f32, y: f32) -> Vec<u64> {
    let atoms: Vec<_> = (0..6)
        .map(|i| {
            let angle = std::f32::consts::FRAC_PI_3 * i as f32;
            doc.add_atom(
                "C",
                Point::new(x + 15. * angle.cos(), y + 15. * angle.sin()),
            )
        })
        .collect();
    for i in 0..6 {
        doc.add_bond(atoms[i], atoms[(i + 1) % 6], 1, "plain");
    }
    atoms
}

fn arrow(doc: &mut Document, from: f32, to: f32) -> u64 {
    let id = doc.next_id();
    doc.arrows.push(Arrow::new(
        id,
        Point::new(from, 0.),
        Point::new(to, 0.),
        Default::default(),
        Default::default(),
    ));
    id
}

/// Reactant, arrow, product, and a reference molecule beyond the product.
fn scheme() -> (Document, Vec<u64>, Vec<u64>, Vec<u64>, u64) {
    let mut doc = Document::default();
    let reactant = ring(&mut doc, -100., 0.);
    let arrow = arrow(&mut doc, -50., 50.);
    let product = ring(&mut doc, 100., 0.);
    let reference = ring(&mut doc, 180., 0.);
    (doc, reactant, product, reference, arrow)
}

fn edit(doc: &mut Document, change: impl FnOnce(&mut Document)) {
    let before = doc.clone();
    change(doc);
    let _ = crate::transaction::reconcile(doc, before).unwrap();
}

#[test]
fn a_reference_beside_the_product_never_joins_the_copied_reaction() {
    let (mut doc, reactant, product, reference, _) = scheme();
    let all = crate::editing::selection(&doc, &doc.all_ids());
    let inferred = reactions::copy_reaction(&doc, &all).unwrap().unwrap();
    assert_eq!(
        inferred.products.len(),
        2,
        "without purpose it is a product"
    );

    edit(&mut doc, |doc| {
        assert!(set(doc, &reference[..1], Purpose::Reference));
    });
    assert!(
        doc.atoms
            .iter()
            .all(|a| { (a.purpose == Purpose::Reference) == reference.contains(&a.id) })
    );
    let all = crate::editing::selection(&doc, &doc.all_ids());
    let inferred = reactions::copy_reaction(&doc, &all).unwrap().unwrap();
    assert_eq!(inferred.reactants[0].atoms, sorted(&reactant));
    assert_eq!(inferred.products.len(), 1);
    assert_eq!(inferred.products[0].atoms, sorted(&product));
}

#[test]
fn preview_reads_the_scheme_row_without_reference_or_illustration() {
    let (mut doc, reactant, product, reference, arrow) = scheme();
    let reagent = ring(&mut doc, 0., -40.);
    edit(&mut doc, |doc| {
        set(doc, &reference, Purpose::Reference);
    });
    let reaction = reactions::preview(&doc, arrow).unwrap();
    assert_eq!(reaction.reactants[0].atoms, sorted(&reactant));
    assert_eq!(reaction.products.len(), 1);
    assert_eq!(reaction.products[0].atoms, sorted(&product));
    assert_eq!(reaction.agents[0].atoms, sorted(&reagent));

    edit(&mut doc, |doc| {
        set(doc, &[arrow], Purpose::Illustration);
    });
    assert!(reactions::preview(&doc, arrow).is_err());
}

#[test]
fn an_illustration_arrow_beside_a_scheme_does_not_block_copy() {
    let (mut doc, reactant, product, reference, _) = scheme();
    let decoration = arrow(&mut doc, 150., 210.);
    edit(&mut doc, |doc| {
        set(doc, &reference, Purpose::Illustration);
        set(doc, &[decoration], Purpose::Illustration);
    });
    let all = crate::editing::selection(&doc, &doc.all_ids());
    let inferred = reactions::copy_reaction(&doc, &all).unwrap().unwrap();
    assert_eq!(inferred.reactants[0].atoms, sorted(&reactant));
    assert_eq!(inferred.products[0].atoms, sorted(&product));
}

#[test]
fn a_new_purpose_removes_reaction_roles_and_assigning_restores_chemistry() {
    let (mut doc, reactant, product, _, arrow) = scheme();
    reactions::assign(&mut doc, arrow, &reactant, Role::Reactant).unwrap();
    reactions::assign(&mut doc, arrow, &product, Role::Product).unwrap();
    edit(&mut doc, |doc| {
        set(doc, &product, Purpose::Reference);
    });
    assert!(doc.reactions[0].products.is_empty());

    reactions::assign(&mut doc, arrow, &product[..1], Role::Product).unwrap();
    assert_eq!(doc.reactions[0].products[0].atoms, sorted(&product));
    assert!(
        product
            .iter()
            .all(|id| doc.atom(*id).unwrap().purpose.is_chemistry())
    );

    edit(&mut doc, |doc| {
        set(doc, &[arrow], Purpose::Illustration);
    });
    assert!(doc.reactions.is_empty());
}

#[test]
fn molecules_keep_one_purpose_through_edits() {
    let (mut doc, reactant, product, reference, _) = scheme();
    edit(&mut doc, |doc| {
        set(doc, &reference, Purpose::Reference);
    });
    // A new atom drawn on a reference molecule joins it as Reference.
    let mut grown = 0;
    edit(&mut doc, |doc| {
        grown = doc.add_atom("O", Point::new(220., 0.));
        doc.add_bond(reference[0], grown, 1, "plain");
    });
    assert_eq!(doc.atom(grown).unwrap().purpose, Purpose::Reference);
    // Joining a Chemistry and a Reference molecule keeps Chemistry.
    edit(&mut doc, |doc| {
        doc.add_bond(product[0], reference[3], 1, "plain");
    });
    assert!(doc.atoms.iter().all(|a| a.purpose.is_chemistry()));
    // Moving or recolouring never changes purpose.
    edit(&mut doc, |doc| {
        set(doc, &reactant, Purpose::Illustration);
    });
    edit(&mut doc, |doc| {
        for atom in doc.atoms.iter_mut().filter(|a| reactant.contains(&a.id)) {
            atom.position = atom.position.offset(0., 500.);
        }
    });
    assert!(
        reactant
            .iter()
            .all(|id| doc.atom(*id).unwrap().purpose == Purpose::Illustration)
    );
}

#[test]
fn illustration_arrows_take_no_condition_text_and_release_theirs() {
    let (mut doc, _, _, _, arrow) = scheme();
    let mut text = 0;
    edit(&mut doc, |doc| {
        text = doc.next_id();
        doc.annotations.push(Annotation {
            angle: 0.,
            id: text,
            position: Point::new(-10., -30.),
            text: "MeOH".into(),
            format: Default::default(),
        });
    });
    assert_eq!(doc.arrow_labels.len(), 1);
    edit(&mut doc, |doc| {
        set(doc, &[arrow], Purpose::Illustration);
    });
    assert!(doc.arrow_labels.is_empty());
    assert!(doc.annotations.iter().any(|a| a.id == text));
    // Dropping text on it again does not attach it.
    edit(&mut doc, |doc| {
        doc.annotations[0].position = Point::new(-12., -28.);
    });
    assert!(doc.arrow_labels.is_empty());
}

#[test]
fn purpose_saves_as_version_40_and_old_files_stay_chemistry() {
    let (mut doc, _, _, reference, _) = scheme();
    assert!(doc.file_version() < crate::document::DRAWING_ELEMENTS);
    let plain = serde_json::to_string(&doc).unwrap();
    assert!(!plain.contains("purpose"));

    set(&mut doc, &reference, Purpose::Reference);
    assert_eq!(doc.file_version(), crate::document::DRAWING_ELEMENTS);
    let saved = serde_json::to_string(&doc).unwrap();
    assert!(saved.contains("\"purpose\":\"reference\""));
    let reopened: Document = serde_json::from_str(&saved).unwrap();
    assert_eq!(reopened, doc);
}

#[test]
fn only_drops_rejected_molecules_and_their_captions() {
    let (mut doc, _, product, reference, arrow) = scheme();
    set(&mut doc, &reference, Purpose::Illustration);
    let chemical = only(&doc, Purpose::is_chemical);
    assert!(chemical.atoms.iter().all(|a| !reference.contains(&a.id)));
    assert!(chemical.atoms.iter().any(|a| a.id == product[0]));
    assert!(chemical.arrows.iter().any(|a| a.id == arrow));
    let untouched = only(&doc, |_| true);
    assert!(matches!(untouched, Cow::Borrowed(_)));
}

fn sorted(ids: &[u64]) -> Vec<u64> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids
}

#[test]
fn a_reference_across_the_arrow_or_an_illustration_arrow_in_the_row_changes_nothing() {
    let (mut doc, reactant, product, beyond, target) = scheme();
    // A reference straddling the arrow midpoint would block inference.
    let across = ring(&mut doc, 0., 10.);
    // A decorative arrow between the reaction arrow and the product would
    // otherwise end the row before the product.
    let between = arrow(&mut doc, 60., 75.);
    edit(&mut doc, |doc| {
        set(doc, &across, Purpose::Reference);
        set(doc, &beyond, Purpose::Reference);
        set(doc, &[between], Purpose::Illustration);
    });
    let reaction = reactions::preview(&doc, target).unwrap();
    assert_eq!(reaction.reactants[0].atoms, sorted(&reactant));
    assert_eq!(reaction.products.len(), 1);
    assert_eq!(reaction.products[0].atoms, sorted(&product));
    let all = crate::editing::selection(&doc, &doc.all_ids());
    let inferred = reactions::copy_reaction(&doc, &all).unwrap().unwrap();
    assert_eq!(inferred.products.len(), 1);
}

#[test]
fn saved_files_keep_purpose_and_reopening_repairs_broken_ones() {
    let (mut doc, reactant, product, reference, arrow) = scheme();
    reactions::assign(&mut doc, arrow, &reactant, Role::Reactant).unwrap();
    reactions::assign(&mut doc, arrow, &product, Role::Product).unwrap();
    set(&mut doc, &reference, Purpose::Reference);
    let bytes = doc.file_json().unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("\"version\": 40") || text.contains("\"version\":40"));
    let reopened = Document::from_json(&bytes).unwrap();
    assert!(
        reference
            .iter()
            .all(|id| reopened.atom(*id).unwrap().purpose == Purpose::Reference)
    );

    // A file whose product was edited to Reference by hand loses the role,
    // and a molecule only partly Reference becomes Chemistry again.
    let mut broken: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for atom in broken["atoms"].as_array_mut().unwrap() {
        let id = atom["id"].as_u64().unwrap();
        if product.contains(&id) {
            atom["purpose"] = "reference".into();
        }
        if id == reference[0] {
            atom.as_object_mut().unwrap().remove("purpose");
        }
    }
    let repaired = Document::from_json(&serde_json::to_vec(&broken).unwrap()).unwrap();
    assert!(repaired.reactions[0].products.is_empty());
    assert!(
        reference
            .iter()
            .all(|id| repaired.atom(*id).unwrap().purpose.is_chemistry())
    );
}

#[test]
fn preview_of_assigned_roles_shows_their_conditions() {
    let (mut doc, reactant, product, _, arrow) = scheme();
    edit(&mut doc, |doc| {
        let id = doc.next_id();
        doc.annotations.push(Annotation {
            angle: 0.,
            id,
            position: Point::new(-10., -30.),
            text: "MeOH".into(),
            format: Default::default(),
        });
    });
    reactions::assign(&mut doc, arrow, &reactant, Role::Reactant).unwrap();
    reactions::assign(&mut doc, arrow, &product, Role::Product).unwrap();
    let reaction = reactions::preview(&doc, arrow).unwrap();
    assert_eq!(reaction.annotations, vec![doc.arrow_labels[0].annotation]);
}

#[test]
fn arrows_without_reaction_roles_have_no_preview() {
    let (mut doc, ..) = scheme();
    let retro = doc.arrows[0].id;
    doc.arrows[0].kind = "retro".into();
    assert!(reactions::preview(&doc, retro).is_err());
}
