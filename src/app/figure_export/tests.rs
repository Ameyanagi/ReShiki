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
