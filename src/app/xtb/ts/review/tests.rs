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

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
fn open(app: &mut App, mode: Mode) {
    let _ = app.update(Message::Xtb(XtbAction::Ts(TsAction::Review(mode, None))));
}

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

#[test]
fn palette_counts_assigned_and_inferred_arrows_together() {
    let (mut doc, inferred) = drawn_claisen();
    let mut reaction = reactions::preview(&doc, inferred).unwrap();
    assert!(reaction.ready());
    let assigned = doc.next_id();
    doc.arrows.push(Arrow::new(
        assigned,
        Point::new(110., 400.),
        Point::new(250., 400.),
        Default::default(),
        Default::default(),
    ));
    reaction.arrow = assigned;
    doc.reactions.push(reaction);

    let mut app = app(doc, assigned);
    assert_eq!(app.xtb_review_target(), Some(assigned));
    app.tab.selected.clear();
    assert_eq!(app.xtb_review_target(), None);
    app.tab.doc.reactions.clear();
    app.tab.doc.arrows[1].purpose = reshiki::purpose::Purpose::Illustration;
    assert_eq!(app.xtb_review_target(), Some(inferred));
    app.tab.selected = vec![assigned];
    assert_eq!(app.xtb_review_target(), None);
}

#[test]
fn species_title_formula_uses_plain_digits_and_the_requested_atoms() {
    let (doc, _) = claisen();
    assert_eq!(
        species_formula(&doc, &doc.reactions[0].reactants[0].atoms).as_deref(),
        Some("C5H8O")
    );
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
fn finish_mapping(app: &mut App) {
    finish_mapping_with(app, |_| {});
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
fn finish_mapping_with(app: &mut App, change: impl FnOnce(&mut Proposal)) {
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(matches!(dialog.mapping, Mapping::Finding));
    let source = with_roles(&app.tab.doc, dialog.arrow).unwrap();
    let mut proposal =
        reshiki::reaction_mapping::propose(&source, dialog.arrow, Default::default()).unwrap();
    change(&mut proposal);
    let _ = app.update(send(Action::Ready(dialog.id, Ok(Arc::new(proposal)))));
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn sure_auto_map_starts_silently_and_commits_inferred_roles_and_maps_together() {
    let (doc, arrow) = drawn_claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.xtb_running());
    assert!(
        !app.ts_review_visible(),
        "A sure map is found without a modal"
    );
    assert!(app.xtb_automatic_mapping_pending(arrow));
    assert!(!app.xtb_automatic_mapping_pending(arrow + 1));
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
    assert_eq!(app.xtb.page, Page::Xtb);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.inferred);
    assert_eq!(dialog.inputs.len(), 2);
    assert_eq!(
        dialog.ignored.as_deref(),
        Some("Ignored: reagents and conditions · H2O")
    );
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    finish_mapping_with(&mut app, |proposal| {
        assert!(proposal.complete && !proposal.multiple_best);
    });
    assert_eq!(
        Changes::from_plan(&app.tab.doc, &scheme_plan(&app.tab.doc, arrow).unwrap()).bonds,
        "Bonds formed: C1–C6 · broken: O3–C4"
    );
    assert!(!app.ts_review_open());
    assert!(!app.xtb_automatic_mapping_pending(arrow));
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
    assert_eq!(app.tab.history.undo_frames().len(), 1);
    assert_eq!(
        app.tab
            .xtb
            .run
            .as_ref()
            .unwrap()
            .ts
            .as_ref()
            .unwrap()
            .request
            .images,
        15
    );
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
    let _ = app.xtb_review_show(Mode::TransitionState, Some(arrow));
    assert!(app.ts_review_visible());
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
fn a_sure_proposal_with_moving_hydrogen_still_waits_for_review() {
    let (mut doc, arrow) = crate::app::xtb::ts::card::tests::hydrogen_transfer();
    for atom in &mut doc.atoms {
        if atom.map_num == 1 {
            atom.map_num = 0;
        }
    }
    let before = doc.clone();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(app.xtb_automatic_mapping_pending(arrow));
    finish_mapping_with(&mut app, |proposal| {
        assert!(proposal.complete && !proposal.multiple_best);
    });
    assert!(app.ts_review_visible());
    assert!(!app.xtb_automatic_mapping_pending(arrow));
    assert!(!app.xtb_running());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert_eq!(
        dialog.changes.as_ref().unwrap().hydrogens,
        ["H from C2 to O3 (least motion)"]
    );
    assert!(dialog.validate().is_ok());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn missing_isotope_maps_open_auto_map_with_an_explanation() {
    let (mut doc, arrow) = claisen();
    let attachments = [
        (doc.reactions[0].reactants[0].atoms[0], false),
        (doc.reactions[0].products[0].atoms[3], true),
    ];
    for (parent, product) in attachments {
        let position = doc.atom(parent).unwrap().position;
        let h = doc.add_atom("H", Point::new(position.x, position.y + 25.));
        doc.atom_mut(h).unwrap().isotope = 2;
        doc.add_bond(parent, h, 1, "plain");
        let reaction = &mut doc.reactions[0];
        let side = if product {
            &mut reaction.products
        } else {
            &mut reaction.reactants
        };
        side[0].atoms.push(h);
    }
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    assert!(!app.xtb_running());
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.unmapped_isotope_hydrogen);
    assert!(app.ts_review_visible());
    assert!(matches!(dialog.mapping, Mapping::Finding));
    finish_mapping(&mut app);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    assert!(dialog.unmapped_isotope_hydrogen);
    assert!(dialog.validate().is_ok());
    assert!(!app.xtb_running());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn mapper_ties_and_unfinished_searches_require_review_before_applying() {
    for tied in [true, false] {
        let (doc, arrow) = drawn_claisen();
        let before = doc.clone();
        let mut app = app(doc, arrow);
        open(&mut app, Mode::TransitionState);
        finish_mapping_with(&mut app, |proposal| {
            proposal.multiple_best = tied;
            proposal.complete = tied;
        });
        assert!(app.ts_review_visible());
        assert!(!app.xtb_running());
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
        assert!(dialog.needs_review());
        assert!(dialog.validate().is_ok());
        let _ = app.update(send(Action::Accept));
        assert!(!app.ts_review_open());
        assert!(app.xtb_running());
        assert_eq!(app.tab.history.undo_frames().len(), 1);
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn explicit_review_waits_even_with_a_complete_sure_map() {
    let (doc, arrow) = claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    app.tab.selected.clear();
    let _ = app.xtb_review_show(Mode::TransitionState, Some(arrow));
    assert!(app.ts_review_visible());
    assert!(!app.xtb_running());
    assert_eq!(app.tab.selected, [arrow]);
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
    assert!(app.tab.xtb.ts_review.as_ref().unwrap().validate().is_ok());
    let _ = app.update(send(Action::PathPoints("21".into())));
    assert!(app.tab.xtb.ts_review.as_ref().unwrap().validate().is_err());
    let _ = app.update(send(Action::PathPoints("18".into())));
    assert_eq!(
        app.tab.xtb.ts_review.as_ref().unwrap().images().unwrap(),
        18
    );
    let _ = app.update(send(Action::Cancel));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_complete_sure_map_starts_from_the_sole_target_without_review() {
    let (doc, arrow) = claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    app.tab.selected.clear();
    open(&mut app, Mode::TransitionState);
    assert!(!app.ts_review_open());
    assert!(app.xtb_running());
    assert_eq!(app.tab.selected, [arrow]);
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
    assert_eq!(app.xtb.page, Page::Xtb);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_stale_silent_map_never_edits_or_starts_the_changed_drawing() {
    let (doc, arrow) = drawn_claisen();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    let id = dialog.id;
    let source = with_roles(&app.tab.doc, arrow).unwrap();
    let proposal = reshiki::reaction_mapping::propose(&source, arrow, Default::default()).unwrap();
    app.tab.revision += 1;
    assert!(
        !app.ts_review_open(),
        "a stale hidden request must not lock the card"
    );
    let _ = app.update(send(Action::Ready(id, Ok(Arc::new(proposal)))));
    assert!(!app.ts_review_open());
    assert!(!app.xtb_running());
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.doc.reactions.is_empty());
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn a_sure_automatic_map_keeps_a_newer_selection() {
    let (doc, arrow) = drawn_claisen();
    let atom = doc.atoms[0].id;
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    app.tab.selected = vec![atom];
    finish_mapping(&mut app);
    assert!(app.xtb_running());
    assert_eq!(app.tab.selected, [atom]);
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn cancelling_silent_mapping_discards_a_late_proposal() {
    let (doc, arrow) = drawn_claisen();
    let before = doc.clone();
    let mut app = app(doc, arrow);
    open(&mut app, Mode::TransitionState);
    let dialog = app.tab.xtb.ts_review.as_ref().unwrap();
    let id = dialog.id;
    let source = with_roles(&app.tab.doc, arrow).unwrap();
    let proposal = reshiki::reaction_mapping::propose(&source, arrow, Default::default()).unwrap();
    let _ = app.update(send(Action::Cancel));
    assert!(!app.xtb_automatic_mapping_pending(arrow));
    assert_eq!(app.status, "Atom matching canceled");
    let _ = app.update(send(Action::Ready(id, Ok(Arc::new(proposal)))));
    assert!(!app.ts_review_open());
    assert!(!app.xtb_running());
    assert_eq!(app.tab.doc, before);
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
    assert!(blocked.ts_review_visible());
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
    assert!(!app.ts_review_open());
    assert_eq!(app.status, SHAPE_NOTE);
    assert!(!app.xtb_running());
    open(&mut app, Mode::Energy);
    assert!(!app.ts_review_open());
    assert!(app.xtb_running(), "{}", app.status);
}
