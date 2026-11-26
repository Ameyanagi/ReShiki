//! Drawings exercise the same plan summaries and readiness checks as the card.
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

fn hydrogen_transfer() -> (Document, u64) {
    let mut doc = Document::default();
    let atoms = [("C", 1), ("C", 2), ("O", 3), ("C", 4)];
    let left = molecule(&mut doc, &atoms, &[(1, 4, 1), (2, 4, 1), (3, 4, 2)]);
    let right = molecule(&mut doc, &atoms, &[(1, 4, 1), (2, 4, 2), (3, 4, 1)]);
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

#[test]
fn mapped_claisen_names_map_numbers_and_starts_ready_without_hydrogen_moves() {
    let (doc, arrow) = claisen();
    let before = doc.clone();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    assert_eq!(subtitle(&doc, arrow, &Ok(plan.clone())), "GFN2-xTB · 1 → 1");
    assert_eq!(
        summary(&doc, &plan),
        Summary {
            atom_map: "Atom map: 6 of 6 heavy atoms".into(),
            bonds: "Bonds formed: C1–C6 · broken: O3–C4".into(),
            hydrogens: vec!["Hydrogens that move: none".into()],
        }
    );
    let card = State::default();
    let scope = (7, arrow);
    assert!(card.reviewed(scope, &plan));
    assert_eq!(
        card.config(&doc, scope, &plan).unwrap(),
        Config {
            images: 15,
            charge: 0,
            unpaired: 0,
        }
    );
    assert!(card.ready(&doc, scope, &plan, true));
    assert!(!card.ready(&doc, scope, &plan, false));
    assert_eq!(doc, before);
}

#[test]
fn an_unmapped_drawing_keeps_the_shape_but_cannot_start() {
    let (mut doc, arrow) = claisen();
    for atom in &mut doc.atoms {
        atom.map_num = 0;
    }
    let plan = ts_plan::plan(&doc, arrow);
    assert_eq!(
        plan,
        Err(TsPlanError::Unmapped {
            mapped: 0,
            heavy: 12,
        })
    );
    assert_eq!(subtitle(&doc, arrow, &plan), "GFN2-xTB · 1 → 1");
    let before = doc.clone();
    let mut app = app(doc, arrow);
    let _ = app.xtb_ts_card_action(Action::Find);
    assert!(app.tab.xtb.run.is_none());
    assert!(app.tab.model_job.is_none());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn moving_hydrogens_require_review_of_this_plan_and_scope() {
    let (doc, arrow) = hydrogen_transfer();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    assert_eq!(
        summary(&doc, &plan).hydrogens,
        ["Hydrogens that move:", "H from C2 to O3 (least motion)"]
    );
    let mut card = State::default();
    let scope = (7, arrow);
    assert!(!card.reviewed(scope, &plan));
    assert!(!card.ready(&doc, scope, &plan, true));
    card.retarget(scope);
    card.review = Some((plan.clone(), true));
    assert!(card.ready(&doc, scope, &plan, true));
    assert!(!card.reviewed((8, arrow), &plan));
    assert!(!card.reviewed((7, arrow + 1), &plan));
    let mut changed = plan.clone();
    changed.hydrogen_moves[0] = (plan.hydrogen_moves[0].1, plan.hydrogen_moves[0].0);
    assert!(!card.reviewed(scope, &changed));
    card.review = Some((plan.clone(), false));
    assert!(!card.ready(&doc, scope, &plan, true));

    let before = doc.clone();
    let mut app = app(doc, arrow);
    let _ = app.xtb_ts_card_action(Action::Find);
    assert!(app.tab.xtb.run.is_none());
    assert!(app.tab.model_job.is_none());
    assert_eq!(app.tab.doc, before);
}

#[test]
fn no_moving_hydrogens_can_still_be_explicitly_unchecked() {
    let (doc, arrow) = claisen();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    let scope = (7, arrow);
    let mut card = State::default();
    card.retarget(scope);
    card.review = Some((plan.clone(), false));
    assert!(!card.reviewed(scope, &plan));
    assert!(!card.ready(&doc, scope, &plan, true));
}

#[test]
fn two_to_two_is_one_diagnostic_and_cannot_start() {
    let mut doc = Document::default();
    let a = molecule(&mut doc, &[("C", 1)], &[]);
    let b = molecule(&mut doc, &[("O", 2)], &[]);
    let c = molecule(&mut doc, &[("C", 1)], &[]);
    let d = molecule(&mut doc, &[("O", 2)], &[]);
    let arrow = assign(&mut doc, &[&a, &b], &[&c, &d]);
    let plan = ts_plan::plan(&doc, arrow);
    assert_eq!(
        plan,
        Err(TsPlanError::Shape {
            reactants: 2,
            products: 2,
        })
    );
    assert_eq!(subtitle(&doc, arrow, &plan), "GFN2-xTB · 2 → 2");
    assert_eq!(
        plan.unwrap_err().to_string(),
        "Transition-state search handles 1→1, 1→2 and 2→1 reactions"
    );
    let before = doc.clone();
    let mut app = app(doc, arrow);
    let _ = app.xtb_ts_card_action(Action::Find);
    assert!(app.tab.xtb.run.is_none());
    assert!(app.tab.model_job.is_none());
    assert_eq!(app.tab.doc, before);
}

#[test]
fn settings_limit_images_and_reuse_calculate_charge_and_spin_validation() {
    let (doc, arrow) = claisen();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    let scope = (7, arrow);
    let mut card = State::default();
    card.retarget(scope);
    for images in ["10", " 15 ", "20"] {
        card.images = images.into();
        assert!(card.config(&doc, scope, &plan).is_ok());
    }
    for images in ["9", "21", "10.5", "many", ""] {
        card.images = images.into();
        assert_eq!(
            card.config(&doc, scope, &plan).unwrap_err(),
            "Images must be a whole number from 10 to 20"
        );
    }
    card.images = "15".into();
    card.charge = "−2".into();
    assert_eq!(card.config(&doc, scope, &plan).unwrap().charge, -2);
    card.unpaired = "1".into();
    assert!(
        card.config(&doc, scope, &plan)
            .unwrap_err()
            .contains("Even electron count")
    );
    card.unpaired = "-1".into();
    assert_eq!(
        card.config(&doc, scope, &plan).unwrap_err(),
        "Unpaired electrons must be a whole number from 0"
    );
    card.charge = "invalid".into();
    assert_eq!(
        card.config(&doc, scope, &plan).unwrap_err(),
        "Charge must be a whole number"
    );
    // Typed values do not follow another arrow or a file loaded into this tab.
    card.retarget((8, arrow));
    assert_eq!(card.typed((8, arrow)), ("", ""));
    assert_eq!(card.config(&doc, (8, arrow), &plan).unwrap().charge, 0);
}

#[test]
fn charge_and_unpaired_defaults_follow_the_start_molecules_drawn_state() {
    let (mut doc, arrow) = claisen();
    let plan = ts_plan::plan(&doc, arrow).unwrap();
    let first = plan.start_atoms[0];
    let atom = doc.atom_mut(first).unwrap();
    atom.charge = 2;
    atom.radical_electrons = 2;
    assert_eq!(reshiki::science::drawn_state(&doc, first), (2, 3));
    assert_eq!(drawn_state(&doc, &plan), (2, 2));
    let config = State::default().config(&doc, (7, arrow), &plan).unwrap();
    assert_eq!((config.charge, config.unpaired), (2, 2));
}

#[test]
fn closing_the_card_keeps_the_drawing_and_selection() {
    let (doc, arrow) = claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    let _focus_drawing = app.xtb_ts_card_action(Action::Open(false));
    assert!(!app.tab.xtb.ts_card.open);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, [arrow]);
    assert!(!app.tab.history.can_undo());
}

#[cfg(any(windows, target_arch = "wasm32", not(feature = "xtb")))]
#[test]
fn an_unavailable_build_cannot_start_or_edit_search_inputs() {
    let (doc, arrow) = claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    assert!(!crate::app::xtb::available());
    let reason = crate::app::xtb::unavailable().unwrap();
    assert!(
        [
            "xTB calculations are coming to Windows once code signing is in place.",
            "xTB calculations run in the desktop app.",
            "This build does not include the xTB engine.",
        ]
        .contains(&reason)
    );
    let _ = app.xtb_ts_card_action(Action::Images("20".into()));
    let _ = app.xtb_ts_card_action(Action::Find);
    assert_eq!(app.tab.xtb.ts_card.images, "15");
    assert!(app.tab.xtb.run.is_none());
    assert!(app.tab.model_job.is_none());
    assert_eq!(app.tab.doc, before);
}
