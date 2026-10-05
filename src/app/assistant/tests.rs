use super::*;

#[test]
fn unsent_assistant_input_blocks_restart_until_cleared() {
    let mut state = State::default();
    assert!(!state.has_unfinished_work());
    state.input = text_editor::Content::with_text("Draw ferrocene");
    assert!(state.has_unfinished_work());
    state.input = text_editor::Content::new();
    state.source_image = Some(source_picture());
    assert!(state.has_unfinished_work());
    state.source_image = None;
    state.reading_image = true;
    assert!(state.has_unfinished_work());
    state.reading_image = false;
    assert!(!state.has_unfinished_work());
}

#[test]
fn sent_images_survive_composer_changes_stop_and_long_conversations() -> Result<(), String> {
    let (mut app, _) = App::new();
    let source = source_picture();
    app.assistant.source_image = Some(source.clone());
    let _ = app.assistant_action(Action::Send);
    assert_eq!(
        app.assistant
            .messages
            .first()
            .ok_or("Missing message")?
            .image,
        Some(source.clone())
    );
    let _ = app.assistant_action(Action::Stop);
    let _ = app.assistant_action(Action::ClearImage);
    for n in 0..30 {
        app.assistant.record("You", format!("Follow-up {n}"));
    }
    assert_eq!(
        app.assistant
            .messages
            .first()
            .ok_or("Missing message")?
            .image,
        Some(source.clone())
    );
    assert!(
        app.assistant
            .messages
            .last()
            .ok_or("Missing message")?
            .image
            .is_none()
    );
    assert_eq!(app.assistant.conversation().len(), 24);
    app.assistant.chat_offset = 120.;
    app.assistant.follow_chat = false;
    let task = app.assistant_action(Action::ViewImage(Some(source.clone())));
    assert_eq!(task.units(), 0);
    assert_eq!(app.assistant.viewed_image, Some(source));
    let _ = app.assistant_action(Action::ViewImage(None));
    assert_eq!(app.assistant.chat_offset, 120.);
    let _ = app.assistant_action(Action::Reset);
    assert!(app.assistant.messages.is_empty());
    assert!(app.assistant.viewed_image.is_none());
    Ok(())
}

fn source_picture() -> reshiki::pictures::Picture {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    reshiki::pictures::Picture::import(&png.into_inner()).unwrap()
}

#[test]
fn pasting_image_attaches_until_send_without_mutating_the_canvas() {
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    let picture = source_picture();
    let task = app.assistant_action(Action::ImageRead {
        serial: app.assistant.image_serial,
        epoch: app.tab.file_epoch,
        image_only: false,
        result: Ok(Some(picture.clone())),
    });
    assert_eq!(task.units(), 0);
    assert!(!app.assistant.busy);
    assert_eq!(app.assistant.source_image, Some(picture));
    assert_eq!(app.tab.doc, before);
    assert!(app.assistant.messages.is_empty());
    let task = app.assistant_action(Action::Send);
    assert!(task.units() > 0);
    assert!(app.assistant.busy);
    assert!(
        app.assistant
            .messages
            .last()
            .unwrap()
            .text
            .contains("attached image")
    );
    let _ = app.assistant_action(Action::Stop);
    assert!(!app.assistant.busy);
}

#[test]
fn late_clipboard_results_cannot_restart_reset_or_stopped_work() {
    let (mut app, _) = App::new();
    let serial = app.assistant.image_serial;
    let epoch = app.tab.file_epoch;
    let _ = app.assistant_action(Action::Reset);
    let _ = app.assistant_action(Action::ImageRead {
        serial,
        epoch,
        image_only: false,
        result: Ok(Some(source_picture())),
    });
    let _ = app.assistant_action(Action::TextPasted {
        serial,
        epoch,
        text: Some("stale text".into()),
    });
    assert!(app.assistant.source_image.is_none());
    assert!(app.assistant.input.text().trim().is_empty());
    assert!(!app.assistant.busy);
    let serial = app.assistant.image_serial;
    let _ = app.assistant_action(Action::Stop);
    let _ = app.assistant_action(Action::ImageRead {
        serial,
        epoch,
        image_only: false,
        result: Ok(Some(source_picture())),
    });
    assert!(app.assistant.source_image.is_none());
    assert!(!app.assistant.busy);
}

#[test]
fn text_paste_and_stale_document_image_keep_the_canvas_unchanged() {
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    let serial = app.assistant.image_serial;
    let epoch = app.tab.file_epoch;
    let _ = app.assistant_action(Action::TextPasted {
        serial,
        epoch,
        text: Some("Draw ethanol".into()),
    });
    assert_eq!(app.assistant.input.text().trim(), "Draw ethanol");
    let _ = app.assistant_action(Action::ImageRead {
        serial,
        epoch: epoch.wrapping_add(1),
        image_only: false,
        result: Ok(Some(source_picture())),
    });
    assert!(app.assistant.source_image.is_none());
    assert!(!app.assistant.busy);
    assert_eq!(app.tab.doc, before);
}
#[test]
fn menus_and_completion_preserve_history_scroll_until_jump_is_requested() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.assistant_action(Action::ChatScrolled {
        follow: false,
        offset: 240.,
    });
    for menu in [Menu::Models, Menu::Effort, Menu::Edits] {
        let task = app.assistant_action(Action::Menu(Some(menu)));
        assert_eq!(task.units(), 0);
        assert!(!app.assistant.follow_chat);
        assert_eq!(app.assistant.chat_offset, 240.);
        let _ = app.assistant_action(Action::Menu(None));
    }
    ready(&mut app);
    assert!(!app.assistant.follow_chat);
    assert_eq!(app.assistant.chat_offset, 240.);
    assert!(app.assistant.draft.is_some());
    let task = app.assistant_action(Action::JumpToResult);
    assert!(task.units() > 0);
    assert!(app.assistant.follow_chat);
    let _ = app.assistant_action(Action::Reset);
    assert_eq!(app.assistant.chat_offset, 0.);
}

#[test]
fn selecting_and_styling_a_scheme_keeps_the_assistant_open_for_replacement() {
    use crate::canvas::Edit;
    use reshiki::{
        arrows::{ArrowStyle, Preset},
        document::{Annotation, Arrow, Point},
        graphics::{Graphic, GraphicKind, GraphicStyle},
        typography::StyleChange,
    };
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let carbon = app.tab.doc.add_atom("C", Point::default());
    let oxygen = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(carbon, oxygen, 1, "plain");
    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(80., -30.),
        text: "Reaction conditions".into(),
        format: Default::default(),
    });
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(Arrow::new(
        arrow,
        Point::new(70., 0.),
        Point::new(140., 0.),
        Preset::Forward,
        ArrowStyle::default(),
    ));
    let graphic = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        graphic,
        GraphicKind::Rectangle,
        Point::new(-30., -50.),
        Point::new(170., 50.),
        GraphicStyle::default(),
        Default::default(),
        false,
    ));
    app.assistant.account = Some(codex::Account {
        connected: true,
        models: vec![],
    });
    let _ = app.assistant_action(Action::Open);
    let _ = app.assistant_action(Action::AutoApply(true));
    let _ = app.assistant_action(Action::Replace(true));
    let _ = app.assistant_action(Action::Example("Replace this scheme"));
    let original = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::SelectAll);
    assert_eq!(app.tab.selected, original.all_ids());
    assert_eq!(app.inspector_tab, InspectorTab::Assistant);
    for ids in [
        vec![carbon, oxygen],
        vec![caption],
        vec![arrow],
        vec![graphic],
        vec![],
    ] {
        let _ = app.update(Message::Canvas(Edit::Select(ids.clone())));
        assert_eq!(app.tab.selected, ids);
        assert!(app.inspector_open);
        assert_eq!(app.inspector_tab, InspectorTab::Assistant);
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
        assert!(app.assistant.preferences.auto_apply && app.assistant.replace);
        assert_eq!(app.assistant.input.text(), "Replace this scheme");
    }
    let _ = app.update(Message::SelectAll);
    let _ = app.update(Message::TextStyle(StyleChange::Color(
        reshiki::palette::Color::Custom([32, 80, 145]),
    )));
    let colored = app.tab.doc.clone();
    assert_ne!(colored, original);
    assert_eq!(app.inspector_tab, InspectorTab::Assistant);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.inspector_tab, InspectorTab::Assistant);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, colored);
    assert_eq!(app.inspector_tab, InspectorTab::Assistant);
    // Explicit navigation still opens the ordinary selection controls.
    let _ = app.update(Message::Inspector(InspectorTab::Properties));
    let _ = app.update(Message::Canvas(Edit::Select(vec![caption])));
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert_eq!(app.tab.caption_target, Some(caption));
}

#[test]
fn automatic_canvas_edits_wait_for_a_text_draft_and_keep_separate_undo_steps() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.assistant.preferences.auto_apply = true;
    let _ = app.inline_action(super::super::inline_text::Action::Begin(
        None,
        reshiki::document::Point::default(),
    ));
    app.caption_action(text_editor::Action::Edit(text_editor::Edit::Paste(
        String::from("Current label").into(),
    )));
    ready(&mut app);
    assert!(app.tab.doc.atoms.is_empty());
    assert!(app.assistant.waiting_for_canvas_edit);
    assert_eq!(app.tab.caption, "Current label");
    assert!(app.finish_inline(true));
    let text_document = app.tab.doc.clone();
    let _ = app.assistant_action(Action::Poll);
    assert_eq!(app.tab.doc.atoms.len(), 1);
    assert_eq!(app.tab.doc.annotations, text_document.annotations);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, text_document);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, Document::default());
}

#[test]
fn requests_poll_and_apply_in_their_original_tab() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.assistant.input = text_editor::Content::with_text("Draw water");
    app.assistant.preferences.auto_apply = true;
    let id = app.tab.id;
    let _ = app.update(Message::Assistant(Action::Send));
    assert_eq!(app.assistant.tab, Some(id));
    let canvas = app.assistant.canvas.as_ref().unwrap().clone();
    let (epoch, revision) = (app.tab.file_epoch, app.tab.revision);
    let front = super::super::tabs::tests::Front::new(&mut app);
    let _ = app.update(Message::Assistant(Action::Poll));
    let snapshot = canvas.read().unwrap();
    assert_eq!(snapshot.epoch, epoch);
    assert!(snapshot.document.all_ids().is_empty());
    drop(snapshot);
    let mut fragment = Document::default();
    fragment.add_atom("O", reshiki::document::Point::default());
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Assistant(Action::Done {
            serial: app.assistant.serial,
            epoch,
            revision,
            replace: vec![],
            result: Box::new(Ok(assistant::review::Outcome {
                proposal: Proposal::default(),
                document: fragment,
                review: assistant::review::Report {
                    verified: true,
                    ..Default::default()
                },
            })),
        })),
    ));
    front.assert_unchanged(&app);
    assert!(!app.assistant.busy && app.assistant.draft.is_none());
    assert_eq!(app.tabs.background[0].doc.atoms[0].element, "O");
    assert!(app.tabs.background[0].history.can_undo());
}

#[test]
fn closing_the_request_tab_cancels_its_canvas_and_ignores_late_results() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.assistant.input = text_editor::Content::with_text("Draw water");
    let id = app.tab.id;
    let _ = app.update(Message::Assistant(Action::Send));
    let serial = app.assistant.serial;
    let _ = app.close_active_tab();
    assert!(app.assistant.tab.is_none() && app.assistant.canvas.is_none());
    assert!(!app.assistant.busy);
    let front = super::super::tabs::tests::Front::new(&mut app);
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Assistant(Action::Done {
            serial,
            epoch: 0,
            revision: 0,
            replace: vec![],
            result: Box::new(Err("Late failure".into())),
        })),
    ));
    front.assert_unchanged(&app);
    assert!(!app.assistant.error && app.assistant.draft.is_none());
}
fn ready(app: &mut App) {
    let mut fragment = Document::default();
    fragment.add_atom("O", reshiki::document::Point::default());
    let proposal = Proposal {
        sketch: None,
        replace_ids: vec![],
        composition: Default::default(),
        explanation: "Water".into(),
        molecules: vec![assistant::Molecule {
            smiles: "O".into(),
            label: "".into(),
            coefficient: 1,
            rotation: 0.,
            compact: false,
        }],
        reactions: vec![],
    };
    let _ = app.assistant_action(Action::Done {
        serial: app.assistant.serial,
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        replace: vec![],
        result: Box::new(Ok(assistant::review::Outcome {
            proposal,
            document: fragment,
            review: assistant::review::Report {
                verified: true,
                ..Default::default()
            },
        })),
    });
}
#[test]
fn proposal_apply_is_one_undo_and_reject_is_nonmutating() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab
        .doc
        .add_atom("C", reshiki::document::Point::default());
    let before = app.tab.doc.clone();
    ready(&mut app);
    assert_eq!(app.tab.doc, before);
    let _ = app.assistant_action(Action::Reject);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.undo(&mut app.tab.doc));
    ready(&mut app);
    let _ = app.assistant_action(Action::Apply);
    assert_eq!(app.tab.doc.atoms.len(), 2);
    assert!(app.tab.history.undo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.history.redo(&mut app.tab.doc));
    assert_eq!(app.tab.doc.atoms.len(), 2);
}
#[test]
fn edits_new_documents_and_cancelled_requests_cannot_be_overwritten() {
    let (mut app, _) = App::new();
    ready(&mut app);
    let before = app.tab.doc.clone();
    app.tab
        .doc
        .add_atom("N", reshiki::document::Point::default());
    app.changed(before);
    let edited = app.tab.doc.clone();
    if let Some(draft) = &mut app.assistant.draft {
        draft.replace = edited.all_ids();
    }
    let _ = app.assistant_action(Action::Apply);
    assert_eq!(app.tab.doc, edited);
    assert!(app.assistant.draft.is_some());
    ready(&mut app);
    app.tab.file_epoch += 1;
    let _ = app.assistant_action(Action::Apply);
    assert_eq!(app.tab.doc, edited);
    let serial = app.assistant.serial;
    let _ = app.assistant_action(Action::Reset);
    let _ = app.assistant_action(Action::Done {
        serial,
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        replace: vec![],
        result: Box::new(Err("late".into())),
    });
    assert!(app.assistant.draft.is_none());
    assert!(app.assistant.status.is_empty());
}
#[test]
fn async_progress_does_not_block_editing_and_additive_proposals_rebase_safely() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.assistant_action(Action::Example("Draw water"));
    let _ = app.assistant_action(Action::Send);
    assert!(app.assistant.busy);
    let serial = app.assistant.serial;
    let before = app.tab.doc.clone();
    let atom = app
        .tab
        .doc
        .add_atom("N", reshiki::document::Point::new(120., 120.));
    app.changed(before);
    let edited = app.tab.doc.clone();
    let _ = app.assistant_action(Action::Example("Next request while waiting"));
    assert!(app.assistant.busy);
    assert_eq!(app.assistant.input.text(), "Next request while waiting");
    let _ = app.assistant_action(Action::Stop);
    assert!(!app.assistant.busy);
    let _ = app.assistant_action(Action::Done {
        serial,
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        replace: vec![],
        result: Box::new(Err("late response".into())),
    });
    assert_eq!(app.tab.doc, edited);
    ready(&mut app);
    app.tab.revision += 1;
    let _ = app.assistant_action(Action::Apply);
    assert_eq!(app.tab.doc.atom(atom), edited.atom(atom));
    assert_eq!(app.tab.doc.atoms.len(), edited.atoms.len() + 1);
    assert!(app.tab.history.undo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, edited);
}
#[test]
fn accept_all_applies_valid_proposals_as_one_undo_and_respects_changed_selection() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let original = app.tab.doc.clone();
    let _ = app.assistant_action(Action::AutoApply(true));
    ready(&mut app);
    assert!(app.assistant.draft.is_none());
    assert_eq!(app.tab.doc.atoms.len(), original.atoms.len() + 1);
    assert!(app.tab.history.undo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, original);
    assert!(app.tab.history.redo(&mut app.tab.doc));
    let _ = app.assistant_action(Action::AutoApply(false));
    let before = app.tab.doc.clone();
    ready(&mut app);
    assert_eq!(app.tab.doc, before);
    if let Some(draft) = &mut app.assistant.draft {
        draft.replace = before.all_ids();
        draft.revision = app.tab.revision.wrapping_sub(1);
    }
    let _ = app.assistant_action(Action::AutoApply(true));
    assert_eq!(app.tab.doc, before);
    assert!(app.assistant.draft.is_some());
    assert!(app.assistant.error);
}
#[test]
fn immediate_activity_stop_retains_preview_and_unverified_drafts_require_review() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.assistant_action(Action::Example("Draw water"));
    let start = std::time::Instant::now();
    let _ = app.assistant_action(Action::Send);
    assert!(start.elapsed() < std::time::Duration::from_millis(200));
    assert!(app.assistant.busy && app.assistant.started.is_some());
    assert_eq!(app.assistant.status, "Preparing your scheme…");
    let original = app.tab.doc.clone();
    let mut preview = Document::default();
    preview.add_atom("O", reshiki::document::Point::default());
    app.assistant.preview = Some(preview.clone());
    let _ = app.assistant_action(Action::Stop);
    assert!(!app.assistant.busy);
    assert_eq!(app.assistant.draft.as_ref().unwrap().fragment, preview);
    let _ = app.assistant_action(Action::AutoApply(true));
    assert_eq!(app.tab.doc, original);
    assert!(app.assistant.draft.is_some());
    let _ = app.assistant_action(Action::PreviewEdit(assistant::review::Edit::Move {
        target: "molecule:0".into(),
        dx_pt: 6.,
        dy_pt: 0.,
    }));
    assert_ne!(
        app.assistant.draft.as_ref().unwrap().fragment.atoms[0].position,
        preview.atoms[0].position
    );
    assert_eq!(app.tab.doc, original);
}
