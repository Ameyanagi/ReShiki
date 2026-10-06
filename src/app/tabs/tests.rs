use super::*;
use crate::app::inspector;
use reshiki::document::{Document, Point};
use reshiki::engine::Response;

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app
}

fn edit(app: &mut App, element: &str) -> u64 {
    let before = app.tab.doc.clone();
    let id = app.tab.doc.add_atom(element, Point::default());
    app.changed(before);
    id
}

fn imported(element: &str) -> Box<Result<Response, String>> {
    let mut document = Document::default();
    document.add_atom(element, Point::default());
    Box::new(Ok(Response {
        document: Some(document),
        analysis: None,
        output: None,
        engine_version: "test".into(),
        warnings: vec![],
    }))
}

fn select(app: &mut App, id: TabId) {
    let _ = app.update(Message::Tabs(Action::Select(id)));
}

pub(in crate::app) struct Front {
    id: TabId,
    index: usize,
    document: Document,
    revision: u64,
    selected: Vec<u64>,
}

impl Front {
    pub(in crate::app) fn new(app: &mut App) -> Self {
        app.add_tab();
        let atom = edit(app, "S");
        app.tab.selected = vec![atom];
        app.tab.camera.center = Point::new(90., -30.);
        app.tab.camera.zoom = 1.7;
        app.tool = crate::canvas::Tool::Erase;
        app.inspector_open = false;
        app.inspector_tab = super::super::InspectorTab::Import;
        app.tab.inspector_ui.update(inspector::Action::Section(
            inspector::Section::Transform,
            true,
        ));
        app.context_menu = Some(super::super::context_menu::State::new(
            iced::Point::new(12., 34.),
            Default::default(),
        ));
        app.style_menu = Some(super::super::color_popover::Menu::Align);
        app.imports.menu = true;
        app.tabs.menu = true;
        app.help_open = true;
        app.view_open = true;
        app.status = "Front tab status".into();
        app.error = true;
        Self {
            id: app.tab.id,
            index: app.tabs.active,
            document: app.tab.doc.clone(),
            revision: app.tab.revision,
            selected: app.tab.selected.clone(),
        }
    }

    pub(in crate::app) fn assert_unchanged(&self, app: &App) {
        assert_eq!(app.tab.id, self.id);
        assert_eq!(app.tabs.active, self.index);
        assert_eq!(app.tab.doc, self.document);
        assert_eq!(app.tab.revision, self.revision);
        assert_eq!(app.tab.selected, self.selected);
        assert_eq!(app.tab.camera.center, Point::new(90., -30.));
        assert_eq!(app.tab.camera.zoom, 1.7);
        assert_eq!(app.tool, crate::canvas::Tool::Erase);
        assert!(!app.inspector_open);
        assert_eq!(app.inspector_tab, super::super::InspectorTab::Import);
        assert_eq!(
            app.tab.inspector_ui.expanded(inspector::Section::Transform),
            Some(true)
        );
        assert_eq!(
            app.context_menu.as_ref().unwrap().position,
            iced::Point::new(12., 34.)
        );
        assert!(matches!(
            app.style_menu,
            Some(super::super::color_popover::Menu::Align)
        ));
        assert!(app.imports.menu && app.tabs.menu && app.help_open && app.view_open);
        assert_eq!(app.status, "Front tab status");
        assert!(app.error);
        assert!(!app.tab.busy && !app.tab.clipboard_busy);
        assert!(
            app.tab.labels_dirty,
            "Follow-up labels belong to the background tab"
        );
    }
}

#[test]
fn tabs_shrink_to_a_minimum_then_the_rest_move_into_the_list() {
    let widths = [150., 100., 120.];
    let wide = 2000.;
    assert_eq!(fit(&widths, 0, wide), (0..3, f32::INFINITY));
    // Wide tabs shrink first, down to the others' width.
    let room = 100. + 100. + 100. + 2. * TAB_GAP + BUTTON + SPACING;
    let (visible, limit) = fit(&widths, 0, room);
    assert_eq!(visible, 0..3);
    assert!((limit - 100.).abs() < 0.01, "{limit}");
    // Below the minimum width, the tabs around the active one show.
    let room = 2. * TAB_MIN + TAB_GAP + BUTTON + SPACING;
    assert_eq!(fit(&widths, 0, room).0, 0..2);
    assert_eq!(fit(&widths, 2, room).0, 1..3);
    // Crowded by busy commands, the active tab alone shrinks, then the
    // list holds every tab.
    let room = TAB_FLOOR + BUTTON + SPACING;
    assert_eq!(fit(&widths, 2, room), (2..3, TAB_FLOOR));
    assert!(fit(&widths, 2, room - 1.).0.is_empty());
    assert_eq!(fit(&widths[..1], 0, room), (0..1, TAB_FLOOR));
}

#[test]
fn long_names_end_with_an_ellipsis() {
    assert_eq!(elide("a.rsk", 200.), "a.rsk");
    let short = elide("a very long drawing name.rsk", 60.);
    assert!(
        short.ends_with('…') && text_width(&short, 12.) <= 60.5,
        "{short}"
    );
}

#[test]
fn new_takes_an_unchanged_empty_tab_and_otherwise_adds_one() {
    let mut app = ready();
    let first = app.tab.id;
    app.tab.caption_format.style.bold = true;
    let _ = app.update(Message::New);
    assert_eq!(app.tab.id, first, "An empty Untitled tab is reused");
    assert!(app.tabs.background.is_empty());
    assert!(!app.tab.caption_format.style.bold, "New starts over");
    edit(&mut app, "N");
    let _ = app.update(Message::New);
    assert_ne!(app.tab.id, first);
    assert_eq!(app.strip().count(), 2);
    assert_eq!(app.tabs.active, 1, "A new tab opens at the end, in front");
    assert!(app.tab.doc.all_ids().is_empty() && !app.dirty());
    assert!(app.tabs.background[0].edited);
    assert_ne!(
        app.tabs.background[0].file_epoch, app.tab.file_epoch,
        "Epochs are unique across tabs"
    );
}

#[test]
fn switching_keeps_each_tabs_view_selection_history_and_sections() {
    let mut app = ready();
    let atom = edit(&mut app, "N");
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![atom])));
    app.tab.camera.zoom = 2.;
    app.tab.camera.center = Point::new(40., -10.);
    let _ = app.update(Message::InspectorAction(inspector::Action::Section(
        inspector::Section::Transform,
        true,
    )));
    let first = app.tab.id;
    let _ = app.update(Message::New);
    edit(&mut app, "O");
    let second = app.tab.id;
    app.tool = crate::canvas::Tool::Erase;
    let _ = app.update(Message::Tabs(Action::Cycle(true)));
    assert_eq!(app.tab.id, first, "⌃Tab wraps around");
    assert_eq!(app.tab.selected, vec![atom]);
    assert_eq!(app.tab.camera.zoom, 2.);
    assert_eq!(app.tab.camera.center, Point::new(40., -10.));
    assert_eq!(
        app.tab.inspector_ui.expanded(inspector::Section::Transform),
        Some(true)
    );
    assert_eq!(app.tool, crate::canvas::Tool::Erase, "The tool is app-wide");
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.atoms.is_empty(), "Undo edits the tab in front");
    let _ = app.update(Message::Tabs(Action::Number(9)));
    assert_eq!(app.tab.id, second, "⌘9 is the last tab");
    assert_eq!(app.tab.doc.atoms[0].element, "O");
    assert!(app.tab.history.can_undo());
    let _ = app.update(Message::Tabs(Action::Number(1)));
    assert_eq!(app.tab.id, first);
    let _ = app.update(Message::Tabs(Action::Number(5)));
    assert_eq!(app.tab.id, first, "No fifth tab");
}

async fn keyboard_labels_fixture() -> (App, u64, u64, Message) {
    use iced::futures::StreamExt;
    use reshiki::keyboard_drawing::Target;

    let mut app = ready();
    let before = app.tab.doc.clone();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.changed(before);
    app.tab.selected = vec![b];
    app.sync_keyboard_drawing();
    app.tab
        .keyboard_drawing
        .set_target(Target::Atom(b), &app.tab.doc);
    app.tab.keyboard_drawing.mark(a);
    let mut labels = iced_runtime::task::into_stream(app.start_label_refresh())
        .expect("A real label calculation starts");
    let Some(iced_runtime::Action::Output(message)) = labels.next().await else {
        panic!("The label task must return its tagged completion");
    };
    assert!(matches!(&message, Message::LabelsReady(_, Ok(_))));
    (app, a, b, message)
}

#[tokio::test]
async fn background_labels_preserve_keyboard_target_and_mark_while_front_tool_is_text() {
    use crate::canvas::Tool;
    use reshiki::keyboard_drawing::Target;

    let (mut app, a, b, labels) = keyboard_labels_fixture().await;
    let first = app.tab.id;
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::New);
    let second = app.tab.id;
    let _ = app.update(Message::Tool(Tool::Text));
    let _ = app.update(Message::Tab(first, Box::new(labels)));
    assert_eq!(app.tab.id, second);
    assert_eq!(app.tool, Tool::Text);
    let background = &app.tabs.background[0];
    assert_eq!(background.keyboard_drawing.target(), Target::Atom(b));
    assert_eq!(background.keyboard_drawing.marked(), Some(a));
    assert!(background.keyboard_drawing.enabled());
    assert_eq!(background.revision, revision);
    assert!(super::super::same_drawing(&background.doc, &drawing));
    assert_eq!(background.doc.atom(b).unwrap().label_h, 3);

    let _ = app.update(Message::Tool(Tool::Select));
    select(&mut app, first);
    assert!(app.keyboard_drawing_active());
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b));
    assert_eq!(app.tab.keyboard_drawing.marked(), Some(a));
}

#[tokio::test]
async fn background_labels_reconcile_deleted_targets_and_new_document_epochs() {
    use crate::canvas::Tool;
    use reshiki::keyboard_drawing::Target;

    for new_epoch in [false, true] {
        let (mut app, a, b, labels) = keyboard_labels_fixture().await;
        let first = app.tab.id;
        app.tab
            .keyboard_drawing
            .set_target(Target::Atom(a), &app.tab.doc);
        if new_epoch {
            app.tab.keyboard_drawing.leave();
        }
        let _ = app.update(Message::New);
        let _ = app.update(Message::Tool(Tool::Text));
        if new_epoch {
            let epoch = app.next_epoch();
            let _ = app.in_tab(first, |app| {
                let mut replacement = Document::default();
                let reused = replacement.add_atom("N", Point::new(200., 100.));
                assert_eq!(reused, a);
                app.tab.doc = replacement;
                app.tab.file_epoch = epoch;
                app.tab.selected = vec![reused];
            });
        } else {
            let _ = app.in_tab(first, |app| app.tab.doc.delete(&[a]));
        }
        let _ = app.update(Message::Tab(first, Box::new(labels)));
        assert_eq!(app.tool, Tool::Text);
        let background = &app.tabs.background[0];
        assert_eq!(
            background.keyboard_drawing.target(),
            Target::Blank(Point::default())
        );
        assert_eq!(background.keyboard_drawing.marked(), None);
        assert_eq!(background.keyboard_drawing.enabled(), !new_epoch);
        if !new_epoch {
            assert!(
                background.doc.atom(b).is_some(),
                "A remaining hotspot is not selected"
            );
        }
    }
}

#[test]
fn background_engine_results_apply_in_their_own_history() {
    for kind in [Job::Insert, Job::Import, Job::ImportFile] {
        let mut app = ready();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Analyze);
        let (id, revision) = (app.tab.id, app.tab.revision);
        let front = Front::new(&mut app);
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::EngineDone {
                revision,
                kind,
                result: imported("O"),
            }),
        ));
        front.assert_unchanged(&app);
        let tab = &app.tabs.background[0];
        assert!(!tab.busy && tab.edited);
        assert_eq!(tab.doc.atoms[0].element, "O");
        select(&mut app, id);
        assert!(app.status.contains("structure") || app.status.contains("Structure"));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(
            !app.tab.history.can_undo(),
            "Exactly one step for the result"
        );
    }
}

#[test]
fn background_checks_and_cleanup_previews_stay_with_their_drawing() {
    for cleanup in [false, true] {
        let mut app = ready();
        let atom = app.tab.doc.add_atom("N", Point::default());
        app.tab.selected = vec![atom];
        app.tab.busy = true;
        let (id, revision) = (app.tab.id, app.tab.revision);
        let before = app.tab.doc.clone();
        let mut computed = before.clone();
        let kind = if cleanup {
            computed.atoms[0].position = Point::new(42., 10.);
            Job::Clean(super::super::cleanup::CleanupJob {
                options: Default::default(),
                selection: vec![atom],
                serial: app.tab.cleanup_serial,
                epoch: app.tab.file_epoch,
            })
        } else {
            computed.atoms[0].label_h = 3;
            Job::Analyze
        };
        let front = Front::new(&mut app);
        assert_eq!(
            app.update(Message::Tab(
                id,
                Box::new(Message::EngineDone {
                    revision,
                    kind,
                    result: Box::new(Ok(Response {
                        document: Some(computed.clone()),
                        analysis: None,
                        output: None,
                        engine_version: "test".into(),
                        warnings: vec![],
                    })),
                })
            ))
            .units(),
            0
        );
        front.assert_unchanged(&app);
        let tab = &app.tabs.background[0];
        assert!(!tab.busy && !tab.history.can_undo());
        if cleanup {
            assert_eq!(tab.doc, before);
            assert_eq!(tab.cleanup.as_ref().unwrap().document, computed);
            select(&mut app, id);
            let _ = app.update(Message::Cleanup(crate::app::cleanup::Action::Apply));
            assert_eq!(app.tab.doc, computed);
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
        } else {
            assert_eq!(tab.doc, computed);
            assert_eq!(tab.status, "No chemistry errors found");
        }
    }
}

#[test]
fn background_pasted_drawing_is_one_undo_step() {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let id = app.tab.id;
    let mut part = Document::default();
    part.add_atom("O", Point::default());
    let contents = format!(
        "{}{}",
        reshiki::editing::CLIPBOARD_PREFIX,
        serde_json::to_string(&part.current()).unwrap()
    );
    let front = Front::new(&mut app);
    let _ = app.update(Message::Tab(id, Box::new(Message::Pasted(Some(contents)))));
    front.assert_unchanged(&app);
    select(&mut app, id);
    assert_eq!(app.status, "Selection pasted");
    assert_eq!(app.tab.doc.atoms[0].element, "O");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn background_edits_cannot_arrive_after_the_window_close_check() {
    let mut app = ready();
    let id = app.tab.id;
    let revision = app.tab.revision;
    app.tab.busy = true;
    app.add_tab();
    let directory = tempfile::tempdir().unwrap();
    app.tab.recovery = Some(reshiki::recovery::Recovery::in_directory(directory.path()).unwrap());
    let _ = app.close_window(iced::window::Id::unique(), vec![]);
    assert!(app.exit.closing());
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::EngineDone {
            revision,
            kind: Job::Insert,
            result: imported("O"),
        }),
    ));
    assert!(app.strip().all(|tab| tab.doc.all_ids().is_empty()));
    assert!(app.exit.closing());
}

#[test]
fn background_pasted_text_starts_its_follow_up_in_the_same_tab() {
    let mut app = ready();
    let (id, revision) = (app.tab.id, app.tab.revision);
    let front = Front::new(&mut app);
    let task = app.update(Message::Tab(
        id,
        Box::new(Message::Pasted(Some("CO".into()))),
    ));
    assert!(task.units() > 0);
    front.assert_unchanged(&app);
    assert!(app.tabs.background[0].busy);
    assert_eq!(app.tabs.background[0].status, "Working…");
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::EngineDone {
            revision,
            kind: Job::Insert,
            result: imported("O"),
        }),
    ));
    front.assert_unchanged(&app);
    assert!(!app.tabs.background[0].busy);
    assert!(
        !app.tabs.background[0].labels_dirty,
        "Its label task was started too"
    );
    assert_eq!(app.tabs.background[0].doc.atoms[0].element, "O");
}

#[test]
fn stale_and_failed_background_engine_results_release_their_tab() {
    for failed in [false, true] {
        let mut app = ready();
        let _ = app.update(Message::Analyze);
        let (id, revision) = (app.tab.id, app.tab.revision);
        edit(&mut app, "N");
        let before = app.tab.doc.clone();
        let front = Front::new(&mut app);
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::EngineDone {
                revision,
                kind: Job::Insert,
                result: if failed {
                    Box::new(Err("Calculation failed".into()))
                } else {
                    imported("O")
                },
            }),
        ));
        front.assert_unchanged(&app);
        let tab = &app.tabs.background[0];
        assert_eq!(tab.doc, before);
        assert!(!tab.busy);
        assert_eq!(tab.error, failed);
        assert!(tab.status.contains(if failed {
            "Calculation failed"
        } else {
            "newer edits"
        }));
    }
}

#[test]
fn closed_tab_results_are_dropped_and_do_not_leave_busy_flags() {
    let mut app = ready();
    let id = app.tab.id;
    app.tab.busy = true;
    app.tab.clipboard_busy = true;
    app.add_tab();
    let _ = app.update(Message::Tabs(Action::Close(Some(id))));
    assert!(app.tab_index(id).is_none());
    let front = Front::new(&mut app);
    for message in [
        Message::EngineDone {
            revision: 0,
            kind: Job::Insert,
            result: imported("O"),
        },
        Message::ClipboardRead {
            epoch: 0,
            revision: 0,
            result: Box::new(Err("late read".into())),
        },
        Message::ClipboardWritten {
            epoch: 0,
            revision: 0,
            cut_ids: vec![1],
            result: Err("late copy".into()),
        },
        Message::Pasted(Some("CO".into())),
    ] {
        assert_eq!(app.update(Message::Tab(id, Box::new(message))).units(), 0);
        front.assert_unchanged(&app);
        assert!(app.strip().all(|tab| !tab.busy && !tab.clipboard_busy));
    }
}

#[test]
fn nested_tab_work_keeps_each_status_with_its_document() {
    let mut app = ready();
    let first = app.tab.id;
    app.status = "First".into();
    app.error = true;
    app.add_tab();
    let second = app.tab.id;
    app.status = "Second".into();
    app.in_tab(first, |app| {
        assert_eq!(app.status, "First");
        assert!(app.error);
        app.in_tab(second, |app| {
            assert_eq!(app.status, "Second");
            assert!(!app.error);
            app.status = "Second updated".into();
            app.error = true;
        });
        assert_eq!(app.status, "First");
        app.status = "First updated".into();
        app.error = false;
    });
    assert_eq!(app.tab.id, second);
    assert_eq!(app.status, "Second updated");
    assert!(app.error);
    select(&mut app, first);
    assert_eq!(app.status, "First updated");
    assert!(!app.error);
}

#[test]
fn status_and_errors_follow_tabs_through_switches_and_background_results() {
    let mut app = ready();
    let first = app.tab.id;
    app.status = "First error".into();
    app.error = true;
    app.add_tab();
    let second = app.tab.id;
    assert_eq!(app.status, super::super::READY);
    assert!(!app.error);
    app.status = "Second status".into();
    for _ in 0..2 {
        select(&mut app, first);
        assert_eq!(app.status, "First error");
        assert!(app.error);
        select(&mut app, second);
        assert_eq!(app.status, "Second status");
        assert!(!app.error);
    }
    let _ = app.update(Message::Tab(
        first,
        Box::new(Message::EngineDone {
            revision: 0,
            kind: Job::Analyze,
            result: Box::new(Err("Background error".into())),
        }),
    ));
    assert_eq!(app.status, "Second status");
    assert!(!app.error);
    select(&mut app, first);
    assert_eq!(app.status, "Background error");
    assert!(app.error);
    select(&mut app, second);
    let _ = app.update(Message::Tabs(Action::Close(None)));
    assert_eq!(app.tab.id, first);
    assert_eq!(app.status, "Background error");
    assert!(app.error);
}

#[test]
fn a_save_that_finishes_behind_the_front_marks_its_own_tab_saved() {
    let mut app = ready();
    edit(&mut app, "N");
    let (first, epoch, snapshot) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
    app.file_io.saving = true;
    let _ = app.update(Message::New);
    edit(&mut app, "C");
    let _ = app.update(Message::Tabs(Action::Close(None)));
    assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
    let _ = app.update(Message::Tab(
        first,
        Box::new(Message::Saved(
            epoch,
            Box::new(snapshot),
            Ok(Some("first.rsk".into())),
        )),
    ));
    assert!(!app.file_io.saving);
    assert!(app.tab.path.is_none() && app.dirty());
    assert!(
        matches!(app.pending, Some(Pending::CloseTab(_))),
        "The dialog stays with the tab in front"
    );
    let saved = &app.tabs.background[0];
    assert_eq!(saved.path, Some("first.rsk".into()));
    assert!(!saved.edited);
}

#[test]
fn a_dialogs_save_waits_only_for_its_own_tabs_save_in_progress() {
    let saved = |app: &mut App, id, epoch, snapshot| {
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::Saved(
                epoch,
                Box::new(snapshot),
                Ok(Some("first.rsk".into())),
            )),
        ));
    };
    // Another tab's save cannot continue the dialog's close, so the
    // answer cancels it instead of leaving tabs and closing blocked.
    let mut app = ready();
    edit(&mut app, "N");
    app.tab.path = Some("first.rsk".into());
    let (first, epoch, snapshot) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
    assert!(app.update(Message::Save).units() > 0);
    let _ = app.update(Message::New);
    edit(&mut app, "C");
    let _ = app.update(Message::Tabs(Action::Close(None)));
    assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
    let _ = app.update(Message::Save);
    assert!(app.pending.is_none());
    saved(&mut app, first, epoch, snapshot);
    assert!(app.pending.is_none() && !app.tabs.background[0].edited);
    let _ = app.update(Message::Tabs(Action::Cycle(true)));
    assert_eq!(app.tab.id, first, "Tabs switch again");
    // The tab's own save in progress still closes it once it lands.
    let mut app = ready();
    edit(&mut app, "N");
    app.tab.path = Some("first.rsk".into());
    let (first, epoch, snapshot) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
    assert!(app.update(Message::Save).units() > 0);
    let _ = app.update(Message::Tabs(Action::Close(None)));
    let _ = app.update(Message::Save);
    assert!(matches!(app.pending, Some(Pending::CloseTab(id)) if id == first));
    saved(&mut app, first, epoch, snapshot);
    assert!(app.pending.is_none() && app.tab.id != first);
}

#[test]
fn the_dialogs_save_closes_its_tab_after_a_finder_file_took_the_front() {
    let mut app = ready();
    edit(&mut app, "N");
    let (asked, epoch, snapshot) = (app.tab.id, app.tab.file_epoch, app.tab.doc.clone());
    let _ = app.update(Message::Tabs(Action::Close(None)));
    assert!(app.update(Message::Save).units() > 0);
    let mut finder = Document::default();
    finder.add_atom("S", Point::default());
    let _ = app.update(Message::FilePrepared(Some((
        "finder.rsk".into(),
        Ok(super::super::files::Prepared::Native(Box::new(finder))),
    ))));
    assert_ne!(app.tab.id, asked);
    let _ = app.update(Message::Tab(
        asked,
        Box::new(Message::Saved(
            epoch,
            Box::new(snapshot),
            Ok(Some("asked.rsk".into())),
        )),
    ));
    assert!(app.pending.is_none(), "The dialog's close went on");
    assert!(app.tab_index(asked).is_none());
    assert_eq!(app.tab.path, Some("finder.rsk".into()));
}

#[test]
fn a_structure_file_opened_beside_a_drawing_is_imported_into_its_new_tab() {
    let mut app = ready();
    edit(&mut app, "N");
    let first = app.tab.id;
    let opened = Some((
        "ethanol.mol".into(),
        Ok(super::super::files::Prepared::Import {
            format: "mol",
            contents: String::new(),
        }),
    ));
    assert!(app.update(Message::FilePrepared(opened)).units() > 0);
    assert_ne!(app.tab.id, first);
    assert!(app.tab.busy && !app.tabs.background[0].busy);
    // `update` marks the engine's result with the tab now in front.
    let _ = app.update(Message::Tab(
        app.tab.id,
        Box::new(Message::EngineDone {
            revision: app.tab.revision,
            kind: Job::ImportFile,
            result: imported("O"),
        }),
    ));
    assert_eq!(app.tab.doc.atoms[0].element, "O");
    assert_eq!(app.tabs.background[0].doc.atoms[0].element, "N");
}

#[test]
fn closing_tabs_asks_only_for_unsaved_ones_and_the_last_leaves_an_empty_tab() {
    let mut app = ready();
    edit(&mut app, "N");
    let first = app.tab.id;
    let _ = app.update(Message::New);
    let second = app.tab.id;
    // A saved tab behind the front one closes where it is.
    let _ = app.update(Message::Tabs(Action::Close(Some(second))));
    assert_eq!(app.tab.id, first);
    assert!(app.pending.is_none());
    let _ = app.update(Message::New);
    let third = app.tab.id;
    // An unsaved tab comes to the front for the dialog.
    let task = app.update(Message::Tabs(Action::Close(Some(first))));
    assert!(task.units() > 0);
    assert_eq!(app.tab.id, first);
    assert!(matches!(app.pending, Some(Pending::CloseTab(id)) if id == first));
    let _ = app.update(Message::Discard);
    assert_eq!(app.tab.id, third);
    assert_eq!(app.strip().count(), 1);
    let _ = app.update(Message::Tabs(Action::Close(None)));
    assert_ne!(app.tab.id, third, "The last tab gives way to a new one");
    assert_eq!(app.strip().count(), 1);
    assert!(app.tab.reusable());
}

#[test]
fn window_close_asks_about_each_unsaved_tab_in_turn_and_cancel_stops_it() {
    let window = iced::window::Id::unique();
    let mut app = ready();
    edit(&mut app, "N");
    let first = app.tab.id;
    let _ = app.update(Message::New);
    let saved = app.tab.id;
    app.tab.path = Some("saved.rsk".into());
    let _ = app.update(Message::New);
    edit(&mut app, "O");
    let third = app.tab.id;
    select(&mut app, saved);
    assert!(app.update(Message::Close(window)).units() > 0);
    assert_eq!(
        app.tab.id, first,
        "The first unsaved tab comes to the front"
    );
    let _ = app.update(Message::Discard);
    assert_eq!(app.tab.id, third);
    assert!(
        matches!(&app.pending, Some(Pending::CloseWindow(_, id, discarded))
            if *id == third && *discarded == [first])
    );
    let _ = app.update(Message::Cancel);
    assert!(app.pending.is_none() && !app.exit.closing());
    assert_eq!(app.strip().count(), 3, "Cancel keeps every tab");
    // Saving the last unsaved tab continues the close.
    let _ = app.update(Message::Close(window));
    let _ = app.update(Message::Discard);
    let epoch = app.tab.file_epoch;
    let snapshot = Box::new(app.tab.doc.clone());
    assert!(
        app.update(Message::Saved(
            epoch,
            snapshot,
            Ok(Some("third.rsk".into()))
        ))
        .units()
            > 0,
        "No draft to remove, so the window closes"
    );
    assert!(app.pending.is_none());
}

#[test]
fn tab_shortcuts_route_before_fields_and_do_not_collide() {
    use super::super::{file_shortcuts::file_message, shortcuts::key_message};
    use iced::keyboard::{Key, Modifiers, key::Named};
    let command = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    let tab = Key::Named(Named::Tab);
    let message = |key: &Key, modifiers| format!("{:?}", file_message(key, modifiers));
    assert_eq!(message(&tab, Modifiers::CTRL), "Some(Tabs(Cycle(true)))");
    assert_eq!(
        message(&tab, Modifiers::CTRL | Modifiers::SHIFT),
        "Some(Tabs(Cycle(false)))"
    );
    assert_eq!(
        message(&Key::Character("w".into()), command),
        "Some(Tabs(Close(None)))"
    );
    for n in 1..=9 {
        let key = Key::Character(n.to_string().into());
        assert_eq!(message(&key, command), format!("Some(Tabs(Number({n})))"));
        // No drawing command uses these keys.
        assert!(key_message(&key, &key, command).is_none());
    }
    assert!(key_message(&tab, &tab, Modifiers::CTRL).is_none());
    let w = Key::Character("w".into());
    assert!(key_message(&w, &w, command).is_none());
}
