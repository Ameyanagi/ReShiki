use super::*;
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
