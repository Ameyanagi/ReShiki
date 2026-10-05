use super::*;
use crate::app::Job;
use reshiki::document::Document;

fn copy_key(app: &App) -> CopyAsKey {
    CopyAsKey {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        selected: app.tab.selected.clone(),
        format: CopyFormat::Smiles,
    }
}

#[test]
fn copy_as_drops_stale_selection_revision_and_epoch_before_publication() {
    for stale in ["selection", "revision", "epoch"] {
        let (mut app, _) = App::new();
        let atom = app.tab.doc.add_atom("O", Point::default());
        app.tab.selected = vec![atom];
        let key = copy_key(&app);
        app.copy_as_busy = true;
        app.tab.clipboard_busy = true;
        match stale {
            "selection" => app.tab.selected.clear(),
            "revision" => app.tab.revision += 1,
            _ => app.tab.file_epoch += 1,
        }
        let before = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        app.status = "Current drawing".into();
        let task = app.copy_as_prepared(key, Err("old conversion failed".into()));
        assert_eq!(task.units(), 0);
        assert!(!app.copy_as_busy && !app.tab.clipboard_busy);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, selected);
        assert!(!app.tab.history.can_undo());
        if stale == "epoch" {
            assert_eq!(app.status, "Current drawing");
        } else {
            assert!(app.status.contains("Clipboard unchanged"));
        }
    }
}

#[test]
fn copy_as_conversion_failure_is_nonmutating_and_retains_the_error() {
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("O", Point::default());
    let before = app.tab.doc.clone();
    let key = copy_key(&app);
    app.copy_as_busy = true;
    app.tab.clipboard_busy = true;
    assert_eq!(
        app.copy_as_prepared(key, Err("Unsupported bond".into()))
            .units(),
        0
    );
    assert_eq!(app.tab.doc, before);
    assert!(app.error && app.status.contains("Unsupported bond"));
    assert!(app.status.contains("Clipboard unchanged"));
    assert!(!app.copy_as_busy && !app.tab.clipboard_busy);
}

#[test]
fn copy_as_work_and_receipts_stay_with_the_originating_tab() {
    use crate::app::tabs::tests::Front;
    for closed in [false, true] {
        for preparing in [false, true] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.doc.add_atom("O", Point::default());
            app.tab.saved = app.tab.doc.clone();
            let id = app.tab.id;
            let key = copy_key(&app);
            app.copy_as_busy = true;
            app.tab.clipboard_busy = true;
            if closed {
                let _ = app.close_active_tab();
            }
            let front = Front::new(&mut app);
            assert!(app.clipboard_working());
            let message = if preparing {
                Message::CopyAsPrepared(key, Box::new(Err("No structure output".into())))
            } else {
                Message::CopyAsWritten(key, Ok(vec!["Retained format warning".into()]))
            };
            let _ = app.update(Message::Tab(id, Box::new(message)));
            front.assert_unchanged(&app);
            assert!(!app.copy_as_busy);
            if !closed {
                let source = app.tabs.background.iter().find(|tab| tab.id == id).unwrap();
                assert!(!source.clipboard_busy);
                assert_eq!(source.doc.atoms.len(), 1);
                assert!(source.status.contains(if preparing {
                    "No structure output"
                } else {
                    "Retained format warning"
                }));
            }
        }
    }
}

#[test]
fn copy_as_uses_one_snapshot_without_changing_selection_or_undo() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let carbon = app.tab.doc.add_atom("C", Point::default());
    app.tab.doc.add_atom("O", Point::new(240., 0.));
    app.tab.selected = vec![carbon];
    let before = app.tab.doc.clone();
    let task = app.copy_as(CopyFormat::Smiles);
    assert!(task.units() > 0);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, [carbon]);
    assert!(!app.tab.history.can_undo());
    assert!(app.copy_as_busy && app.tab.clipboard_busy);
    assert_eq!(app.copy_as(CopyFormat::Mol).units(), 0);
}

#[test]
fn an_ordinary_copy_outliving_its_tab_cannot_overtake_a_new_copy_as() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let atom = app.tab.doc.add_atom("O", Point::default());
    app.tab.selected = vec![atom];
    app.tab.saved = app.tab.doc.clone();
    let (id, epoch, revision) = (app.tab.id, app.tab.file_epoch, app.tab.revision);
    let _task = app.copy_native(false, false);
    assert!(app.native_copy_busy);
    let _ = app.close_active_tab();
    app.tab.doc.add_atom("N", Point::default());
    assert_eq!(app.copy_as(CopyFormat::Smiles).units(), 0);
    assert!(!app.copy_as_busy);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::ClipboardWritten {
            epoch,
            revision,
            cut_ids: vec![],
            result: success(),
        }),
    ));
    assert!(!app.native_copy_busy);
    assert_eq!(app.tab.doc, before);
    assert!(app.copy_as(CopyFormat::Smiles).units() > 0);
}
use reshiki::engine::Response;

fn success() -> Result<CopyOutcome, String> {
    Ok(CopyOutcome {
        external_editable: true,
        image_only: false,
        chemical_format: None,
        notices: vec![],
    })
}

#[test]
fn background_clipboard_reads_and_cuts_keep_the_front_unchanged() {
    use crate::app::tabs::{Action, tests::Front};
    for cut in [false, true] {
        for stale in [false, true] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.doc.add_atom("N", Point::default());
            let (id, epoch, revision) = (app.tab.id, app.tab.file_epoch, app.tab.revision);
            let cut_ids = app.tab.doc.all_ids();
            app.tab.clipboard_busy = true;
            if stale {
                let before = app.tab.doc.clone();
                app.tab.doc.add_atom("C", Point::new(42., 0.));
                app.changed(before);
            }
            let before = app.tab.doc.clone();
            let front = Front::new(&mut app);
            let mut part = Document::default();
            part.add_atom("O", Point::default());
            let message = if cut {
                Message::ClipboardWritten {
                    epoch,
                    revision,
                    cut_ids,
                    result: success(),
                }
            } else {
                Message::ClipboardRead {
                    epoch,
                    revision,
                    result: Box::new(Ok(part.into())),
                }
            };
            let _ = app.update(Message::Tab(id, Box::new(message)));
            front.assert_unchanged(&app);
            assert!(!app.tabs.background[0].clipboard_busy);
            let _ = app.update(Message::Tabs(Action::Select(id)));
            if stale {
                assert_eq!(app.tab.doc, before);
                assert!(app.status.contains("changed"));
            } else {
                assert_ne!(app.tab.doc, before);
                assert!(app.status.contains(if cut { "cut" } else { "pasted" }));
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, before);
                assert!(!app.tab.history.can_undo());
            }
        }
    }
}

#[test]
fn successful_picture_fallback_has_a_concise_status_and_retains_details() {
    let (mut app, _) = App::new();
    app.clipboard_written(
        app.tab.file_epoch,
        app.tab.revision,
        vec![],
        Ok(CopyOutcome {
            external_editable: false,
            image_only: false,
            chemical_format: None,
            notices: vec!["Unsupported projected wedge style".into()],
        }),
    );
    assert!(!app.error);
    assert!(
        app.status
            .lines()
            .next()
            .is_some_and(|s| s.len() < 110 && s.contains("Copied"))
    );
    assert!(app.status.contains("Unsupported projected wedge style"));
}

#[test]
fn cut_waits_for_success_and_refuses_stale_completion() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let before = app.tab.doc.clone();
    let (epoch, revision) = (app.tab.file_epoch, app.tab.revision);
    app.clipboard_written(epoch, revision, vec![a, b], Err("Write failed".into()));
    assert_eq!(app.tab.doc, before);
    app.clipboard_written(epoch.wrapping_add(1), revision, vec![a, b], success());
    assert_eq!(app.tab.doc, before);
    app.clipboard_written(epoch, revision.wrapping_add(1), vec![a, b], success());
    assert_eq!(app.tab.doc, before);
    // A changed selection must not change the pending Cut's snapshot.
    app.tab.selected = vec![a];
    app.clipboard_written(epoch, revision, vec![b], success());
    assert!(app.tab.doc.atom(a).is_some());
    assert!(app.tab.doc.atom(b).is_none());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn paste_is_atomic_and_stale_data_does_not_replace_newer_work() {
    let (mut app, _) = App::new();
    let mut part = Document::default();
    let a = part.add_atom("N", Point::default());
    let b = part.add_atom("C", Point::new(42., 0.));
    part.add_bond(a, b, 1, "plain");
    let before = app.tab.doc.clone();
    app.clipboard_read(
        app.tab.file_epoch,
        app.tab.revision.wrapping_add(1),
        Ok(part.clone().into()),
    );
    assert_eq!(app.tab.doc, before);
    app.clipboard_read(app.tab.file_epoch, app.tab.revision, Ok(part.into()));
    assert_eq!(app.tab.doc.atoms.len(), before.atoms.len() + 2);
    assert_eq!(app.tab.selected.len(), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn reshiki_pastes_keep_palette_colors_and_other_sources_stay_exact() {
    use reshiki::palette::{Color, Hue, Row};
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.bonds[0].color = Color::Palette(Hue::Teal, Row::Strong);
    let part = editing::selection(&app.tab.doc, &[a, b]);
    app.clipboard_read(
        app.tab.file_epoch,
        app.tab.revision,
        Ok(PasteOutcome::native(part.clone())),
    );
    assert_eq!(app.tab.doc.bonds[1].color, app.tab.doc.bonds[0].color);
    assert!(
        app.tab.doc.atoms[2..]
            .iter()
            .all(|a| !a.display.color_override)
    );
    // The same drawing from ChemDraw keeps its source appearance.
    app.tab.doc.canvas_theme = reshiki::canvas_theme::CanvasTheme::Dark;
    let teal = reshiki::palette::Palette::of(&part).rgb(part.bonds[0].color);
    app.clipboard_read(
        app.tab.file_epoch,
        app.tab.revision,
        Ok(reshiki::canvas_theme::resolved_document(&part)
            .into_owned()
            .into()),
    );
    assert_eq!(app.tab.doc.bonds[2].color, Color::Custom(teal));
    assert!(
        app.tab.doc.atoms[4..]
            .iter()
            .all(|a| a.display.color_override)
    );
}

#[test]
fn external_metadata_warnings_remain_visible_after_paste_and_file_import() {
    let warning = "External reaction roles and condition references are not retained";
    let mut doc = Document::default();
    doc.add_atom("O", Point::default());
    let (mut app, _) = App::new();
    app.clipboard_read(
        app.tab.file_epoch,
        app.tab.revision,
        Ok(PasteOutcome {
            document: doc.clone(),
            warnings: vec![warning.into()],
            native: false,
        }),
    );
    assert!(app.status.starts_with("Editable drawing pasted"));
    assert!(app.status.contains(warning));
    assert!(!app.error);
    for kind in [Job::Import, Job::ImportFile, Job::Insert] {
        let _ = app.update(Message::EngineDone {
            revision: app.tab.revision,
            kind,
            result: Box::new(Ok(Response {
                document: Some(doc.clone()),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![warning.into()],
            })),
        });
        assert!(app.status.contains(warning));
        assert!(!app.error);
    }
}
