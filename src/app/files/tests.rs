use super::*;
use crate::app::{App, Message, Pending, inline_text};
use crate::session::Binding;
use reshiki::document::{Document, Point};
use rfd::MessageDialogResult as Answer;

#[test]
fn each_office_host_selects_its_save_backend() {
    for (host, expected) in [
        (None, SaveBackend::Atomic),
        (Some(Host::Office), SaveBackend::OfficeOle),
        (Some(Host::LibreOffice), SaveBackend::LibreOfficeRetry),
        (Some(Host::Microsoft365), SaveBackend::Addin),
    ] {
        assert_eq!(save_backend(host), expected);
    }
}

fn prepared() -> Opened {
    let mut document = Document::default();
    document.add_atom("O", Point::default());
    Some((
        "oxygen.rsk".into(),
        Ok(Prepared::Native(Box::new(document))),
    ))
}

fn png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(3, 2)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

#[tokio::test]
async fn editable_png_opens_as_an_untitled_drawing_with_native_save_target() -> Result<(), String> {
    let mut drawing = Document::default();
    drawing.add_atom("O", Point::default());
    let contents = reshiki::payload::embed("png", png(), &reshiki::payload::encode(&drawing)?)?;
    let path = PathBuf::from("images/oxygen.PNG");
    let opened = prepare_contents(path.clone(), Ok(contents.clone())).await;
    assert!(matches!(&opened, Some((_, Ok(Prepared::Picture(_))))));
    let (mut app, _) = App::new();
    app.cancel_engine_job();
    app.tab.busy = false;
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("N", Point::default());
    app.changed(before);
    let kept = app.tab.doc.clone();
    let _ = app.update(Message::FilePrepared(opened));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.name(), "oxygen");
    assert!(app.tab.path.is_none() && app.dirty());
    assert_eq!(app.tabs.background.len(), 1);
    assert_eq!(app.tabs.background[0].doc, kept);
    assert_eq!(app.drawing_save_target(false), (None, "oxygen.rsk".into()));
    assert_eq!(app.status, "Opened the editable drawing from oxygen.PNG");
    // Opening the same PNG again is another editable import, not a saved-file duplicate.
    let _ = app.update(Message::FilePrepared(
        prepare_contents(path, Ok(contents)).await,
    ));
    assert_eq!(app.tabs.background.len(), 2);
    assert!(app.strip().all(|tab| tab.path.is_none()));
    Ok(())
}

#[test]
fn png_open_preserves_ordinary_picture_refusal_and_explains_invalid_payload() {
    let path = std::path::Path::new("picture.png");
    let ordinary = png();
    assert_eq!(
        prepare(path, ordinary.clone()).unwrap_err(),
        "Can't import .png"
    );
    let invalid = reshiki::payload::embed("png", ordinary, b"invalid payload").unwrap();
    let reason = reshiki::payload::read(&invalid).unwrap().unwrap_err();
    let error = prepare(path, invalid).unwrap_err();
    assert_eq!(
        error,
        format!("This picture's editable drawing could not be read: {reason}")
    );
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::FilePrepared(Some((
        path.into(),
        Err(error.clone()),
    ))));
    assert!(app.error);
    assert_eq!(app.status, error);
    assert_eq!(app.tab.doc, before);
    assert!(app.tabs.background.is_empty());
}

#[test]
fn opening_takes_an_empty_tab_and_keeps_edited_and_drafting_tabs() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::FilePrepared(prepared()));
    assert!(
        app.tabs.background.is_empty(),
        "The empty startup tab is reused"
    );
    assert_eq!(app.tab.path, Some("oxygen.rsk".into()));
    for inline in [false, true] {
        let (mut app, _) = App::new();
        if inline {
            let _ = app.inline_action(inline_text::Action::Begin(None, Point::default()));
        } else {
            let before = app.tab.doc.clone();
            app.tab.doc.add_atom("N", Point::default());
            app.changed(before);
        }
        let expected = app.tab.doc.clone();
        let _ = app.update(Message::FilePrepared(prepared()));
        assert_eq!(app.tab.path, Some("oxygen.rsk".into()));
        assert!(!app.error);
        let [kept] = app.tabs.background.as_slice() else {
            panic!("One tab behind the opened file");
        };
        assert_eq!(kept.doc, expected);
        assert_eq!(kept.inline_text.is_some(), inline);
        assert!(kept.path.is_none());
    }
}

#[test]
fn open_errors_and_files_already_open_add_no_tab() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::FilePrepared(prepared()));
    let _ = app.update(Message::New);
    assert_eq!(app.tabs.active, 1);
    let _ = app.update(Message::FilePrepared(Some((
        "old.rsk".into(),
        Err("Old error".into()),
    ))));
    assert!(app.error && app.status == "Old error");
    let _ = app.update(Message::FilePrepared(prepared()));
    assert_eq!(app.strip().count(), 2);
    assert_eq!(app.tabs.active, 0, "The open file's tab comes to the front");
    assert_eq!(app.status, "oxygen.rsk is already open");
}

#[test]
fn native_worker_rejects_invalid_drawing_and_preserves_import_format() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (_, invalid) = runtime
        .block_on(prepare_contents("bad.rsk".into(), Ok(b"not JSON".to_vec())))
        .unwrap();
    assert!(invalid.unwrap_err().contains("Could not open document"));
    let (_, imported) = runtime
        .block_on(prepare_contents(
            "example.CDXML".into(),
            Ok(b"<CDXML/>".to_vec()),
        ))
        .unwrap();
    assert!(
        matches!(imported.unwrap(), Prepared::Import { format: "cdxml", contents } if contents == "<CDXML/>")
    );
    for (name, expected) in [
        ("nacl.cif", "cif"),
        ("ligand.pdb", "pdb"),
        ("POSCAR", "poscar"),
        ("CONTCAR", "poscar"),
        ("rutile.vasp", "poscar"),
        ("water.xyz", "xyz"),
        ("si.extxyz", "xyz"),
    ] {
        let (_, crystal) = runtime
            .block_on(prepare_contents(name.into(), Ok(b"text".to_vec())))
            .unwrap();
        assert!(
            matches!(crystal.unwrap(), Prepared::Import { format, .. } if format == expected),
            "{name}"
        );
    }
}

#[test]
fn native_open_raises_old_versions_and_clears_computed_labels() {
    let mut document = Document::default();
    let carbon = document.add_atom("C", Point::default());
    let oxygen = document.add_atom("O", Point::new(42., 0.));
    document.add_bond(carbon, oxygen, 1, "plain");
    document.version = 14;
    for atom in &mut document.atoms {
        atom.cip_label = Some("R".into());
    }
    document.bonds[0].cip_label = Some("E".into());
    // An explicit light canvas: dark drawings before version 17 migrate to 17.
    let mut json = serde_json::to_value(&document).unwrap();
    json["canvas_theme"] = serde_json::json!("light");
    let Prepared::Native(opened) = prepare(
        std::path::Path::new("old.rsk"),
        serde_json::to_vec(&json).unwrap(),
    )
    .unwrap() else {
        panic!("A native drawing opens natively");
    };
    assert_eq!(opened.version, 15);
    assert!(opened.atoms.iter().all(|atom| atom.cip_label.is_none()));
    assert!(opened.bonds[0].cip_label.is_none());
    let mut expected = document;
    expected.version = 15;
    reshiki::atom_labels::clear_computed(&mut expected);
    assert_eq!(*opened, expected);
    let error = prepare(std::path::Path::new("bad.rsk"), b"not JSON".to_vec()).unwrap_err();
    assert!(error.starts_with("Could not open document: "), "{error}");
}

#[test]
fn arrow_semantics_native_open_preserves_legacy_roles_and_warns() {
    use reshiki::{
        arrows::{ArrowStyle, Preset},
        document::Arrow,
        reactions::{Participant, Reaction},
    };
    for count in 0..=2 {
        let mut document = Document::default();
        for index in 0..=count + 1 {
            let preset = if index == 0 {
                Preset::Forward
            } else {
                Preset::Resonance
            };
            let atom = document.add_atom("C", Point::new(0., index as f32 * 60.));
            let arrow = document.next_id();
            document.arrows.push(Arrow::new(
                arrow,
                Point::new(80., index as f32 * 60.),
                Point::new(160., index as f32 * 60.),
                preset,
                ArrowStyle::preset(preset),
            ));
            let mut reaction = Reaction::new(arrow);
            // The final resonance arrow retains a cleared reaction record.
            if index <= count {
                reaction.reactants.push(Participant {
                    atoms: vec![atom],
                    coefficient: 1,
                });
            }
            document.reactions.push(reaction);
        }
        document.version = reshiki::document::REACTIONS;
        document.validate().unwrap();
        let path = PathBuf::from(format!("legacy-{count}.rsk"));
        let prepared = prepare(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        let (mut app, _) = App::new();
        let _ = app.update(Message::FilePrepared(Some((path, Ok(prepared)))));
        assert_eq!(app.tab.doc, document);
        assert!(!app.error && !app.dirty());
        assert_eq!(
            app.status,
            match count {
                0 => "Document opened",
                1 => "Document opened · Warning: 1 arrow has incompatible reaction roles",
                _ => "Document opened · Warning: 2 arrows have incompatible reaction roles",
            }
        );
    }
}

#[test]
fn save_dialog_answers_map_to_save_discard_and_cancel() {
    for answer in [Answer::Custom(SAVE.into()), Answer::Yes] {
        assert!(matches!(save_answer(answer), Message::Save));
    }
    for answer in [Answer::Custom(DONT_SAVE.into()), Answer::No] {
        assert!(matches!(save_answer(answer), Message::Discard));
    }
    for answer in [Answer::Custom("Cancel".into()), Answer::Cancel, Answer::Ok] {
        assert!(matches!(save_answer(answer), Message::Cancel));
    }
}

#[test]
fn saves_are_serial_and_cancelled_save_releases_pending_slot() {
    let (mut app, _) = App::new();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("drawing.rsk");
    app.tab.path = Some(path.clone());
    let task = app.update(Message::Save);
    assert!(task.units() > 0);
    assert!(app.file_io.saving);
    assert!(
        !path.exists(),
        "dispatch cannot perform filesystem work synchronously"
    );
    let second = app.update(Message::Save);
    assert_eq!(second.units(), 0);
    let _ = app.update(Message::Saved(
        app.tab.file_epoch,
        Box::new(app.tab.doc.clone()),
        Ok(None),
    ));
    assert!(!app.file_io.saving);
    assert!(!path.exists());
}

#[test]
fn microsoft_365_save_waits_for_receipt_and_failure_cancels_close_without_clearing_dirty() {
    let (mut app, _) = App::new();
    let path = PathBuf::from("recovery/drawing.rsk");
    app.tab.path = Some(path.clone());
    app.tab.office = Some(Binding::standalone(path, Host::Microsoft365));
    app.tab.doc.add_atom("O", Point::default());
    let snapshot = app.tab.doc.clone();
    let saved_before = app.tab.saved.clone();
    let _ = app.update(Message::Tabs(crate::app::tabs::Action::Close(None)));
    let task = app.update(Message::Save);
    assert!(task.units() > 0 && app.file_io.saving);
    assert!(app.status.contains("waiting for Microsoft 365"));
    assert!(app.dirty());
    let _ = app.update(Message::Saved(
        app.tab.file_epoch,
        Box::new(snapshot.clone()),
        Err("Draft saved; Microsoft 365 has not confirmed the update".into()),
    ));
    assert!(app.pending.is_none() && !app.file_io.saving);
    assert_eq!(app.tab.doc, snapshot);
    assert_eq!(app.tab.saved, saved_before);
    assert!(app.dirty() && app.error);
    assert!(!app.status.contains("Drawing updated"));
}

#[test]
fn stale_save_results_preserve_the_new_documents_pending_action() {
    for result in [
        Ok(Some("old.rsk".into())),
        Ok(None),
        Err("Old failure".into()),
    ] {
        let (mut app, _) = App::new();
        let epoch = app.tab.file_epoch;
        let snapshot = app.tab.doc.clone();
        let _ = app.update(Message::Save);
        app.reset_tab();
        app.tab.doc.add_atom("O", Point::default());
        let _ = app.update(Message::Tabs(crate::app::tabs::Action::Close(None)));
        app.status = "Current drawing".into();
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Saved(epoch, Box::new(snapshot), result));
        assert!(!app.file_io.saving);
        assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.status, "Current drawing");
        assert!(!app.error && app.tab.path.is_none());
        // The outstanding dialog still answers the new drawing's request.
        let _ = app.update(Message::Discard);
        assert!(app.tab.doc.all_ids().is_empty() && app.pending.is_none());
    }
}

#[test]
fn edits_during_a_save_reopen_the_pending_dialog() {
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("O", Point::default());
    let snapshot = app.tab.doc.clone();
    let _ = app.update(Message::Tabs(crate::app::tabs::Action::Close(None)));
    let _ = app.update(Message::Save);
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("N", Point::new(80., 0.));
    app.changed(before);
    let edited = app.tab.doc.clone();
    let task = app.update(Message::Saved(
        app.tab.file_epoch,
        Box::new(snapshot.clone()),
        Ok(Some("drawing.rsk".into())),
    ));
    assert!(task.units() > 0, "Ask about the edits made during the save");
    assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
    assert_eq!(app.tab.saved, snapshot);
    assert_eq!(app.tab.doc, edited);
    assert!(app.dirty());
    let _ = app.update(Message::Cancel);
    assert!(app.pending.is_none());
    assert_eq!(app.tab.doc, edited);
}

#[test]
fn prepare_rejects_unsupported_files_and_shows_the_reason() {
    let contents = b"caffeine\n     RDKit          2D\n\n 14 15  0  0  0  0  0  0  0  0999 V2000\n";
    for (name, expected) in [
        ("simple.sdf", "Can't import .sdf"),
        ("simple.SDF", "Can't import .sdf"),
        ("notes.txt", "Can't import .txt"),
        ("drawing", "Can't import this file"),
    ] {
        let path = std::path::Path::new(name);
        let error = prepare(path, contents.to_vec()).unwrap_err();
        assert_eq!(error, expected, "{name}");
        let (mut app, _) = App::new();
        let _ = app.update(Message::FilePrepared(Some((path.into(), Err(error)))));
        assert!(app.error);
        assert_eq!(app.status, expected, "{name}");
    }
}

#[test]
fn prepare_preserves_smiles_extensions() {
    for name in ["drawing.smi", "drawing.smiles", "drawing.SMI"] {
        assert!(
            matches!(
                prepare(std::path::Path::new(name), b"C".to_vec()).unwrap(),
                Prepared::Import { format: "smiles", contents } if contents == "C"
            ),
            "{name}"
        );
    }
}

/// The CLI and app agree on molecular file extensions the app opens.
/// Crystal inputs are recognized but rejected by the CLI argument parser.
#[test]
fn cli_input_formats_match_the_app() {
    let native = Document::default().file_json().unwrap();
    for extension in [
        "rsk", "RSK", "reshiki", "moruno", "mol", "rxn", "rsmi", "cdxml", "CDXML", "cdx", "inchi",
        "smi", "smiles",
    ] {
        let path = PathBuf::from(format!("drawing.{extension}"));
        let contents = if reshiki::compatibility::is_native_extension(extension) {
            native.clone()
        } else {
            b"C".to_vec()
        };
        let chosen = match prepare(&path, contents).unwrap() {
            Prepared::Native(_) => "reshiki",
            Prepared::Import { format, .. } => format,
            Prepared::Picture(_) => panic!("Structure files are not pictures"),
        };
        assert_eq!(
            chosen,
            reshiki::cli::input_format(extension),
            "{extension:?}"
        );
    }
}
