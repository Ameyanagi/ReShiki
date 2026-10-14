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

fn live_review(app: &App) -> (bool, Option<Document>) {
    let proposal = app.tab.mapping.proposal.as_ref().unwrap();
    let document = proposal.apply(&app.tab.doc, true).ok().map(|mut doc| {
        doc.atom_labels.maps = true;
        for atom in &mut doc.atoms {
            atom.display.mapping.show = Some(true);
        }
        doc
    });
    (proposal.current(&app.tab.doc), document)
}

fn review(app: &App) -> (bool, Option<Document>) {
    let preview = app.tab.mapping.preview.as_ref().unwrap();
    assert_eq!(preview.key, (app.tab.revision, app.tab.label_generation));
    let review = (preview.current, preview.document.clone());
    assert_eq!(review, live_review(app));
    review
}

#[test]
fn review_thumbnail_tracks_labels_and_restores_after_undo() {
    use reshiki::atom_labels::refresh::Refresh;
    let mut app = app();
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    app.tab.labels_dirty = true;
    let _ = app.update(Message::Mapping(Action::Ready(epoch, serial, Ok(proposal))));
    let (current, thumbnail) = review(&app);
    assert!(current);
    let thumbnail = thumbnail.unwrap();
    assert_eq!(thumbnail.atom(1).unwrap().label_h, 0);
    assert_eq!(thumbnail.atom(2).unwrap().label_h, 0);
    assert!(thumbnail.atom_labels.maps);
    assert!(
        thumbnail
            .atoms
            .iter()
            .all(|atom| atom.display.mapping.show == Some(true))
    );

    let revision = app.tab.revision;
    let key = app.tab.label_refresh.pending_key().unwrap();
    let labels = Refresh::calculate(&app.tab.doc, &Arc::default()).map(Arc::new);
    let _ = app.update(Message::LabelsReady(key, labels));
    assert_eq!(app.tab.revision, revision);
    let (current, thumbnail) = review(&app);
    assert!(current, "Computed labels do not change proposal freshness");
    let thumbnail = thumbnail.unwrap();
    assert_eq!(thumbnail.atom(1).unwrap().label_h, 4);
    assert_eq!(thumbnail.atom(2).unwrap().label_h, 4);
    assert_eq!(Some(thumbnail.clone()), live_review(&app).1);

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Move(vec![1], 10., 0.)));
    assert_eq!(review(&app), (false, None), "Drawing changed review state");
    let _ = app.update(Message::Undo);
    assert_eq!(review(&app), (true, Some(thumbnail.clone())));
    assert!(app.tab.revision > revision);
    let _ = app.update(Message::Redo);
    assert_eq!(review(&app), (false, None));
    let _ = app.update(Message::Undo);
    assert_eq!(review(&app), (true, Some(thumbnail)));
}

#[test]
fn background_analysis_refreshes_the_review_thumbnail_without_an_edit() {
    use crate::app::Job;
    use reshiki::engine::Response;
    let mut app = app();
    let id = app.tab.id;
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    let _ = app.update(Message::Mapping(Action::Ready(epoch, serial, Ok(proposal))));
    let before = review(&app).1.unwrap();
    assert_eq!(before.atom(1).unwrap().label_h, 0);
    let revision = app.tab.revision;
    let mut checked = app.tab.doc.clone();
    checked.atom_mut(1).unwrap().label_h = 4;
    checked.atom_mut(2).unwrap().label_h = 4;
    let front = Front::new(&mut app);
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::EngineDone {
            revision,
            kind: Job::Analyze,
            result: Box::new(Ok(Response {
                document: Some(checked),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![],
            })),
        }),
    ));
    front.assert_unchanged(&app);
    let (current, thumbnail) = app.in_tab(id, |app| review(app)).unwrap();
    assert!(current);
    assert_eq!(thumbnail.as_ref().unwrap().atom(1).unwrap().label_h, 4);
    assert_eq!(thumbnail.as_ref().unwrap().atom(2).unwrap().label_h, 4);
    let _ = app.update(Message::Tabs(Tabs::Select(id)));
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    let (current, thumbnail) = review(&app);
    assert!(current);
    let thumbnail = thumbnail.unwrap();
    assert_eq!(thumbnail.atom(1).unwrap().label_h, 4);
    assert_eq!(thumbnail.atom(2).unwrap().label_h, 4);
    assert_ne!(thumbnail, before);
    assert_eq!(Some(thumbnail), live_review(&app).1);
}

#[test]
fn review_thumbnail_tracks_uncommitted_hues_and_cancellation() {
    use crate::app::color_popover::Action as Color;
    let mut app = app();
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    let _ = app.update(Message::Mapping(Action::Ready(epoch, serial, Ok(proposal))));
    let before = review(&app).1.unwrap();
    let revision = app.tab.revision;
    for action in [Color::Color, Color::EditHues, Color::Step(1)] {
        let _ = app.update(Message::StyleMenu(action));
    }
    assert_eq!(app.tab.revision, revision);
    assert_eq!(review(&app), (false, None));
    let _ = app.update(Message::StyleMenu(Color::Cancel));
    assert_eq!(app.tab.revision, revision);
    assert_eq!(review(&app), (true, Some(before)));
    assert!(!app.tab.history.can_undo());
}

const BEFORE_ACTION: &str = "Mapping status before action";

fn action_app() -> App {
    let mut app = app();
    app.status = BEFORE_ACTION.into();
    app.error = true;
    app
}

fn assert_outcome(app: &App, status: &str, error: bool, undo_depth: usize) {
    assert_eq!(app.status, status);
    assert_eq!(app.error, error);
    assert_eq!(app.tab.history.undo_frames().len(), undo_depth);
}

#[test]
fn open_action_preserves_status_error_and_history() {
    let mut app = action_app();
    app.tab.selected = vec![1];
    let _ = app.mapping_action(Action::Open);
    assert_outcome(&app, BEFORE_ACTION, true, 0);
    assert_eq!(app.tab.mapping.atom, Some(1));
    assert_eq!(app.tab.mapping.text, "0");
}

#[test]
fn text_action_preserves_status_error_and_history() {
    let mut app = action_app();
    app.tab.selected = vec![1];
    let _ = app.mapping_action(Action::Text("27".into()));
    assert_outcome(&app, BEFORE_ACTION, true, 0);
    assert_eq!(app.tab.mapping.text, "27");
}

#[test]
fn apply_action_updates_status_error_and_one_history_frame() {
    let mut app = action_app();
    app.tab.selected = vec![1];
    app.tab.mapping.atom = Some(1);
    app.tab.mapping.text = "27".into();
    let _ = app.mapping_action(Action::Apply);
    assert_outcome(&app, "Atom mapping updated", false, 1);
    assert_eq!(app.tab.doc.atom(1).unwrap().map_num, 27);
}

#[test]
fn clear_action_updates_status_error_and_one_history_frame() {
    let mut app = action_app();
    app.tab.doc = mapping::set_map(&app.tab.doc, 1, 27).unwrap();
    app.tab.selected = vec![1];
    let _ = app.mapping_action(Action::Clear);
    assert_outcome(&app, "Atom mapping updated", false, 1);
    assert_eq!(app.tab.doc.atom(1).unwrap().map_num, 0);
}

#[test]
fn pair_action_updates_status_error_and_one_history_frame() {
    let mut app = action_app();
    app.tab.selected = vec![1, 2];
    let _ = app.mapping_action(Action::Pair);
    assert_outcome(&app, "Atom mapping updated", false, 1);
    assert_eq!(app.tab.doc.atom(1).unwrap().map_num, 1);
    assert_eq!(app.tab.doc.atom(2).unwrap().map_num, 1);
}

#[test]
fn show_action_commits_only_when_the_document_changes() {
    let mut app = action_app();
    let show = app.tab.doc.atom_labels.maps;
    let _ = app.mapping_action(Action::Show(show));
    assert_outcome(&app, "Atom mapping updated", false, 0);
    let _ = app.mapping_action(Action::Show(!show));
    assert_outcome(&app, "Atom mapping updated", false, 1);
    assert_eq!(app.tab.doc.atom_labels.maps, !show);
}

#[test]
fn position_action_sets_status_and_preserves_error_and_history() {
    let mut app = action_app();
    app.tab.doc = mapping::set_map(&app.tab.doc, 1, 27).unwrap();
    let _ = app.mapping_action(Action::Position);
    assert_outcome(&app, "Drag atom-map handles · Escape finishes", true, 0);
    assert_eq!(app.tab.selected, vec![1]);
    assert_eq!(app.tool, Tool::EditPoints);
}

#[test]
fn reset_positions_action_commits_only_when_the_document_changes() {
    let mut app = action_app();
    let _ = app.mapping_action(Action::ResetPositions);
    assert_outcome(&app, "Atom mapping updated", false, 0);
    app.tab.doc.atom_mut(1).unwrap().display.mapping.offset = Some(Point::new(0., -38.));
    let _ = app.mapping_action(Action::ResetPositions);
    assert_outcome(&app, "Atom mapping updated", false, 1);
    assert_eq!(app.tab.doc.atom(1).unwrap().display.mapping.offset, None);
}

#[test]
fn auto_action_sets_status_error_and_pending_without_history() {
    let mut app = action_app();
    assert!(app.mapping_action(Action::Auto).units() > 0);
    assert_outcome(&app, "Finding atom correspondence…", false, 0);
    assert!(app.tab.mapping.pending.is_some());
}

#[test]
fn ready_action_sets_status_error_and_proposal_without_history() {
    let mut app = action_app();
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    let _ = app.mapping_action(Action::Ready(epoch, serial, Ok(proposal.clone())));
    assert_outcome(&app, "Atom-map proposal ready to review", false, 0);
    assert!(Arc::ptr_eq(
        app.tab.mapping.proposal.as_ref().unwrap(),
        &proposal,
    ));
}

#[test]
fn apply_proposal_action_updates_status_error_and_one_history_frame() {
    let mut app = action_app();
    let (_, _, proposal) = pending_proposal(&mut app);
    app.tab.mapping.proposal = Some(proposal);
    let _ = app.mapping_action(Action::ApplyProposal);
    assert_outcome(&app, "Atom mapping updated", false, 1);
    assert!(app.tab.mapping.proposal.is_none());
}

#[test]
fn dismiss_action_preserves_status_error_and_history() {
    let mut app = action_app();
    let (_, _, proposal) = pending_proposal(&mut app);
    app.tab.mapping.proposal = Some(proposal);
    let _ = app.mapping_action(Action::Dismiss);
    assert_outcome(&app, BEFORE_ACTION, true, 0);
    assert!(app.tab.mapping.proposal.is_none());
}

#[test]
fn review_pair_action_preserves_status_error_and_history() {
    let mut app = action_app();
    let (_, _, proposal) = pending_proposal(&mut app);
    app.tab.mapping.proposal = Some(proposal);
    let _ = app.mapping_action(Action::ReviewPair(0));
    assert_outcome(&app, BEFORE_ACTION, true, 0);
    assert_eq!(app.tab.selected, vec![1, 2]);
    assert_eq!(app.tool, Tool::Select);
}

#[test]
fn align_action_updates_status_error_and_one_history_frame() {
    let mut app = action_app();
    let extra_product = app.tab.doc.add_atom("C", Point::new(300., 0.));
    reactions::assign(&mut app.tab.doc, 3, &[extra_product], Role::Product).unwrap();
    app.tab.doc = mapping::pair(&app.tab.doc, 3, &[1, 2]).unwrap();
    let _ = app.mapping_action(Action::Align);
    assert_outcome(&app, "Atom mapping updated", false, 1);
    assert_ne!(app.tab.doc.atom(2).unwrap().position, Point::new(250., 0.));
}

#[test]
fn apply_errors_keep_selection_editor_and_parse_precedence_without_history() {
    let mut app = action_app();
    app.tab.mapping.text = "invalid".into();
    let _ = app.mapping_action(Action::Apply);
    assert_outcome(&app, "Select one atom to edit its map", true, 0);
    app.tab.selected = vec![1];
    let _ = app.mapping_action(Action::Apply);
    assert_outcome(&app, "Enter the atom map for this atom first", true, 0);
    app.tab.mapping.atom = Some(1);
    for value in ["invalid", "2147483648"] {
        app.tab.mapping.text = value.into();
        let _ = app.mapping_action(Action::Apply);
        assert_outcome(
            &app,
            "Enter an atom map from 1 to 2147483647, or 0 to clear",
            true,
            0,
        );
    }
}

#[test]
fn mapping_edits_without_their_required_inputs_report_errors_without_history() {
    let mut app = action_app();
    let _ = app.mapping_action(Action::Clear);
    assert_outcome(&app, "Select one atom to edit its map", true, 0);
    let _ = app.mapping_action(Action::ApplyProposal);
    assert_outcome(&app, "Run Auto-map first", true, 0);
    app.tab.doc.reactions.clear();
    app.tab.doc.arrows.clear();
    for action in [Action::Pair, Action::Align] {
        let _ = app.mapping_action(action);
        assert_outcome(&app, "Assign reactants and products first", true, 0);
    }
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

#[test]
fn edit_atom_map_reopens_a_collapsed_atom_maps_section() {
    use crate::app::inspector::{Action as Inspector, Section};
    let mut app = app();
    app.tab.selected = vec![1];
    let _ = app.update(Message::InspectorAction(Inspector::Section(
        Section::AtomMaps,
        false,
    )));
    assert_eq!(
        app.tab.inspector_ui.expanded(Section::AtomMaps),
        Some(false)
    );
    assert!(app.update(Message::Mapping(Action::Open)).units() > 0);
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    assert_eq!(app.tab.inspector_ui.expanded(Section::AtomMaps), Some(true));
}

#[tokio::test]
#[ignore = "Opt-in real renderer Atom maps accessibility ids"]
async fn atom_map_controls_and_review_rows_publish_unique_ids() -> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::operation};
    use reshiki::accessibility::Collect;
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut app = app();
    app.tab.selected = vec![1];
    let (epoch, serial, proposal) = pending_proposal(&mut app);
    let _ = app.update(Message::Mapping(Action::Ready(epoch, serial, Ok(proposal))));
    let size = iced::Size::new(288., 2400.);
    let mut ui = iced_runtime::UserInterface::build(
        app.mapping_section(),
        size,
        iced_runtime::user_interface::Cache::new(),
        &mut renderer,
    );
    let mut collect = Collect::new(iced::Rectangle::with_size(size));
    ui.operate(&renderer, &mut operation::black_box(&mut collect));
    let snapshot = collect.snapshot();
    assert!(
        snapshot.duplicate_ids.is_empty(),
        "{:?}",
        snapshot.duplicate_ids
    );
    let node = |id: &str| {
        snapshot
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("Missing {id}"))
    };
    for id in [
        "inspector-section-AtomMaps",
        "mapping.show",
        "mapping.atom",
        "mapping.set",
        "mapping.clear",
        "mapping.auto",
        "mapping.pair",
        "mapping.align",
        "mapping.labels.move",
        "mapping.labels.reset",
        "mapping.review.0",
        "mapping.apply",
        "mapping.discard",
    ] {
        let node = node(id);
        assert!(!node.name.is_empty(), "Unnamed {id}");
        assert!(node.visible_bounds.is_some(), "Invisible {id}");
    }
    assert_eq!(
        node("mapping.show").checked,
        Some(app.tab.doc.atom_labels.maps)
    );
    // Review rows name both atoms of the pair by element and id.
    assert_eq!(node("mapping.review.0").name, "Map 1 · C #1 → C #2");
    assert!(node("mapping.apply").enabled);
    Ok(())
}

fn carbon_chain(doc: &mut Document, x: f32) -> Vec<u64> {
    let mut ids: Vec<u64> = vec![];
    for index in 0..12_u8 {
        let y = if index % 2 == 0 { 0. } else { 15. };
        let id = doc.add_atom("C", Point::new(x + 26. * f32::from(index), y));
        if let Some(&previous) = ids.last() {
            doc.add_bond(previous, id, 1, "plain");
        }
        ids.push(id);
    }
    ids
}

#[tokio::test]
#[ignore = "Opt-in real renderer Edit atom map reveal check"]
async fn edit_atom_map_reveals_its_field_above_a_long_review() -> Result<(), String> {
    use iced::advanced::{
        renderer::Headless,
        widget::operation::{self, Outcome},
    };
    use iced::futures::StreamExt;
    use reshiki::accessibility::Collect;
    type Ui<'a> = iced_runtime::UserInterface<'a, Message, iced::Theme, iced::Renderer>;
    fn field_visible(ui: &mut Ui<'_>, renderer: &iced::Renderer, size: iced::Size) -> bool {
        let mut collect = Collect::new(iced::Rectangle::with_size(size));
        ui.operate(renderer, &mut operation::black_box(&mut collect));
        collect
            .snapshot()
            .nodes
            .iter()
            .find(|node| node.id == "mapping.atom")
            .and_then(|node| {
                node.visible_bounds
                    .map(|visible| (visible.height - node.bounds.height).abs() < 0.5)
            })
            .unwrap_or(false)
    }
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    // Twelve pairs: the review is taller than the inspector's viewport.
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let reactant = carbon_chain(&mut app.tab.doc, 0.);
    let product = carbon_chain(&mut app.tab.doc, 600.);
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(360., 0.),
        Point::new(540., 0.),
        Default::default(),
        Default::default(),
    ));
    reactions::assign(&mut app.tab.doc, arrow, &reactant, Role::Reactant)?;
    reactions::assign(&mut app.tab.doc, arrow, &product, Role::Product)?;
    app.tab.saved = app.tab.doc.clone();
    let proposal = Arc::new(mapping::propose(&app.tab.doc, arrow, Budget::default())?);
    app.tab.mapping.serial = app.tab.mapping.serial.wrapping_add(1);
    let (epoch, serial) = (app.tab.file_epoch, app.tab.mapping.serial);
    app.tab.mapping.pending = Some((epoch, serial));
    let _ = app.update(Message::Mapping(Action::Ready(epoch, serial, Ok(proposal))));
    assert_eq!(
        app.tab.mapping.proposal.as_ref().map(|p| p.pairs.len()),
        Some(12)
    );
    app.tab.selected = vec![reactant[0]];
    let task = app.mapping_action(Action::Open);
    let size = iced::Size::new(1040., 680.);
    let mut ui = iced_runtime::UserInterface::build(
        app.view(),
        size,
        iced_runtime::user_interface::Cache::new(),
        &mut renderer,
    );
    // Run each widget operation as the event loop does, then look at the field.
    let mut steps = vec![];
    let mut stream = iced_runtime::task::into_stream(task).ok_or("Open scrolls the inspector")?;
    while let Some(action) = stream.next().await {
        let iced_runtime::Action::Widget(operation) = action else {
            continue;
        };
        let mut current = Some(operation);
        while let Some(mut operation) = current.take() {
            ui.operate(&renderer, operation.as_mut());
            if let Outcome::Chain(next) = operation.finish() {
                current = Some(next);
            }
        }
        steps.push(field_visible(&mut ui, &renderer, size));
    }
    assert_eq!(
        steps,
        [false, true],
        "The end of the page leaves the field above the view; the reveal shows it"
    );
    Ok(())
}
