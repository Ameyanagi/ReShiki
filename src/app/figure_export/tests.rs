use super::*;

#[test]
fn browser_exports_ask_only_for_untitled_names_and_keep_the_chosen_name() {
    use crate::app::prompt::{Answer, Prompt};

    let (mut app, _) = App::new();
    app.tab.busy = false;
    let id = app.tab.id;
    for (kind, format, extension) in [
        (Kind::Figure, "png", "png"),
        (Kind::Figure, "svg-office", "svg"),
        (Kind::Figure, "pdf", "pdf"),
        (Kind::Structure, "mol", "mol"),
        (Kind::Nmr, "tsv", "tsv"),
    ] {
        let export = Prepared::new(vec![1, 2, 3], format, None, vec![], kind);
        let _ = app.export_ready(export.clone(), false);
        assert!(app.prompt.is_none(), "Desktop keeps the save dialog");

        let _ = app.export_ready(export.clone(), true);
        assert!(matches!(
            &app.prompt,
            Some(Prompt::Download { name, .. }) if name == &format!("Molecule.{extension}")
        ));
        let chosen = format!("paracetamol.{extension}");
        let _ = app.update(Message::DownloadName(chosen.clone()));
        let Message::Tab(tab, message) = app.prompt.take().unwrap().answer(Answer::Save) else {
            panic!("An export answer keeps its originating tab");
        };
        assert_eq!(tab, id);
        let Message::SaveExport(name, pending) = *message else {
            panic!("Confirming starts the prepared export");
        };
        assert_eq!(name, chosen);
        let _ = app.update(pending.completed(Ok(Some(PathBuf::from(name)))));
        assert!(app.status.contains(&chosen));

        let _ = app.export_ready(export, true);
        let task = app.update(Message::Prompt(Answer::Cancel));
        assert!(app.prompt.is_none());
        assert_eq!(task.units(), 0, "Cancel starts no download");
        assert!(!app.figure_exporting);
    }

    app.tab.path = Some("caffeine.rsk".into());
    let named = Prepared::new(vec![], "png", app.export_stem(), vec![], Kind::Figure);
    assert_eq!(named.name, "caffeine.png");
    let _ = app.export_ready(named, true);
    assert!(app.prompt.is_none(), "Named drawings download directly");
}

#[test]
fn switched_and_closed_tabs_keep_export_completions_off_the_front_tab() {
    for closed in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let id = app.tab.id;
        app.figure_exporting = true;
        if closed {
            let _ = app.close_active_tab();
        }
        let front = super::super::tabs::tests::Front::new(&mut app);
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::FigureExported(Err("Export failed".into()))),
        ));
        assert!(!app.figure_exporting);
        front.assert_unchanged(&app);
        if !closed {
            assert_eq!(app.tabs.background[0].status, "Export failed");
            assert!(app.tabs.background[0].error);
        }
    }
}

#[test]
fn office_svg_uses_figure_snapshot_without_changing_selection_or_document() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Default::default());
    app.tab.selected = vec![a];
    let snapshot = app.tab.doc.clone();
    let _task = app.update(Message::Export("svg-office"));
    assert!(app.figure_exporting);
    assert!(!app.tab.busy);
    assert_eq!(app.status, "Preparing SVG for Office export…");
    assert_eq!(app.tab.doc, snapshot);
    assert_eq!(app.tab.selected, vec![a]);
}

#[tokio::test]
async fn svg_export_preferences_control_live_text_and_payload_independently() {
    use iced::futures::StreamExt;
    use reshiki::document::{Annotation, Document};

    let (mut app, _) = App::new();
    app.engine = reshiki::engine::LocalEngine::in_process();
    app.tab.busy = false;
    app.tab.path = Some("figure.rsk".into());
    app.tab.doc = Document::default();
    app.tab.doc.annotations.push(Annotation {
        id: 1,
        position: Default::default(),
        text: "Editable figure text".into(),
        format: Default::default(),
    });
    app.tab.selected = vec![1];
    let snapshot = app.tab.doc.clone();
    for live_text in [false, true] {
        let _ = app.update(Message::View(
            super::super::view_settings::Action::ExportSvgLiveText(live_text),
        ));
        for editable in [false, true] {
            let _ = app.update(Message::View(
                super::super::view_settings::Action::ExportEditableDrawing(editable),
            ));
            for format in ["svg", "svg-office"] {
                let mut stream = iced_runtime::task::into_stream(app.prepare_figure(
                    format,
                    false,
                    app.appearance.export_editable_drawing,
                ))
                .expect("SVG prepares asynchronously");
                let Some(iced_runtime::Action::Output(result)) = stream.next().await else {
                    panic!("Missing prepared SVG");
                };
                let prepared = result.unwrap();
                assert_eq!(prepared.format, format);
                assert_eq!(prepared.name, "figure.svg");
                let svg = std::str::from_utf8(&prepared.bytes).unwrap();
                let xml = roxmltree::Document::parse(svg).unwrap();
                assert_eq!(
                    xml.descendants().any(|node| node.has_tag_name("text")),
                    format == "svg" && live_text,
                    "Format: {format}; keep text editable: {live_text}; include drawing: {editable}"
                );
                if editable {
                    assert_eq!(
                        reshiki::payload::read(&prepared.bytes).unwrap().unwrap(),
                        reshiki::payload::visible(&snapshot.current())
                    );
                    assert!(
                        prepared
                            .details
                            .iter()
                            .any(|detail| detail == "Includes the editable drawing")
                    );
                } else {
                    assert!(reshiki::payload::read(&prepared.bytes).is_none());
                }
                assert_eq!(app.tab.doc, snapshot);
                assert_eq!(app.tab.selected, [1]);
            }
        }
    }
}

#[test]
fn svg_export_keeps_its_name_and_figure_route_with_either_text_preference() {
    for live_text in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.appearance.export_svg_live_text = live_text;
        let atom = app.tab.doc.add_atom("O", Default::default());
        app.tab.selected = vec![atom];
        let snapshot = app.tab.doc.clone();
        let _ = app.update(Message::Export("svg"));
        assert!(app.figure_exporting);
        assert!(!app.tab.busy);
        assert_eq!(app.status, "Preparing SVG export…");
        assert_eq!(app.tab.doc, snapshot);
        assert_eq!(app.tab.selected, [atom]);
    }
}

#[cfg(windows)]
#[test]
fn emf_uses_figure_snapshot_without_changing_selection_or_document() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Default::default());
    app.tab.selected = vec![a];
    let snapshot = app.tab.doc.clone();
    let _task = app.update(Message::Export("emf"));
    assert!(app.figure_exporting);
    assert!(!app.tab.busy);
    assert_eq!(app.status, "Preparing EMF export…");
    assert_eq!(app.tab.doc, snapshot);
    assert_eq!(app.tab.selected, vec![a]);
}

#[test]
fn exporting_a_snapshot_blocks_duplicate_exports_and_always_resets() {
    let (mut app, _) = App::new();
    let snapshot = app.tab.doc.clone();
    let _task = app.update(Message::Export("png"));
    assert!(app.figure_exporting);
    assert!(!app.error);
    assert!(app.update(Message::Export("pdf")).units() == 0);
    let _ = app.update(Message::FigureExported(Err("Disk is full".into())));
    assert!(!app.figure_exporting);
    assert!(app.error);
    assert_eq!(app.status, "Disk is full");
    app.figure_exporting = true;
    let _ = app.update(Message::FigureExported(Ok(None)));
    assert!(!app.figure_exporting);
    assert!(!app.error);
    let _ = app.update(Message::FigureExported(Ok(Some(Saved {
        path: PathBuf::from("gallery.png"),
        recent: None,
        details: vec!["PNG: 6627 × 5285 pixels at 300 dpi".into()],
    }))));
    assert!(app.status.contains("300 dpi"));
    assert_eq!(app.tab.doc, snapshot);
    assert!(super::super::atom_text::background(
        &Message::FigureExported(Ok(None))
    ));
}

#[test]
fn canceling_a_structure_export_finishes_with_a_non_error_receipt() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::Export("mol"));
    app.error = true;
    let _ = app.update(Message::Exported(Ok(None)));
    assert_eq!(app.status, "Export canceled");
    assert!(!app.error);
}

#[test]
fn export_format_labels_cover_figure_and_structure_formats() {
    for (format, expected) in [
        ("png", "PNG"),
        ("svg", "SVG"),
        ("svg-office", "SVG for Office"),
        ("pdf", "PDF"),
        ("mol", "MOL"),
        ("cdxml", "CDXML"),
    ] {
        assert_eq!(super::super::export_format_label(format), expected);
    }
}

#[test]
fn export_receipts_distinguish_browser_handoff_from_desktop_write() {
    let path = std::path::Path::new("/exports/aspirin.pdf");
    let details = vec!["PDF: 2 pages".into()];
    assert_eq!(
        super::super::files::export_status(path, &details, false),
        "Exported aspirin.pdf · PDF: 2 pages"
    );
    assert_eq!(
        super::super::files::export_status(path, &details, true),
        "Downloaded aspirin.pdf · PDF: 2 pages"
    );
}

async fn prepared(app: &App, format: &'static str, editable: bool) -> Prepared {
    use iced::futures::StreamExt;
    let mut stream =
        iced_runtime::task::into_stream(app.prepare_figure(format, false, editable)).unwrap();
    let Some(iced_runtime::Action::Output(result)) = stream.next().await else {
        panic!("Missing figure");
    };
    result.unwrap()
}

fn labelled_app() -> App {
    let (mut app, _) = App::new();
    app.engine = reshiki::engine::LocalEngine::in_process();
    app.tab.busy = false;
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id: 1,
        position: Default::default(),
        text: "First figure".into(),
        format: Default::default(),
    });
    app
}

#[tokio::test]
async fn export_again_renders_current_drawing_with_saved_options_to_exact_destination() {
    use iced::futures::StreamExt;
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("chosen figure.png");
    let mut app = labelled_app();
    app.tab.path = Some(directory.path().join("drawing.rsk"));
    app.appearance.export_background = crate::appearance::Background::Custom;
    app.appearance.custom_background = [29, 56, 87];
    app.appearance.export_dpi = crate::appearance::Dpi::try_from(300).unwrap();
    let first = prepared(&app, "png", false).await;
    std::fs::write(&destination, &first.bytes).unwrap();
    let _ = app.update(first.completed(Ok(Some(destination.clone()))));
    assert_eq!(
        app.repeat_export().unwrap().label(),
        "Export again: chosen figure.png (PNG · up to 300 dpi)"
    );

    // New preferences must not silently change the repeated export's recipe.
    app.appearance.export_background = crate::appearance::Background::White;
    app.appearance.custom_background = [255, 0, 0];
    app.appearance.export_dpi = crate::appearance::Dpi::try_from(1200).unwrap();
    app.appearance.export_editable_drawing = true;
    app.tab.doc.annotations[0].text = "A longer, revised figure".into();
    let drawing = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let original = std::fs::read(&destination).unwrap();
    let mut stream = iced_runtime::task::into_stream(app.update(Message::ExportAgain)).unwrap();
    assert!(app.figure_exporting);
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
            let _ = app.update(message);
        }
    }
    assert!(!app.figure_exporting);
    assert!(!app.error, "{}", app.status);
    let bytes = std::fs::read(&destination).unwrap();
    assert_ne!(
        bytes, original,
        "Repeat renders the edited drawing, not old bytes"
    );
    assert!(
        reshiki::payload::read(&bytes).is_none(),
        "Saved editable choice wins"
    );
    let image = image::load_from_memory(&bytes).unwrap().into_rgba8();
    assert_eq!(image.get_pixel(0, 0).0, [29, 56, 87, 255]);
    assert!(app.status.contains("300 dpi"), "{}", app.status);
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.selected, selection);
    assert!(!app.tab.history.can_undo());
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        1,
        "No new filename or document save"
    );
}

#[tokio::test]
async fn recent_export_survives_restart_but_does_not_target_a_different_drawing() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = labelled_app();
    let source = directory.path().join("source.rsk");
    app.tab.path = Some(source.clone());
    let export = prepared(&app, "svg", false).await;
    let _ = app.update(export.completed(Ok(Some(directory.path().join("custom.svg")))));
    let record = directory.path().join("recent-export.json");
    app.recent_export
        .as_ref()
        .unwrap()
        .save_to(&record)
        .unwrap();
    let loaded = Recent::load_from(&record).unwrap();
    assert!(
        loaded.recipe.origin.is_none(),
        "Session tab ids are never persisted"
    );
    app.recent_export = Some(loaded);
    assert!(app.repeat_export().is_some());
    app.tab.path = Some(directory.path().join("other.rsk"));
    assert!(app.repeat_export().is_none());
    assert_eq!(app.update(Message::ExportAgain).units(), 0);
    let normal = prepared(&app, "pdf", true).await;
    assert_eq!(
        normal.name, "other.pdf",
        "Normal Export retains its own filename"
    );
    assert_eq!(normal.directory.as_deref(), Some(directory.path()));
    app.tab.path = Some(source);
    assert!(app.repeat_export().is_some());
}

#[tokio::test]
async fn cancellation_failure_and_untitled_replacement_preserve_recent_export_safely() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = labelled_app();
    let previous = directory.path().join("previous.pdf");
    let export = prepared(&app, "pdf", true).await;
    let _ = app.update(export.completed(Ok(Some(previous.clone()))));
    assert!(app.repeat_export().is_some());
    for result in [Ok(None), Err("Disk full".into())] {
        let attempted = prepared(&app, "png", false).await;
        let _ = app.update(attempted.completed(result));
        assert_eq!(app.recent_export.as_ref().unwrap().path, previous);
    }
    let record = directory.path().join("recent-export.json");
    app.recent_export
        .as_ref()
        .unwrap()
        .save_to(&record)
        .unwrap();
    app.recent_export = Recent::load_from(&record);
    assert!(
        app.repeat_export().is_none(),
        "An old untitled file never binds to a new tab after restart"
    );
    // A replacement document can reuse the tab id, but not its file epoch.
    let export = prepared(&app, "pdf", true).await;
    let _ = app.update(export.completed(Ok(Some(previous))));
    assert!(app.repeat_export().is_some());
    app.tab.file_epoch += 1;
    assert!(app.repeat_export().is_none());
}

#[tokio::test]
async fn repeat_svg_keeps_saved_text_and_payload_choices_and_failed_write_is_retryable() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = labelled_app();
    app.appearance.export_svg_live_text = true;
    let export = prepared(&app, "svg", false).await;
    let _ = app.update(export.completed(Ok(Some(directory.path().join("missing/out.svg")))));
    app.appearance.export_svg_live_text = false;
    let recent = app.repeat_export().unwrap().clone();
    let result = prepared_recipe(&app, recent.recipe.clone()).await;
    let svg = roxmltree::Document::parse(std::str::from_utf8(&result.bytes).unwrap()).unwrap();
    assert!(svg.descendants().any(|node| node.has_tag_name("text")));
    assert!(reshiki::payload::read(&result.bytes).is_none());
    use iced::futures::StreamExt;
    let mut stream = iced_runtime::task::into_stream(app.update(Message::ExportAgain)).unwrap();
    while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
            let _ = app.update(message);
        }
    }
    assert!(app.error);
    assert!(!app.figure_exporting);
    assert_eq!(app.repeat_export().unwrap().path, recent.path);
}

async fn prepared_recipe(app: &App, recipe: Recipe) -> Prepared {
    use iced::futures::StreamExt;
    let mut stream = iced_runtime::task::into_stream(app.prepare_recipe(recipe)).unwrap();
    let Some(iced_runtime::Action::Output(result)) = stream.next().await else {
        panic!("Missing figure");
    };
    result.unwrap()
}

#[tokio::test]
async fn browser_repeat_never_identifies_files_by_their_shared_basename() {
    let mut app = labelled_app();
    app.tab.path = Some("drawing.rsk".into());
    let export = prepared(&app, "svg", false).await;
    let recipe = export.recipe.unwrap();
    assert!(recipe.belongs_to_platform(&app.tab, true));
    // Another dropped drawing can have the same browser name, including in
    // the same reused tab. Only the actual session identity may match.
    app.tab.file_epoch += 1;
    assert!(!recipe.belongs_to_platform(&app.tab, true));
    app.tab.id = super::super::document_tab::TabId(app.tab.id.0 + 1);
    assert!(!recipe.belongs_to_platform(&app.tab, true));
    let mut reopened: Recipe =
        serde_json::from_slice(&serde_json::to_vec(&recipe).unwrap()).unwrap();
    assert!(reopened.origin.is_none());
    assert!(
        !reopened.belongs_to_platform(&app.tab, true),
        "A bare filename cannot restore browser identity after reload"
    );
    reopened.source = None;
    assert!(!reopened.belongs_to_platform(&app.tab, false));
}

#[tokio::test]
async fn switched_or_closed_export_completion_remembers_the_destination_without_touching_front_tab()
{
    let directory = tempfile::tempdir().unwrap();
    for closed in [false, true] {
        let mut app = labelled_app();
        app.tab.path = Some(directory.path().join("source.rsk"));
        app.tab.mark_saved(app.tab.doc.clone());
        let id = app.tab.id;
        let export = prepared(&app, "pdf", false).await;
        let receipt = export.completed(Ok(Some(directory.path().join("chosen.pdf"))));
        if closed {
            let _ = app.close_active_tab();
        } else {
            app.add_tab();
        }
        let drawing = app.tab.doc.clone();
        let status = app.status.clone();
        let _ = app.update(Message::Tab(id, Box::new(receipt)));
        assert!(app.recent_export.is_some());
        assert!(app.repeat_export().is_none());
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.status, status);
    }
}

#[tokio::test]
async fn native_relative_source_names_do_not_alias_across_working_directories() {
    let directory = tempfile::tempdir().unwrap();
    let a = directory.path().join("a");
    let b = directory.path().join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(a.join("drawing.rsk"), b"first").unwrap();
    std::fs::write(b.join("drawing.rsk"), b"second").unwrap();
    let first = source_identity_in(Path::new("drawing.rsk"), &a).unwrap();
    let second = source_identity_in(Path::new("drawing.rsk"), &b).unwrap();
    assert_ne!(first, second);
    assert!(first.is_absolute() && second.is_absolute());
    let mut app = labelled_app();
    app.tab.path = Some(first.clone());
    let export = prepared(&app, "pdf", false).await;
    let _ = app.update(export.completed(Ok(Some(a.join("custom.pdf")))));
    app.recent_export.as_mut().unwrap().recipe.origin = None;
    assert!(app.repeat_export().is_some());
    app.tab.path = Some(second);
    assert!(app.repeat_export().is_none());

    // Loading an old relative source cannot resolve its historical directory.
    app.recent_export.as_mut().unwrap().recipe.source = Some("drawing.rsk".into());
    let record = directory.path().join("recent.json");
    app.recent_export
        .as_ref()
        .unwrap()
        .save_to(&record)
        .unwrap();
    let loaded = Recent::load_from(&record).unwrap();
    assert!(loaded.recipe.source.is_none());
    assert_eq!(
        loaded.path,
        a.join("custom.pdf"),
        "The remembered folder remains usable"
    );
    app.recent_export = Some(loaded);
    app.tab.path = Some("drawing.rsk".into());
    assert!(app.repeat_export().is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn naming_or_renaming_the_same_drawing_keeps_its_repeat_target_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = labelled_app();
    let destination = directory.path().join("figure.svg");
    let export = prepared(&app, "svg", false).await;
    let _ = app.update(export.completed(Ok(Some(destination.clone()))));
    for name in ["first.rsk", "renamed.rsk"] {
        // The normal save completion, including the first name of an untitled
        // drawing, must move only its source binding and keep the export file.
        let source = directory.path().join(name);
        std::fs::write(&source, app.tab.doc.file_json().unwrap()).unwrap();
        let snapshot = app.tab.doc.clone();
        let _ = app.update(Message::Saved(
            app.tab.file_epoch,
            Box::new(snapshot),
            Ok(Some(source)),
        ));
        let record = directory.path().join("recent.json");
        app.recent_export
            .as_ref()
            .unwrap()
            .save_to(&record)
            .unwrap();
        app.recent_export = Recent::load_from(&record);
        assert!(
            app.repeat_export().is_some(),
            "Naming {name} retains restart identity"
        );
        assert_eq!(app.repeat_export().unwrap().path, destination);
    }
    // A save for a different front drawing cannot rebind the remembered file.
    app.add_tab();
    let source_before = app.recent_export.as_ref().unwrap().recipe.source.clone();
    let _ = app.update(Message::Saved(
        app.tab.file_epoch,
        Box::new(app.tab.doc.clone()),
        Ok(Some(directory.path().join("other.rsk"))),
    ));
    assert_eq!(
        app.recent_export.as_ref().unwrap().recipe.source,
        source_before
    );
    assert!(app.repeat_export().is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn saving_while_picture_preparation_is_pending_keeps_the_completed_source_binding() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = labelled_app();
    let export = prepared(&app, "svg", false).await;
    let source = directory.path().join("named-during-export.rsk");
    std::fs::write(&source, app.tab.doc.file_json().unwrap()).unwrap();
    let snapshot = app.tab.doc.clone();
    let _ = app.update(Message::Saved(
        app.tab.file_epoch,
        Box::new(snapshot),
        Ok(Some(source)),
    ));
    let _ = app.update(export.completed(Ok(Some(directory.path().join("figure.svg")))));
    let record = directory.path().join("recent.json");
    app.recent_export
        .as_ref()
        .unwrap()
        .save_to(&record)
        .unwrap();
    app.recent_export = Recent::load_from(&record);
    assert!(app.repeat_export().is_some());
}
