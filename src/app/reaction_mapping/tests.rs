use super::*;
use crate::app::tabs::{Action as Tabs, tests::Front};
use reshiki::{
    document::{Arrow, Point},
    reactions::{self, Role},
};

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let r = app.tab.doc.add_atom("C", Point::default());
    let p = app.tab.doc.add_atom("C", Point::new(250., 0.));
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(80., 0.),
        Point::new(180., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut app.tab.doc, arrow, &[r], Role::Reactant).unwrap();
    reactions::assign(&mut app.tab.doc, arrow, &[p], Role::Product).unwrap();
    app.tab.saved = app.tab.doc.clone();
    app
}

fn pending_proposal(app: &mut App) -> (u64, u64, Arc<Proposal>) {
    let proposal = Arc::new(mapping::propose(&app.tab.doc, 3, Budget::default()).unwrap());
    app.tab.mapping.serial = app.tab.mapping.serial.wrapping_add(1);
    let (epoch, serial) = (app.tab.file_epoch, app.tab.mapping.serial);
    app.tab.mapping.pending = Some((epoch, serial));
    (epoch, serial, proposal)
}

#[test]
fn file_epoch_clears_mapping_pending_and_old_result_cannot_clear_new_work() {
    let mut app = app();
    assert!(app.update(Message::Mapping(Action::Auto)).units() > 0);
    let old = app.tab.mapping.pending.unwrap();
    app.tab
        .arrows
        .begin_source(reshiki::arrow_anchors::Pick::Atom(1));
    app.begin_file_epoch();
    assert!(!app.tab.mapping.is_pending());
    assert!(app.tab.arrows.source().is_none());
    assert!(app.update(Message::Mapping(Action::Auto)).units() > 0);
    let current = app.tab.mapping.pending.unwrap();
    assert_ne!(current, old);
    let _ = app.update(Message::Mapping(Action::Ready(
        old.0,
        old.1,
        Err("Stale mapping completion".into()),
    )));
    assert_eq!(app.tab.mapping.pending, Some(current));
    assert!(!app.error);
}

#[test]
fn completion_routes_only_to_originating_tab() {
    let mut app = app();
    let id = app.tab.id;
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    let front = Front::new(&mut app);
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Mapping(Action::Ready(
            epoch,
            serial,
            Ok(proposal.clone()),
        ))),
    ));
    front.assert_unchanged(&app);
    assert!(app.tab.mapping.pending.is_none());
    assert!(app.tab.mapping.proposal.is_none());
    let _ = app.update(Message::Tabs(Tabs::Select(id)));
    assert!(app.tab.mapping.pending.is_none());
    assert!(Arc::ptr_eq(
        app.tab.mapping.proposal.as_ref().unwrap(),
        &proposal,
    ));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert_eq!(app.status, "Atom-map proposal ready to review");
    assert!(!app.error);
}

#[test]
fn completion_for_a_closed_originating_tab_is_dropped() {
    let mut app = app();
    let id = app.tab.id;
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    app.add_tab();
    let _ = app.update(Message::Tabs(Tabs::Close(Some(id))));
    assert!(app.tab_index(id).is_none());
    let front = Front::new(&mut app);
    app.tab.mapping.pending = Some((epoch, serial));
    let count = app.strip().count();
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Mapping(Action::Ready(epoch, serial, Ok(proposal)))),
    ));
    front.assert_unchanged(&app);
    assert!(app.tab_index(id).is_none());
    assert_eq!(app.strip().count(), count);
    assert_eq!(app.tab.mapping.pending, Some((epoch, serial)));
    assert!(app.tab.mapping.proposal.is_none());
    assert!(
        app.tabs
            .background
            .iter()
            .all(|tab| { tab.mapping.pending.is_none() && tab.mapping.proposal.is_none() })
    );
}

#[test]
fn completion_for_a_replaced_drawing_preserves_its_new_request() {
    let mut app = app();
    let id = app.tab.id;
    let before = app.tab.doc.clone();
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    app.reset_tab();
    assert_eq!(app.tab.id, id);
    assert_ne!(app.tab.file_epoch, epoch);
    // Reload the same drawing so proposal freshness cannot mask an epoch error.
    app.tab.doc = before.clone();
    app.tab.saved = before.clone();
    let (replacement_epoch, replacement_serial, _) = pending_proposal(&mut app);
    assert_eq!(replacement_serial, serial);
    let revision = app.tab.revision;
    app.status = "Replacement mapping request".into();
    let front = Front::new(&mut app);
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Mapping(Action::Ready(epoch, serial, Ok(proposal)))),
    ));
    front.assert_unchanged(&app);
    assert!(app.tab.mapping.pending.is_none());
    assert!(app.tab.mapping.proposal.is_none());
    let _ = app.update(Message::Tabs(Tabs::Select(id)));
    assert_eq!(
        app.tab.mapping.pending,
        Some((replacement_epoch, replacement_serial)),
    );
    assert!(app.tab.mapping.proposal.is_none());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert_eq!(app.status, "Replacement mapping request");
}

#[test]
fn completion_is_installed_before_the_atom_text_modal_gate() {
    use crate::app::atom_text::Action as AtomText;
    let mut app = app();
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    let _ = app.update(Message::AtomText(AtomText::Begin(Some(1))));
    let _ = app.update(Message::AtomText(AtomText::Input("N".into())));
    assert!(app.tab.atom_text.is_some());
    let _ = app.update(Message::Mapping(Action::Ready(
        epoch,
        serial,
        Ok(proposal.clone()),
    )));
    assert!(app.tab.mapping.pending.is_none());
    assert!(Arc::ptr_eq(
        app.tab.mapping.proposal.as_ref().unwrap(),
        &proposal,
    ));
    assert!(app.tab.atom_text.is_some());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert_eq!(app.status, "Atom-map proposal ready to review");
    assert!(!app.error);
    let _ = app.update(Message::AtomText(AtomText::Apply));
    assert!(app.tab.atom_text.is_none());
    assert_eq!(app.tab.doc.atom(1).unwrap().element, "N");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn manual_pair_label_moves_and_proposal_acceptance_are_single_undo_steps() {
    let mut app = app();
    let original = app.tab.doc.clone();
    app.tab.selected = vec![1];
    let _ = app.update(Message::Mapping(Action::Open));
    let _ = app.update(Message::Mapping(Action::Text("27".into())));
    let _ = app.update(Message::Mapping(Action::Apply));
    assert_eq!(app.tab.doc.atom(1).unwrap().map_num, 27);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    app.tab.selected = vec![1, 2];
    let _ = app.update(Message::Mapping(Action::Pair));
    assert_eq!(
        app.tab.doc.atom(1).unwrap().map_num,
        app.tab.doc.atom(2).unwrap().map_num
    );
    let paired = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(crate::canvas::Edit::AtomIndicator(
        reshiki::atom_labels::Owner::Mapping(1),
        Point::new(0., -38.),
    )));
    assert_eq!(
        app.tab.doc.atom(1).unwrap().display.mapping.offset,
        Some(Point::new(0., -38.))
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, paired);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let proposal = Arc::new(mapping::propose(&app.tab.doc, 3, Budget::default()).unwrap());
    app.tab.mapping.pending = Some((app.tab.file_epoch, 1));
    let _ = app.update(Message::Mapping(Action::Ready(
        app.tab.file_epoch,
        1,
        Ok(proposal),
    )));
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Mapping(Action::ApplyProposal));
    assert_ne!(app.tab.doc, original);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}
#[test]
fn a_stale_worker_and_invalid_map_edit_do_not_modify_the_document() {
    let mut app = app();
    let original = app.tab.doc.clone();
    app.tab.selected = vec![1];
    let _ = app.mapping_action(Action::Open);
    let _ = app.mapping_action(Action::Text("2147483648".into()));
    let _ = app.mapping_action(Action::Apply);
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
    let proposal = Arc::new(mapping::propose(&app.tab.doc, 3, Budget::default()).unwrap());
    app.tab.mapping.pending = Some((app.tab.file_epoch, 2));
    let _ = app.mapping_action(Action::Ready(app.tab.file_epoch, 1, Ok(proposal)));
    assert!(app.tab.mapping.proposal.is_none());
    assert_eq!(app.tab.doc, original);
}

#[test]
fn a_shared_repeated_participant_rejects_pairing_without_an_undo_frame() {
    use reshiki::reactions::{Participant, Reaction};
    let mut app = app();
    let z = app.tab.doc.add_atom("C", Point::new(500., 0.));
    let second = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        second,
        Point::new(320., 0.),
        Point::new(420., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(second);
    reaction.reactants.push(Participant {
        atoms: vec![2],
        coefficient: 2,
    });
    reaction.products.push(Participant {
        atoms: vec![z],
        coefficient: 1,
    });
    app.tab.doc.reactions.push(reaction);
    app.tab.doc.validate().unwrap();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Reaction(super::super::reactions::Action::Choose(
        3,
    )));
    app.tab.selected = vec![1, 2];
    let _ = app.mapping_action(Action::Pair);
    assert!(app.error);
    assert!(app.status.contains("coefficients"));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}
