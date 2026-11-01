use super::*;
use crate::app::import::{self, InputKind};
use reshiki::naming::Provenance;

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.imports.kind = InputKind::Name;
    let _ = app.naming_action(Action::Name("ethanol".into()));
    app
}

fn record() -> Record {
    Record {
        title: "ethanol".into(),
        systematic_name: None,
        name_status: None,
        provenance: Provenance::OpsinRust,
        ..super::super::test_support::record("CCO")
    }
}

async fn preview(app: &App, smiles: &str) -> Result<Preview, String> {
    prepare_preview(app.engine.clone(), record(), smiles.into())
        .await
        .map_err(|error| error.to_string())
}

#[tokio::test]
async fn name_insert_keeps_the_camera_when_structure_and_caption_fit() -> Result<(), String> {
    for rulers in [false, true] {
        let mut app = ready();
        app.engine = LocalEngine::in_process();
        app.tab.doc = Document::default();
        app.viewport = iced::Size::new(1200., 800.);
        app.guides.rulers = rulers;
        app.tab.camera = Camera {
            center: Point::new(140., -80.),
            zoom: 1.5,
        };
        let camera = app.tab.camera;
        let result = preview(&app, "CCO").await?;
        app.tab.naming.set_preview(result);

        app.insert_import_preview(true);

        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.annotations.len(), 1);
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);
        let paper = app.guides.paper(iced::Rectangle::with_size(app.viewport));
        let visible = iced::Rectangle::with_size(paper.size());
        let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.selected).unwrap();
        assert!(visible.contains(app.tab.camera.screen(lo, paper)));
        assert!(visible.contains(app.tab.camera.screen(hi, paper)));
    }
    Ok(())
}

#[tokio::test]
async fn name_insert_reveals_both_structures_and_captions_without_zooming_in() -> Result<(), String>
{
    let mut app = ready();
    app.engine = LocalEngine::in_process();
    app.tab.doc = Document::default();
    app.viewport = iced::Size::new(1200., 800.);
    app.tab.camera = Camera {
        center: Point::new(300., 0.),
        zoom: 1.25,
    };
    let result = preview(&app, "CCO").await?;
    app.tab.naming.set_preview(result.clone());
    app.insert_import_preview(true);
    assert!(!app.error, "{}", app.status);
    app.tab.camera.center = Point::default();
    let camera = app.tab.camera;
    let paper = app.guides.paper(iced::Rectangle::with_size(app.viewport));
    let visible = iced::Rectangle::with_size(paper.size());
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.doc.all_ids()).unwrap();
    assert!(camera.screen(lo, paper).x > paper.width * 2. / 3.);
    assert!(visible.contains(camera.screen(hi, paper)));
    app.tab.naming.set_preview(result);

    app.insert_import_preview(true);

    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.atoms.len(), 6);
    assert_eq!(app.tab.doc.annotations.len(), 2);
    assert_eq!(app.tab.doc.molecule_names.len(), 2);
    assert!(app.tab.camera.zoom <= camera.zoom);
    let (lo, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &app.tab.doc.all_ids()).unwrap();
    assert!(visible.contains(app.tab.camera.screen(lo, paper)));
    assert!(visible.contains(app.tab.camera.screen(hi, paper)));
    Ok(())
}

fn complete(app: &mut App, ticket: Ticket, preview: Preview) {
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Names(
            vec![preview.record.clone()],
            Some(Box::new(preview)),
        ))),
    ));
}

#[tokio::test]
async fn clearing_name_input_closes_preview_and_reopening_selects_the_retained_name() {
    use iced::advanced::widget::{Id, operation::Outcome};
    use iced::futures::StreamExt;
    use iced::widget::text_input::{State, Value};
    type InputState = State<<iced::Renderer as iced::advanced::text::Renderer>::Paragraph>;

    async fn input_state(task: Task<Message>) -> InputState {
        let mut state = InputState::new();
        let id = Id::new("import-name");
        let mut actions = iced_runtime::task::into_stream(task).expect("Name input widget task");
        while let Some(action) = actions.next().await {
            let iced_runtime::Action::Widget(mut operation) = action else {
                continue;
            };
            loop {
                operation.focusable(Some(&id), iced::Rectangle::default(), &mut state);
                operation.text_input(Some(&id), iced::Rectangle::default(), &mut state);
                if let Outcome::Chain(next) = operation.finish() {
                    operation = next;
                } else {
                    break;
                }
            }
        }
        state
    }

    let mut app = ready();
    for message in [
        Message::Inspector(InspectorTab::Import),
        Message::Imports(import::Action::InputKind(InputKind::Name)),
    ] {
        let state = input_state(app.update(message)).await;
        assert!(state.is_focused());
        assert_eq!(
            state.cursor().selection(&Value::new(&app.tab.naming.name)),
            Some((0, "ethanol".len())),
        );
    }
    app.tab.naming.set_preview(Preview::new(
        record(),
        super::super::test_support::ethanol(),
        vec![],
        "CCO".into(),
    ));
    app.tab.naming.preview_open = true;
    app.tab.naming.notice = Some(Notice::new(NoticeKind::Info, "Preview ready"));
    app.tab.naming.import_alias = Some(0);
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let frames = app.tab.history.frames();

    let state = input_state(app.update(Message::Naming(Action::ClearName))).await;

    assert!(app.tab.naming.name.is_empty());
    assert!(app.tab.naming.preview.is_none());
    assert!(!app.tab.naming.preview_open);
    assert!(app.tab.naming.notice.is_none());
    assert!(app.tab.naming.import_alias.is_none());
    assert!(state.is_focused(), "Clear returns focus to the Name box");
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.history.frames(), frames);
}

#[tokio::test]
async fn one_insert_resolves_and_commits_structure_and_verified_caption_together()
-> Result<(), String> {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.insert_input();
    let pending = app.tab.naming.pending.unwrap();
    assert!(matches!(
        pending,
        Pending::Import {
            add_caption: true,
            ..
        }
    ));
    assert!(!app.tab.naming.preview_open);
    let _ = app.insert_input();
    assert_eq!(app.tab.naming.pending, Some(pending));
    let result = preview(&app, "CCO").await?;
    complete(&mut app, pending.ticket(), result);
    assert_eq!(naming::document_identity(&app.tab.doc)?.smiles, "CCO");
    assert_eq!(app.tab.doc.annotations.len(), 1);
    assert_eq!(app.tab.doc.annotations[0].text, "ethanol");
    assert_eq!(app.tab.doc.molecule_names.len(), 1);
    assert!(!app.tab.naming.preview_open);
    let inserted = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, inserted);
    let reopened = Document::from_native_file(&app.tab.doc.file_json()?)?;
    assert_eq!(reopened.molecule_names, app.tab.doc.molecule_names);
    assert_eq!(reopened.annotations, app.tab.doc.annotations);
    Ok(())
}

#[tokio::test]
async fn insert_without_caption_and_optional_preview_reuse_need_no_acknowledgement()
-> Result<(), String> {
    let mut app = ready();
    let _ = app.naming_action(Action::AddNameBelow(false));
    let mut result = preview(&app, "CCO").await?;
    result.warnings.push("Depiction layout information".into());
    app.tab.naming.set_preview(result);
    let _ = app.naming_action(Action::PreviewOpen(true));
    assert!(app.tab.naming.pending.is_none());
    let _ = app.insert_input();
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert!(app.tab.doc.annotations.is_empty());
    assert!(app.tab.doc.molecule_names.is_empty());
    Ok(())
}

#[tokio::test]
async fn exhausted_ids_report_failed_import_without_changing_the_drawing() -> Result<(), String> {
    let mut app = ready();
    let result = preview(&app, "CCO").await?;
    app.tab.doc.add_atom("O", Point::new(120., 80.));
    app.tab.doc.atoms[0].id = u64::MAX - 1;
    app.tab.doc.validate()?;
    let before = app.tab.doc.clone();
    let mut attempted = before.clone();
    assert!(editing::append(&mut attempted, &result.document, Point::default()).is_empty());
    assert_eq!(attempted, before);
    app.tab.selected = before.all_ids();
    app.tool = Tool::Text;
    app.tab.camera.center = Point::new(320., -80.);
    app.tab.camera.zoom = 2.;
    let selected = app.tab.selected.clone();
    let camera = app.tab.camera;
    let revision = app.tab.revision;
    let history = (
        app.tab.history.can_undo(),
        app.tab.history.can_redo(),
        app.tab.history.frames(),
    );
    app.tab.naming.set_preview(result);
    let notice = "Keep the preview details";
    app.tab.naming.notice = Some(Notice::new(NoticeKind::Info, notice));

    let _ = app.insert_input();

    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(
        (
            app.tab.history.can_undo(),
            app.tab.history.can_redo(),
            app.tab.history.frames(),
        ),
        history
    );
    assert!(app.error);
    assert_eq!(app.status, "Could not insert ethanol");
    assert_eq!(
        app.tab
            .naming
            .notice
            .as_ref()
            .map(|notice| notice.detail.as_str()),
        Some(notice)
    );
    assert_eq!(app.tool, Tool::Text);
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    Ok(())
}

#[tokio::test]
async fn ready_import_selects_the_inserted_objects_and_records_one_undo() -> Result<(), String> {
    let mut app = ready();
    let existing = app.tab.doc.add_atom("N", Point::new(-160., 40.));
    app.tab.selected = vec![existing];
    let before = app.tab.doc.clone();
    let result = preview(&app, "CCO").await?;
    app.tab.naming.set_preview(result);

    let _ = app.insert_input();

    let inserted = app
        .tab
        .doc
        .all_ids()
        .into_iter()
        .filter(|id| *id != existing)
        .collect::<Vec<_>>();
    assert_eq!(inserted.len(), 4);
    assert_eq!(app.tab.selected, inserted);
    assert_eq!(app.tab.history.undo_frames().count(), 1);
    assert_eq!(app.tab.history.frames(), 1);
    assert!(!app.error);
    assert_eq!(app.status, "Inserted ethanol · Undo removes it");
    assert!(app.tab.naming.notice.is_none());
    assert_eq!(app.tool, Tool::Select);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    Ok(())
}

#[tokio::test]
async fn edited_preview_never_inserts_original_name_or_unapplied_smiles() -> Result<(), String> {
    let mut app = ready();
    let result = preview(&app, "CCN").await?;
    assert!(result.modified);
    app.tab.naming.set_preview(result);
    let _ = app.naming_action(Action::Smiles("CCC".into()));
    let _ = app.insert_input();
    assert!(app.tab.doc.atoms.is_empty());
    assert!(!app.tab.history.can_undo());
    let _ = app.naming_action(Action::Smiles("CCN".into()));
    let _ = app.insert_input();
    assert_eq!(naming::document_identity(&app.tab.doc)?.smiles, "CCN");
    assert!(app.tab.doc.annotations.is_empty());
    assert!(app.tab.doc.molecule_names.is_empty());
    assert!(
        app.tab
            .naming
            .notice
            .as_ref()
            .map(|notice| notice.detail.as_str())
            .unwrap()
            .contains("omitted")
    );
    let notice = app.tab.naming.notice.as_ref().unwrap();
    assert_eq!(notice.kind, NoticeKind::Info);
    assert_eq!(
        notice.detail,
        "Edited structure inserted; original name omitted."
    );
    assert_eq!(notice.summary(), notice.detail);
    Ok(())
}

#[tokio::test]
async fn changed_alias_choice_keeps_retry_detail_and_summary() -> Result<(), String> {
    let mut app = ready();
    let result = preview(&app, "CCO").await?;
    app.tab.naming.set_preview(result);
    app.tab.naming.import_alias = Some(usize::MAX);
    let before = app.tab.doc.clone();

    app.insert_import_preview(true);

    let notice = app.tab.naming.notice.as_ref().unwrap();
    assert_eq!(notice.kind, NoticeKind::Retry);
    assert_eq!(
        notice.detail,
        "The alias choice changed. Choose its name again."
    );
    assert_eq!(notice.summary(), notice.detail);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    Ok(())
}

#[test]
fn finished_errors_keep_details_and_choose_dock_summaries_from_their_kind() {
    const PARSER_REJECTION: &str = "The OPSIN Rust port could not interpret this name unambiguously: unsupported optical rotation";
    const DAEMONIZING_LAUNCHER: &str = "Local worker exited without closing its response pipes; daemonizing launchers are unsupported";
    const UNSUPPORTED: &str =
        "Name not supported locally or ambiguous. Try a systematic name, or choose Look up online.";
    const SALT_DETAIL: &str = "Local naming supports one connected molecule; disconnected salts and mixtures are unsupported";
    const FAILED: &str = "Could not import this name. See Details.";
    for (error, kind, detail, summary) in [
        (
            naming::Error::Cancelled("Local name parsing was cancelled".into()),
            NoticeKind::Retry,
            "Local name parsing was cancelled",
            "Local name parsing was cancelled",
        ),
        (
            naming::Error::Cancelled("Local name generation was cancelled".into()),
            NoticeKind::Retry,
            "Local name generation was cancelled",
            "Local name generation was cancelled",
        ),
        (
            naming::Error::Rejected(PARSER_REJECTION.into()),
            NoticeKind::Unsupported,
            PARSER_REJECTION,
            UNSUPPORTED,
        ),
        (
            naming::Error::Rejected("The OPSIN Rust port returned no molecular structure".into()),
            NoticeKind::Unsupported,
            "The OPSIN Rust port returned no molecular structure",
            UNSUPPORTED,
        ),
        (
            naming::Error::Disconnected(SALT_DETAIL.into()),
            NoticeKind::Disconnected,
            SALT_DETAIL,
            SALT,
        ),
        (
            naming::Error::Worker("Could not start bounded local naming worker".into()),
            NoticeKind::Failed,
            "Could not start bounded local naming worker",
            FAILED,
        ),
        (
            naming::Error::Worker(DAEMONIZING_LAUNCHER.into()),
            NoticeKind::Failed,
            DAEMONIZING_LAUNCHER,
            FAILED,
        ),
    ] {
        let mut app = ready();
        let before = app.tab.doc.clone();
        let _ = app.insert_input();
        let ticket = app.tab.naming.pending.unwrap().ticket();

        let _ = app.naming_action(Action::Finished(ticket, Box::new(Err(error))));

        let notice = app.tab.naming.notice.as_ref().unwrap();
        assert_eq!(notice.kind, kind);
        assert_eq!(notice.detail, detail);
        assert_eq!(notice.summary(), summary);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

const SALT: &str = "Salts and mixtures aren't supported yet. Draw the ions, or paste SMILES under Import → Structure.";

/// The real parser path: salts get their own summary, while unknown names
/// keep the unsupported-or-ambiguous one.
#[tokio::test]
async fn actual_salt_names_explain_the_salt_limit_and_unknown_names_stay_ambiguous() {
    for (name, kind, summary) in [
        ("sodium chloride", NoticeKind::Disconnected, SALT),
        ("sodium acetate", NoticeKind::Disconnected, SALT),
        (
            "reshiki-no-such-chemical-name-50",
            NoticeKind::Unsupported,
            "Name not supported locally or ambiguous. Try a systematic name, or choose Look up online.",
        ),
    ] {
        let mut app = ready();
        let _ = app.naming_action(Action::Name(name.into()));
        let before = app.tab.doc.clone();
        let _ = app.insert_input();
        let ticket = app.tab.naming.pending.unwrap().ticket();
        let error = resolve_preview(app.engine.clone(), name.into(), naming::Cancel::default())
            .await
            .expect_err(name);
        let _ = app.naming_action(Action::Finished(ticket, Box::new(Err(error))));
        let notice = app.tab.naming.notice.as_ref().unwrap();
        assert_eq!(notice.kind, kind, "{name}");
        assert_eq!(notice.summary(), summary, "{name}");
        assert!(!notice.detail.is_empty(), "{name}");
        assert_eq!(app.tab.doc, before, "{name}");
        assert!(!app.tab.history.can_undo(), "{name}");
    }
}

/// The salt summary points to Import → Structure; it must accept the ions.
#[tokio::test]
async fn structure_import_inserts_the_salt_smiles_the_summary_suggests() -> Result<(), String> {
    let mut app = ready();
    let _ = app.import_action(import::Action::InputKind(InputKind::Structure));
    app.imports.set_text("[Na+].[Cl-]");
    let revision = app.tab.revision;
    let result = app
        .engine
        .request(crate::app::input_request(&app.imports.input.text()))
        .await;
    let _ = app.engine_done(revision, crate::app::Job::Insert, result);
    assert!(!app.error, "{}", app.status);
    let mut elements = app
        .tab
        .doc
        .atoms
        .iter()
        .map(|atom| atom.element.as_str())
        .collect::<Vec<_>>();
    elements.sort_unstable();
    assert_eq!(elements, ["Cl", "Na"]);
    assert!(app.tab.doc.bonds.is_empty());
    Ok(())
}

#[tokio::test]
async fn cancelled_edited_switched_or_stale_import_completions_never_commit() -> Result<(), String>
{
    for case in ["cancel", "name", "mode", "revision", "epoch"] {
        let mut app = ready();
        let result = preview(&app, "CCO").await?;
        let _ = app.insert_input();
        let ticket = app.tab.naming.pending.unwrap().ticket();
        match case {
            "cancel" => {
                let _ = app.naming_action(Action::Cancel);
            }
            "name" => {
                let _ = app.naming_action(Action::Name("propane".into()));
            }
            "mode" => {
                let _ = app.import_action(import::Action::InputKind(InputKind::Structure));
            }
            "revision" => app.tab.revision += 1,
            "epoch" => app.tab.file_epoch += 1,
            _ => unreachable!(),
        }
        let before = app.tab.doc.clone();
        complete(&mut app, ticket, result);
        assert_eq!(app.tab.doc, before, "{case}");
        assert!(!app.tab.history.can_undo(), "{case}");
        if case != "epoch" {
            assert!(app.tab.naming.pending.is_none(), "{case}");
            assert!(app.tab.naming.local_cancel.is_none(), "{case}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn completion_routes_only_to_originating_tab() -> Result<(), String> {
    use crate::app::tabs::{Action as Tabs, tests::Front};
    let mut app = ready();
    let id = app.tab.id;
    let result = preview(&app, "CCO").await?;
    let _ = app.insert_input();
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let front = Front::new(&mut app);
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Naming(Action::Finished(
            ticket,
            Box::new(Ok(Outcome::Names(vec![record()], Some(Box::new(result))))),
        ))),
    ));
    front.assert_unchanged(&app);
    let _ = app.update(Message::Tabs(Tabs::Select(id)));
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert_eq!(app.tab.doc.annotations[0].text, "ethanol");
    assert!(app.tab.history.can_undo());
    Ok(())
}

#[test]
fn busy_requests_and_unsupported_names_leave_drawing_and_history_untouched() {
    let mut app = ready();
    app.tab.busy = true;
    let _ = app.insert_input();
    assert!(app.tab.naming.pending.is_none());
    app.tab.busy = false;
    let before = app.tab.doc.clone();
    let _ = app.insert_input();
    let ticket = app.tab.naming.pending.unwrap().ticket();
    const DETAIL: &str = "The OPSIN Rust port could not interpret this name unambiguously: unsupported optical rotation";
    let detail = DETAIL;
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Err(naming::Error::Rejected(detail.into()))),
    ));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert_eq!(
        app.tab
            .naming
            .notice
            .as_ref()
            .map(|notice| notice.detail.as_str()),
        Some(detail)
    );
    assert_eq!(
        app.tab.naming.notice.as_ref().unwrap().summary(),
        "Name not supported locally or ambiguous. Try a systematic name, or choose Look up online."
    );
    assert_eq!(
        Notice::new(
            NoticeKind::Failed,
            "Could not start bounded local naming worker"
        )
        .summary(),
        "Could not import this name. See Details."
    );
}

#[tokio::test]
async fn actual_local_parser_result_inserts_directly_with_verified_source_title()
-> Result<(), String> {
    let mut app = ready();
    let _ = app.insert_input();
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let outcome = resolve_preview(
        app.engine.clone(),
        "ethanol".into(),
        naming::Cancel::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    let _ = app.naming_action(Action::Finished(ticket, Box::new(Ok(outcome))));
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    assert_eq!(naming::document_identity(&app.tab.doc)?.smiles, "CCO");
    assert_eq!(app.tab.doc.annotations[0].text, "ethanol");
    Ok(())
}

#[tokio::test]
async fn linked_name_survives_native_copy_and_ordinary_figure_and_editable_exports()
-> Result<(), String> {
    let mut app = ready();
    let result = preview(&app, "CCO").await?;
    app.tab.naming.set_preview(result);
    let _ = app.insert_input();
    let atoms = app
        .tab
        .doc
        .atoms
        .iter()
        .map(|atom| atom.id)
        .collect::<Vec<_>>();
    let copied = editing::selection(&app.tab.doc, &atoms);
    let restored = Document::from_native_file(&copied.file_json()?)?;
    assert_eq!(restored.molecule_names.len(), 1);
    assert_eq!(restored.annotations[0].text, "ethanol");
    for format in ["svg", "pdf"] {
        assert!(!reshiki::export::figure(&copied, format)?.bytes.is_empty());
    }
    let svg = String::from_utf8(reshiki::export::figure(&copied, "svg")?.bytes)
        .map_err(|error| error.to_string())?;
    assert!(svg.contains("ethanol"));
    for format in ["cdxml", "cdx"] {
        let mut request = Request::molecule("export", copied.clone());
        request.format = Some(format.into());
        let output = app
            .engine
            .request(request)
            .await?
            .output
            .ok_or("Missing editable export")?;
        let external = app
            .engine
            .request(Request::import(format, &output))
            .await?
            .document
            .ok_or("Missing editable import")?;
        assert!(
            external
                .annotations
                .iter()
                .any(|caption| caption.text == "ethanol"),
            "{format}"
        );
        assert!(external.molecule_names.is_empty());
        assert_eq!(naming::document_identity(&external)?.smiles, "CCO");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "Opt-in actual renderer compact Import dock accessibility and dispatch"]
async fn default_dock_exposes_one_insert_and_collapses_preview_and_details() -> Result<(), String> {
    use iced::advanced::renderer::Headless;
    use reshiki::accessibility::{Activate, Collect};
    let app = ready();
    let size = iced::Size::new(268., 400.);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut ui = iced_runtime::UserInterface::build(
        app.import_panel(),
        size,
        Default::default(),
        &mut renderer,
    );
    let mut collect = Collect::new(iced::Rectangle::with_size(size));
    ui.operate(
        &renderer,
        &mut iced::advanced::widget::operation::black_box(&mut collect),
    );
    let snapshot = collect.snapshot();
    assert!(snapshot.duplicate_ids.is_empty());
    for id in [
        "import-kind-structure",
        "import-kind-name",
        "import-name",
        "import-clear",
        "import-name-caption",
        "import-name-preview",
        "import-name-insert",
        "import-name-details",
        "naming.online-ask",
    ] {
        let node = snapshot
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("Missing {id}"));
        assert!(node.visible_bounds.is_some(), "Invisible {id}");
    }
    assert!(
        snapshot
            .nodes
            .iter()
            .all(|node| node.id != "import-name-smiles"
                && (!node.id.starts_with("naming.") || node.id == "naming.online-ask"))
    );
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .find(|node| node.id == "import-name-caption")
            .unwrap()
            .checked,
        Some(true)
    );
    let mut activate = Activate::<Message>::new("import-name-insert");
    ui.operate(
        &renderer,
        &mut iced::advanced::widget::operation::black_box(&mut activate),
    );
    assert!(matches!(activate.message(), Some(Message::InsertInput)));
    let mut activate = Activate::<Message>::new("import-clear");
    ui.operate(
        &renderer,
        &mut iced::advanced::widget::operation::black_box(&mut activate),
    );
    assert!(matches!(
        activate.message(),
        Some(Message::Naming(Action::ClearName))
    ));
    Ok(())
}

#[tokio::test]
async fn retained_plus_lactic_import_keeps_stereo_caption_and_atomic_history() -> Result<(), String>
{
    let mut app = ready();
    let original = app.tab.doc.clone();
    let name = "(+)-lactic acid";
    let _ = app.naming_action(Action::Name(name.into()));
    let _ = app.insert_input();
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let outcome = resolve_preview(app.engine.clone(), name.into(), naming::Cancel::default())
        .await
        .map_err(|error| error.to_string())?;
    let _ = app.naming_action(Action::Finished(ticket, Box::new(Ok(outcome))));
    assert!(!app.error);
    assert_eq!(app.tab.doc.atoms.len(), 6);
    assert_eq!(app.tab.doc.bonds.len(), 5);
    naming::verify_identity(
        "C[C@H](O)C(=O)O",
        &naming::document_identity(&app.tab.doc)?.smiles,
    )?;
    assert_eq!(app.tab.doc.annotations.len(), 1);
    assert_eq!(app.tab.doc.annotations[0].text, name);
    assert_eq!(app.tab.doc.molecule_names.len(), 1);
    let linked = app.tab.doc.molecule_names[0].clone();
    naming::verify_identity("C[C@H](O)C(=O)O", &linked.smiles)?;
    app.step_history(false);
    assert_eq!(app.tab.doc, original);
    app.step_history(true);
    assert_eq!(app.tab.doc.molecule_names, [linked]);
    naming::verify_identity(
        "C[C@H](O)C(=O)O",
        &naming::document_identity(&app.tab.doc)?.smiles,
    )?;
    let before = app.tab.doc.clone();
    let history = (app.tab.history.can_undo(), app.tab.history.can_redo());
    let unsupported = "(+)-butan-2-ol";
    let _ = app.naming_action(Action::Name(unsupported.into()));
    let _ = app.insert_input();
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let error = resolve_preview(
        app.engine.clone(),
        unsupported.into(),
        naming::Cancel::default(),
    )
    .await
    .expect_err("unrelated optical rotation must not assign R/S");
    let _ = app.naming_action(Action::Finished(ticket, Box::new(Err(error))));
    assert_eq!(app.tab.doc, before);
    assert_eq!(
        (app.tab.history.can_undo(), app.tab.history.can_redo()),
        history
    );
    Ok(())
}
