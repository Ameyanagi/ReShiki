use super::*;

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
    assert_eq!(app.status, "Preparing SVG · Office picture export…");
    assert_eq!(app.tab.doc, snapshot);
    assert_eq!(app.tab.selected, vec![a]);
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
