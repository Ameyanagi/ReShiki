//! Scheme review through the same messages as the row, menu and palette.
use super::*;
use crate::app::xtb::ts::card::tests::claisen;
use reshiki::document::{Arrow, Point};
use reshiki::reactions::{Participant, Reaction};

fn app(doc: Document, arrow: u64) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = doc;
    app.tab.selected = vec![arrow];
    app.tab.busy = false;
    app
}

fn open(app: &mut App, mode: Mode) {
    let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Review(mode, None))));
}

#[test]
fn complete_balanced_map_runs_in_one_click_when_xtb_is_available() {
    let (doc, arrow) = claisen();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.ts_review_open());
    assert_eq!(app.xtb_running(), available());
    assert_eq!(app.tab.model_job.is_some(), available());
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
fn drawn_claisen() -> (Document, u64) {
    let (mut doc, arrow) = claisen();
    let reaction = doc.reactions.pop().unwrap();
    doc.arrows[0].start = Point::new(110., 0.);
    doc.arrows[0].end = Point::new(250., 0.);
    for (participants, start) in [(&reaction.reactants, -80.), (&reaction.products, 310.)] {
        for (index, id) in participants[0].atoms.iter().enumerate() {
            let atom = doc.atom_mut(*id).unwrap();
            atom.position = Point::new(start + 25. * index as f32, 0.);
            if atom.map_num == 6 {
                atom.map_num = 0;
            }
        }
    }
    doc.add_atom("O", Point::new(180., -32.));
    (doc, arrow)
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
fn finish_mapping(app: &mut App) {
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(matches!(dialog.mapping, Mapping::Finding));
    let source = with_roles(&app.tab.doc, dialog.arrow).unwrap();
    let proposal =
        reshiki::reaction_mapping::propose(&source, dialog.arrow, Default::default()).unwrap();
    let _ = app.update(send(Action::Ready(dialog.id, Ok(Arc::new(proposal)))));
    assert!(matches!(
        app.tab.xtb.ts_review.as_ref().unwrap().mapping,
        Mapping::Proposed(_)
    ));
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn inferred_roles_and_auto_map_wait_for_accept_then_commit_together() {
    let (doc, arrow) = drawn_claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.xtb_running());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.inferred);
    assert_eq!(dialog.inputs.len(), 2);
    assert_eq!(
        dialog.ignored.as_deref(),
        Some("Ignored: reagents and conditions · H2O")
    );
    finish_mapping(&mut app);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.validate().is_ok(), "{:?}", dialog.validate());
    assert_eq!(
        dialog.changes.as_ref().unwrap().bonds,
        "Bonds formed: C1–C6 · broken: O3–C4"
    );
    let _ = app.update(send(Action::Accept));
    assert!(!app.ts_review_open());
    assert!(app.xtb_running());
    assert_eq!(app.tab.doc.reactions.len(), 1);
    assert_eq!(app.tab.doc.reactions[0].agents.len(), 1);
    assert!(
        app.tab.doc.reactions[0].reactants[0]
            .atoms
            .iter()
            .all(|id| app.tab.doc.atom(*id).unwrap().map_num != 0)
    );
    assert!(app.tab.history.can_undo());
    let _ = app.update(Message::Xtb(XtbAction::Cancel));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.reactions, before.reactions);
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .zip(before.atoms)
            .all(|(after, before)| after.map_num == before.map_num)
    );
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn edit_map_hands_off_the_inferred_proposal_without_editing_the_drawing() {
    let (doc, arrow) = drawn_claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    finish_mapping(&mut app);
    let _ = app.update(send(Action::EditMap));
    assert!(!app.ts_review_open());
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.mapping.proposal().is_some());
    let _ = app.update(Message::Mapping(
        crate::app::reaction_mapping::Action::ApplyProposal,
    ));
    assert_eq!(app.tab.doc.reactions.len(), 1);
    assert!(app.tab.history.can_undo());
    assert!(!app.xtb_running());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn complete_map_with_moving_hydrogen_requires_the_sheet() {
    let (doc, arrow) = crate::app::xtb::ts::card::tests::hydrogen_transfer();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.xtb_running());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(matches!(dialog.mapping, Mapping::Drawing));
    assert_eq!(
        dialog.changes.as_ref().unwrap().hydrogens,
        ["H from C2 to O3 (least motion)"]
    );
    assert!(dialog.validate().is_ok());
    let _ = app.update(send(Action::Cancel));
    assert!(!app.ts_review_open());
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn reviewed_charges_set_the_run_state_and_incompatible_multiplicities_block_it() {
    let (doc, arrow) = crate::app::xtb::ts::card::tests::hydrogen_transfer();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    let _ = app.update(send(Action::Charge(0, "−2".into())));
    assert!(
        app.tab
            .xtb
            .ts_review
            .as_ref()
            .unwrap()
            .validate()
            .unwrap_err()
            .contains("charge +2")
    );
    let _ = app.update(send(Action::Charge(1, "-2".into())));
    let _ = app.update(send(Action::Multiplicity(1, "3".into())));
    assert!(
        app.tab
            .xtb
            .ts_review
            .as_ref()
            .unwrap()
            .validate()
            .unwrap_err()
            .contains("cannot share one spin state")
    );
    let _ = app.update(send(Action::Multiplicity(1, "1".into())));
    let _ = app.update(send(Action::Accept));
    assert_eq!(app.tab.xtb.run.as_ref().unwrap().config.charge, -2);
    assert!(app.tab.doc.atoms.iter().all(|atom| atom.charge == 0));
}

fn ions(extra_chloride: bool) -> (Document, u64) {
    let mut doc = Document::default();
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(50., 0.),
        Point::new(150., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    for (product, atoms) in [
        (false, &mut reaction.reactants),
        (true, &mut reaction.products),
    ] {
        let x = if product { 185. } else { 0. };
        let na = doc.add_atom("Na", Point::new(x, 0.));
        let sodium = doc.atom_mut(na).unwrap();
        sodium.charge = 1;
        sodium.map_num = 1;
        atoms.push(Participant {
            atoms: vec![na],
            coefficient: 1,
        });
        if product || !extra_chloride {
            let cl = doc.add_atom("Cl", Point::new(x + 22., 0.));
            let chloride = doc.atom_mut(cl).unwrap();
            chloride.charge = -1;
            chloride.map_num = 2;
            atoms.push(Participant {
                atoms: vec![cl],
                coefficient: 1,
            });
        }
    }
    doc.reactions.push(reaction);
    (doc, arrow)
}

#[test]
fn imbalance_names_the_element_and_charge_difference() {
    let (doc, _) = ions(true);
    let (inputs, _) = species::inputs(&doc, &doc.reactions[0]).unwrap();
    assert_eq!(
        species::validate(&inputs).unwrap_err(),
        "Products have one more Cl and charge −1 relative to reactants"
    );
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn imbalance_blocks_accept_and_two_to_two_allows_reaction_energy() {
    let (doc, arrow) = ions(true);
    let mut blocked = app(doc, arrow);
    open(&mut blocked, Mode::Energy);
    let dialog = blocked.tab.xtb.ts_review.as_ref().unwrap();
    assert!(
        dialog
            .validate()
            .unwrap_err()
            .contains("one more Cl and charge −1")
    );
    let _ = blocked.update(send(Action::Accept));
    assert!(!blocked.xtb_running());

    let (mut doc, arrow) = ions(false);
    for atom in &mut doc.atoms {
        atom.element = if atom.map_num == 1 { "O" } else { "C" }.into();
        atom.charge = 0;
    }
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert_eq!(
        app.tab
            .xtb
            .ts_review
            .as_ref()
            .unwrap()
            .validate()
            .unwrap_err(),
        SHAPE_NOTE
    );
    let _ = app.update(send(Action::Accept));
    assert!(!app.xtb_running());
    let _ = app.update(send(Action::Cancel));
    open(&mut app, Mode::Energy);
    assert!(!app.ts_review_open());
    assert!(app.xtb_running(), "{}", app.status);
}
