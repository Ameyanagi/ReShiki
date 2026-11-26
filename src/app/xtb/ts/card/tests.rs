//! Reaction drawings and regression coverage for Predict’s reaction card.
use super::*;
use reshiki::document::{Arrow, Point};
use reshiki::reactions::{Participant, Reaction};
use std::collections::BTreeMap;

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
            .map(|molecule| Participant {
                atoms: molecule.values().copied().collect(),
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

pub(in crate::app::xtb::ts) fn claisen() -> (Document, u64) {
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
    (doc, arrow)
}

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
pub(in crate::app::xtb::ts) fn hydrogen_transfer() -> (Document, u64) {
    let mut doc = Document::default();
    let atoms = [("C", 1), ("C", 2), ("O", 3), ("C", 4), ("H", 5)];
    let left = molecule(
        &mut doc,
        &atoms,
        &[(1, 4, 1), (2, 4, 1), (3, 4, 2), (2, 5, 1)],
    );
    let right = molecule(
        &mut doc,
        &atoms,
        &[(1, 4, 1), (2, 4, 2), (3, 4, 1), (2, 5, 1)],
    );
    let arrow = assign(&mut doc, &[&left], &[&right]);
    (doc, arrow)
}

fn app(doc: Document, arrow: u64) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc;
    app.tab.selected = vec![arrow];
    app
}

fn pairs(bonds: &[[MappedAtom; 2]]) -> Vec<String> {
    bonds
        .iter()
        .map(|[a, b]| format!("{}{}–{}{}", a.element, a.number, b.element, b.number))
        .collect()
}

#[test]
fn reaction_label_uses_names_then_formulas() {
    let (mut doc, arrow) = claisen();
    let atoms = doc.reactions[0].reactants[0].atoms.clone();
    let identity = reshiki::naming::selected_identity(&doc, &atoms)
        .unwrap()
        .smiles;
    reshiki::molecule_names::show(
        &mut doc,
        &atoms,
        "allyl vinyl ether".into(),
        identity,
        Default::default(),
        false,
    )
    .unwrap();
    let app = app(doc, arrow);
    let label = app.xtb_reaction_label(arrow);
    assert_eq!(label.title(), "allyl vinyl ether → C5H8O");
    assert_eq!(label.first_reactant(), Some("allyl vinyl ether"));
}

#[test]
fn the_run_captures_the_drawn_map_numbers() {
    let (mut doc, arrow) = claisen();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    let bonds = BondChanges::from_plan(&doc, &plan);
    assert_eq!(pairs(&bonds.0.formed), ["C1–C6"]);
    assert_eq!(pairs(&bonds.0.broken), ["O3–C4"]);

    doc.atom_mut(plan.form[0].0).unwrap().map_num = 99;
    assert_eq!(
        pairs(&bonds.0.formed),
        ["C1–C6"],
        "edits must not change the run's map"
    );
}

#[test]
fn diels_alder_lists_bonds_in_arrow_direction_when_search_starts_on_product() {
    let mut doc = Document::default();
    let diene = molecule(
        &mut doc,
        &[("C", 1), ("C", 2), ("C", 3), ("C", 4)],
        &[(1, 2, 2), (2, 3, 1), (3, 4, 2)],
    );
    let ethylene = molecule(&mut doc, &[("C", 5), ("C", 6)], &[(5, 6, 2)]);
    let ring = molecule(
        &mut doc,
        &(1..=6).map(|number| ("C", number)).collect::<Vec<_>>(),
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
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    assert_eq!(plan.start, Side::Product);
    let bonds = BondChanges::from_plan(&doc, &plan);
    assert_eq!(pairs(&bonds.0.formed), ["C1–C6", "C4–C5"]);
    assert!(bonds.0.broken.is_empty());
}

#[test]
fn prediction_reaction_selection_requires_one_chemistry_arrow() {
    let (doc, arrow) = claisen();
    let atom = doc.atoms[0].id;
    let mut app = app(doc, arrow);
    assert_eq!(app.xtb_selected_reaction(), Some(arrow));
    app.tab.selected = vec![atom];
    assert_eq!(app.xtb_selected_reaction(), None);
    app.tab.selected = vec![arrow, atom];
    assert_eq!(app.xtb_selected_reaction(), None);
    app.tab.selected = vec![arrow];
    app.tab.doc.arrows[0].kind = "resonance".into();
    assert_eq!(app.xtb_selected_reaction(), None);
}
