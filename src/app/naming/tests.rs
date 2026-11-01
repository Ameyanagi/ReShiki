use super::test_support::{ethanol, record};
use super::*;

#[tokio::test]
async fn name_smiles_preview_uses_the_documents_depiction_options() -> Result<(), String> {
    let smiles = "CC[C@](C)(Br)Cl";
    let options = DrawingOptions {
        two_wedge_stereocentres: true,
        ..Default::default()
    };
    let preview = prepare_preview(
        LocalEngine::in_process(),
        record(smiles),
        smiles.into(),
        options,
    )
    .await
    .map_err(|error| error.to_string())?;
    assert_eq!(
        preview
            .document
            .bonds
            .iter()
            .filter(|bond| matches!(bond.display.as_str(), "wedge" | "hash"))
            .count(),
        2
    );
    naming::verify_identity(
        &preview.identity,
        &naming::document_identity(&preview.document)?.smiles,
    )?;
    Ok(())
}

#[tokio::test]
async fn generated_fusion_hydrogens_preserve_formula_inchi_and_naming_outcomes()
-> Result<(), String> {
    let engine = LocalEngine::in_process();
    for (smiles, expected_hydrogens, expected_formula) in [
        ("C1CC[C@H]2CCCC[C@@H]2C1", 2, "C10H18"),
        (
            "C[C@]12CC[C@H]3[C@@H]([C@@H]1CC[C@@H]2O)CCC4=CC(=O)CC[C@]34C",
            3,
            "C19H28O2",
        ),
    ] {
        let mut shown_request = Request::import_smiles(smiles);
        shown_request.drawing_options = Some(DrawingOptions::default());
        let shown = engine
            .request(shown_request)
            .await?
            .document
            .ok_or("Missing drawing with fusion hydrogens")?;
        let mut hidden_request = Request::import_smiles(smiles);
        hidden_request.drawing_options = Some(DrawingOptions {
            ring_fusion_hydrogens: false,
            ..Default::default()
        });
        let hidden = engine
            .request(hidden_request)
            .await?
            .document
            .ok_or("Missing drawing without fusion hydrogens")?;
        assert_eq!(
            shown
                .atoms
                .iter()
                .filter(|atom| atom.element == "H")
                .count(),
            expected_hydrogens,
            "{smiles}"
        );
        assert!(hidden.atoms.iter().all(|atom| atom.element != "H"));

        // Analyze the resulting drawings, rather than the pre-depiction
        // importer state, to catch H double-counting or changed drawn stereo.
        let shown_analysis = engine
            .request(Request::molecule("analyze", shown.clone()))
            .await?
            .analysis
            .ok_or("Missing analysis of explicit fusion hydrogens")?;
        let hidden_analysis = engine
            .request(Request::molecule("analyze", hidden.clone()))
            .await?
            .analysis
            .ok_or("Missing analysis without fusion hydrogens")?;
        assert_eq!(shown_analysis.formula, expected_formula, "{smiles}");
        assert_eq!(shown_analysis.formula, hidden_analysis.formula, "{smiles}");
        assert!(shown_analysis.inchi.starts_with("InChI=1S/"));
        assert_eq!(shown_analysis.inchi, hidden_analysis.inchi, "{smiles}");
        assert_eq!(
            shown_analysis.inchikey, hidden_analysis.inchikey,
            "{smiles}"
        );

        let shown_identity = naming::document_identity(&shown)?;
        let hidden_identity = naming::document_identity(&hidden)?;
        naming::verify_identity(smiles, &shown_identity.smiles)?;
        naming::verify_identity(&shown_identity.smiles, &hidden_identity.smiles)?;
        // Exercise the same naming worker in-process so this check never
        // launches the application executable or a GUI.
        let name = |identity: &naming::Identity| -> Result<Result<String, String>, String> {
            let request = serde_json::json!({
                "operation": "generate",
                "protocol": 2,
                "profile": "ReShiki organic rules 1",
                "smiles": identity.smiles,
            });
            let response =
                naming::worker::respond(&serde_json::to_vec(&request).map_err(|e| e.to_string())?)?;
            let response: serde_json::Value =
                serde_json::from_slice(&response).map_err(|e| e.to_string())?;
            serde_json::from_value(response["result"].clone()).map_err(|e| e.to_string())
        };
        let shown_name = name(&shown_identity)?;
        let hidden_name = name(&hidden_identity)?;
        assert_eq!(shown_name, hidden_name, "{smiles}");
    }
    Ok(())
}

#[test]
fn structure_naming_failures_do_not_create_name_import_notices() {
    for generate in [false, true] {
        for stale in [false, true] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.doc = ethanol();
            app.tab.selected = app.tab.doc.all_ids();
            let action = if generate {
                Action::Generate
            } else {
                Action::ShowMoleculeName(app.tab.selected.clone())
            };
            let _ = app.naming_action(action);
            let pending = app.tab.naming.pending.unwrap();
            if generate {
                assert!(matches!(pending, Pending::Generate(_)));
            } else {
                assert!(matches!(pending, Pending::Caption(_)));
            }
            let detail = "Unsupported fused-ring parent in local organic rules";
            let result = if stale {
                app.tab.revision += 1;
                Ok(Outcome::Structure(record("CCO")))
            } else {
                Err(naming::Error::Rejected(detail.into()))
            };
            let before = app.tab.doc.clone();
            let _ = app.naming_action(Action::Finished(pending.ticket(), Box::new(result)));
            assert!(app.tab.naming.notice.is_none());
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
            if generate {
                assert_eq!(
                    app.tab.naming.structure_error.as_deref(),
                    Some(if stale {
                        "The drawing changed. Try again."
                    } else {
                        detail
                    })
                );
            } else {
                assert!(app.tab.naming.structure_error.is_none());
                assert!(app.error);
                assert_eq!(
                    app.status,
                    if stale {
                        "The molecule changed while naming. Show its name again"
                    } else {
                        detail
                    }
                );
            }
        }
    }
}

#[test]
fn structure_naming_preserves_import_notice_and_resolve_clears_only_that_notice() {
    for generate in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc = ethanol();
        app.tab.selected = app.tab.doc.all_ids();
        app.tab.naming.name = "ethanol".into();
        let notice = "Keep the name import details";
        app.tab.naming.notice = Some(Notice::new(NoticeKind::Info, notice));
        let previous_error = "Previous structure naming error";
        app.tab.naming.structure_error = Some(previous_error.into());
        let action = if generate {
            Action::Generate
        } else {
            Action::ShowMoleculeName(app.tab.selected.clone())
        };
        let _ = app.naming_action(action);
        assert_eq!(app.tab.naming.notice.as_ref().unwrap().detail, notice);
        assert_eq!(
            app.tab.naming.structure_error.as_deref(),
            (!generate).then_some(previous_error)
        );
        let ticket = app.tab.naming.pending.unwrap().ticket();
        let detail = "Cannot generate this structure name";
        let _ = app.naming_action(Action::Finished(
            ticket,
            Box::new(Err(naming::Error::Rejected(detail.into()))),
        ));
        assert_eq!(app.tab.naming.notice.as_ref().unwrap().detail, notice);
        let structure_error = if generate { detail } else { previous_error };
        assert_eq!(
            app.tab.naming.structure_error.as_deref(),
            Some(structure_error)
        );
        let _ = app.naming_action(Action::Resolve);
        assert!(matches!(app.tab.naming.pending, Some(Pending::Request(_))));
        assert!(app.tab.naming.notice.is_none());
        assert_eq!(
            app.tab.naming.structure_error.as_deref(),
            Some(structure_error)
        );
    }
}

#[test]
fn cancelling_structure_naming_preserves_name_import_notice() {
    for generate in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        app.tab.doc = ethanol();
        app.tab.selected = app.tab.doc.all_ids();
        let notice = "Keep the name import details";
        app.tab.naming.notice = Some(Notice::new(NoticeKind::Info, notice));
        let action = if generate {
            Action::Generate
        } else {
            Action::ShowMoleculeName(app.tab.selected.clone())
        };
        let _ = app.naming_action(action);
        let _ = app.naming_action(Action::Cancel);
        assert!(!app.tab.naming.is_pending());
        assert!(app.tab.naming.local_cancel.is_none());
        assert_eq!(app.tab.naming.notice.as_ref().unwrap().detail, notice);
        let detail = "Local name generation was cancelled";
        if generate {
            assert_eq!(app.tab.naming.structure_error.as_deref(), Some(detail));
        } else {
            assert!(app.tab.naming.structure_error.is_none());
            assert!(app.error);
            assert_eq!(app.status, detail);
        }
    }
}

#[test]
fn native_open_preserves_pending_naming_in_an_empty_source_tab() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let source = app.tab.id;
    let epoch = app.tab.file_epoch;
    assert!(app.update(Message::Naming(Action::Generate)).units() > 0);
    let ticket = app.tab.naming.pending.unwrap().ticket();
    assert!(!app.tab.reusable());
    let mut document = Document::default();
    document.add_atom("O", Point::default());
    let _ = app.update(Message::FilePrepared(Some((
        "pending-name-open.rsk".into(),
        Ok(super::super::files::Prepared::Native(Box::new(
            document.clone(),
        ))),
    ))));
    assert_ne!(app.tab.id, source);
    assert!(app.tab.file_epoch > epoch);
    assert!(!app.tab.naming.is_pending());
    assert_eq!(app.tab.doc, document);
    let original = app.strip().find(|tab| tab.id == source).unwrap();
    assert_eq!(original.file_epoch, epoch);
    assert_eq!(original.naming.pending.unwrap().ticket(), ticket);
    let front = app.tab.id;
    let _ = app.update(Message::Tab(
        source,
        Box::new(Message::Naming(Action::Finished(
            ticket,
            Box::new(Ok(Outcome::Structure(record("CCO")))),
        ))),
    ));
    let original = app.strip().find(|tab| tab.id == source).unwrap();
    assert!(!original.naming.is_pending());
    assert_eq!(original.naming.structure.as_ref().unwrap().0, ticket);
    assert_eq!(app.tab.id, front);
    assert_eq!(app.tab.doc, document);
}

#[test]
fn file_epoch_clears_naming_and_old_completion_cannot_clear_a_new_request() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    assert!(app.update(Message::Naming(Action::Generate)).units() > 0);
    let old = app.tab.naming.pending.unwrap().ticket();
    assert!(app.tab.naming.local_cancel.is_some());
    // Retained generated names stay visible as previous names after a file change.
    app.tab.naming.structure = Some((old, record("CCO")));
    app.tab.naming.import_alias = Some(0);
    let epoch = app.tab.file_epoch;
    app.begin_file_epoch();
    assert!(app.tab.file_epoch > epoch);
    assert!(!app.tab.naming.is_pending());
    assert!(app.tab.naming.local_cancel.is_none());
    assert!(app.tab.naming.import_alias.is_none());
    assert_eq!(app.tab.naming.structure.as_ref().unwrap().0, old);
    assert!(app.update(Message::Naming(Action::Generate)).units() > 0);
    let current = app.tab.naming.pending.unwrap().ticket();
    assert_ne!(current, old);
    let _ = app.update(Message::Naming(Action::Finished(
        old,
        Box::new(Ok(Outcome::Structure(record("CCO")))),
    )));
    assert_eq!(app.tab.naming.pending.unwrap().ticket(), current);
    assert!(app.tab.naming.local_cancel.is_some());
    assert!(app.tab.naming.structure.is_none());
    assert!(app.tab.naming.notice.is_none());
    let _ = app.update(Message::Naming(Action::Finished(
        current,
        Box::new(Ok(Outcome::Structure(record("CCO")))),
    )));
    assert!(!app.tab.naming.is_pending());
    assert_eq!(app.tab.naming.structure.as_ref().unwrap().0, current);
}

#[tokio::test]
async fn preview_insertion_is_native_editable_and_one_history_step() -> Result<(), String> {
    let (mut app, _) = App::new();
    let preview = prepare_preview(
        app.engine.clone(),
        record("CCO"),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    app.tab.naming.set_preview(preview);
    let preview = app.tab.naming.preview.as_ref().unwrap();
    let atom = preview.document.atoms[0].id;
    let point = preview.document.atoms[0].position;
    let _ = app.naming_action(Action::PreviewEdit(Edit::Move(vec![atom], 3., 0.)));
    assert_eq!(
        app.tab
            .naming
            .preview
            .as_ref()
            .unwrap()
            .document
            .atom(atom)
            .unwrap()
            .position,
        point.offset(3., 0.)
    );
    let original = app.tab.doc.clone();
    let _ = app.naming_action(Action::Insert);
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert_eq!(app.tab.doc.bonds.len(), 2);
    assert_eq!(naming::document_identity(&app.tab.doc)?.smiles, "CCO");
    let inserted = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, inserted);
    let json = serde_json::to_string(&app.tab.doc).map_err(|e| e.to_string())?;
    let restored: Document = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    assert_eq!(naming::document_identity(&restored)?.smiles, "CCO");
    Ok(())
}

#[tokio::test]
async fn exhausted_ids_report_failed_name_preview_without_changing_the_drawing()
-> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let preview = prepare_preview(
        app.engine.clone(),
        record("CCO"),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    app.tab.doc.add_atom("O", Point::new(120., 80.));
    app.tab.doc.atoms[0].id = u64::MAX - 1;
    app.tab.doc.validate()?;
    let before = app.tab.doc.clone();
    let mut attempted = before.clone();
    assert!(editing::append(&mut attempted, &preview.document, Point::default()).is_empty());
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
    app.tab.naming.set_preview(preview);
    let notice = "Keep the preview details";
    app.tab.naming.notice = Some(Notice::new(NoticeKind::Info, notice));

    let _ = app.naming_action(Action::Insert);

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
    assert_eq!(app.status, "Could not insert ethan-1-ol");
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
async fn name_preview_is_placed_beside_a_caption_only_drawing() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id: 1,
        position: Point::new(250., 80.),
        text: "Existing caption".into(),
        format: Default::default(),
    });
    app.tab.camera.center = Point::new(-100., -60.);
    let before = app.tab.doc.clone();
    let preview = prepare_preview(
        app.engine.clone(),
        record("CCO"),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    let source = preview.document.clone();
    let center = editing::center(&source, &source.all_ids());
    let (_, existing_max) = before.bounds();
    let (insert_min, _) = source.bounds();
    let offset = Point::new(
        existing_max.x + before.drawing_style.bond_length_world - insert_min.x,
        app.tab.camera.center.y - center.y,
    );
    app.tab.naming.set_preview(preview);

    let _ = app.naming_action(Action::Insert);

    assert_eq!(app.tab.doc.annotations, before.annotations);
    assert_eq!(app.tab.doc.atoms.len(), source.atoms.len());
    for (inserted, original) in app.tab.doc.atoms.iter().zip(&source.atoms) {
        assert_eq!(
            inserted.position,
            original.position.offset(offset.x, offset.y)
        );
    }
    assert_eq!(
        app.tab.selected,
        app.tab
            .doc
            .atoms
            .iter()
            .map(|atom| atom.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(app.tab.history.undo_frames().count(), 1);
    assert_eq!(app.tab.history.frames(), 1);
    assert!(!app.error);
    let status = "Inserted editable name result · Undo removes the insertion";
    assert_eq!(app.status, status);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    Ok(())
}

#[tokio::test]
async fn native_preview_and_exchange_retain_specified_and_unspecified_stereo() -> Result<(), String>
{
    let engine = LocalEngine::default();
    for smiles in [
        "C[C@@H](O)C(=O)O",
        "C[C@H](O)C(=O)O",
        "CC(O)C(=O)O",
        "C/C=C/C",
        "C/C=C\\C",
        "[13CH3]CO",
        "CC(=O)[O-]",
    ] {
        let preview = prepare_preview(
            engine.clone(),
            record(smiles),
            smiles.into(),
            Default::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
        naming::verify_identity(
            smiles,
            &naming::document_identity(&preview.document)?.smiles,
        )?;
        let mut request = Request::molecule("export", preview.document);
        request.format = Some("mol".into());
        let mol = engine
            .request(request)
            .await?
            .output
            .ok_or("Missing MOL export")?;
        let restored = engine
            .request(Request::import("mol", &mol))
            .await?
            .document
            .ok_or("Missing restored drawing")?;
        naming::verify_identity(smiles, &naming::document_identity(&restored)?.smiles)?;
    }
    Ok(())
}

#[tokio::test]
async fn warnings_and_unapplied_smiles_edits_block_insertion() -> Result<(), String> {
    let (mut app, _) = App::new();
    let mut source = record("CCO");
    source.warnings.push("Review native import layout".into());
    let preview = prepare_preview(app.engine.clone(), source, "CCO".into(), Default::default())
        .await
        .map_err(|error| error.to_string())?;
    app.tab.naming.set_preview(preview);
    let _ = app.naming_action(Action::Insert);
    assert!(app.tab.doc.atoms.is_empty());
    let _ = app.naming_action(Action::Acknowledge(true));
    let _ = app.naming_action(Action::Smiles("CCN".into()));
    let _ = app.naming_action(Action::Insert);
    assert!(app.tab.doc.atoms.is_empty());
    let _ = app.naming_action(Action::Smiles("CCO".into()));
    let _ = app.naming_action(Action::Insert);
    assert_eq!(app.tab.doc.atoms.len(), 3);
    Ok(())
}

#[tokio::test]
async fn changed_preview_no_longer_claims_original_chemical_name() -> Result<(), String> {
    let engine = LocalEngine::default();
    let original = prepare_preview(
        engine.clone(),
        record("CCO"),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    assert!(!original.modified);
    let edited = prepare_preview(engine, record("CCO"), "CCN".into(), Default::default())
        .await
        .map_err(|error| error.to_string())?;
    assert!(edited.modified);
    assert_eq!(edited.identity, "CCN");
    Ok(())
}

#[tokio::test]
async fn typing_during_preview_rebuild_retains_new_text_and_rejects_old_completion()
-> Result<(), String> {
    for action in [Action::UpdatePreview, Action::RestorePreview] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let initial = if matches!(action, Action::RestorePreview) {
            "CCN"
        } else {
            "CCO"
        };
        let preview = prepare_preview(
            app.engine.clone(),
            record("CCO"),
            initial.into(),
            Default::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
        app.tab.naming.set_preview(preview);
        let _ = app.naming_action(Action::Smiles("CCN".into()));
        let result_identity = if matches!(action, Action::RestorePreview) {
            "CCO"
        } else {
            "CCN"
        };
        let _ = app.naming_action(action);
        let stale = app.tab.naming.pending.unwrap().ticket();
        let _ = app.naming_action(Action::Smiles("CCC".into()));
        assert!(app.tab.naming.pending.is_none());
        let old = prepare_preview(
            app.engine.clone(),
            record("CCO"),
            result_identity.into(),
            Default::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
        let _ = app.naming_action(Action::Finished(
            stale,
            Box::new(Ok(Outcome::Preview(Box::new(old.clone())))),
        ));
        assert_eq!(app.tab.naming.smiles, "CCC");
        assert_eq!(app.tab.naming.preview.as_ref().unwrap().identity, initial);
        let _ = app.naming_action(Action::Insert);
        assert!(app.tab.doc.atoms.is_empty());
        assert!(!app.tab.history.can_undo());

        let _ = app.naming_action(Action::UpdatePreview);
        let latest = app.tab.naming.pending.unwrap().ticket();
        let _ = app.naming_action(Action::Finished(
            stale,
            Box::new(Ok(Outcome::Preview(Box::new(old)))),
        ));
        assert_eq!(app.tab.naming.pending.unwrap().ticket(), latest);
        assert_eq!(app.tab.naming.smiles, "CCC");
        let current = prepare_preview(
            app.engine.clone(),
            record("CCO"),
            "CCC".into(),
            Default::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
        let _ = app.naming_action(Action::Finished(
            latest,
            Box::new(Ok(Outcome::Preview(Box::new(current)))),
        ));
        assert_eq!(app.tab.naming.preview.as_ref().unwrap().identity, "CCC");
        let _ = app.naming_action(Action::Insert);
        assert_eq!(naming::document_identity(&app.tab.doc)?.smiles, "CCC");
    }
    Ok(())
}

#[tokio::test]
async fn preview_text_edits_do_not_cancel_independent_local_name_generation() -> Result<(), String>
{
    let (mut app, _) = App::new();
    let preview = prepare_preview(
        app.engine.clone(),
        record("CCO"),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    app.tab.doc = preview.document.clone();
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.naming.set_preview(preview);
    let _ = app.naming_action(Action::Generate);
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let _ = app.naming_action(Action::Smiles("CCC".into()));
    assert_eq!(app.tab.naming.pending.unwrap().ticket(), ticket);
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Structure(record("CCO")))),
    ));
    assert!(app.tab.naming.structure.is_some());
    assert_eq!(app.tab.naming.smiles, "CCC");
    assert_eq!(app.tab.naming.preview.as_ref().unwrap().identity, "CCO");
    Ok(())
}

#[test]
fn stale_results_cannot_reach_another_local_document_or_request() {
    let (mut app, _) = App::new();
    let ticket = app.naming_ticket();
    let _ = app.naming_action(Action::Name("aspirin".into()));
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Names(vec![record("CCO")], None))),
    ));
    assert!(app.tab.naming.candidates.is_empty());
    let ticket = app.naming_ticket();
    app.tab.revision += 1;
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Structure(record("CCO")))),
    ));
    assert!(app.tab.naming.structure.is_none());
}

#[tokio::test]
async fn caption_requires_current_graph_and_is_undoable() -> Result<(), String> {
    let (mut app, _) = App::new();
    let preview = prepare_preview(
        app.engine.clone(),
        record("CCO"),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    app.tab.doc = preview.document;
    app.tab.selected = app.tab.doc.all_ids();
    let ticket = Ticket {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        serial: 0,
    };
    app.tab.naming.structure = Some((ticket, record("CCO")));
    let _ = app.naming_action(Action::Caption);
    assert_eq!(app.tab.doc.annotations[0].text, "ethan-1-ol");
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.annotations.is_empty());
    app.tab.selected.clear();
    let _ = app.naming_action(Action::Caption);
    assert!(app.tab.doc.annotations.is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "Opt-in native renderer accessibility metadata and dispatch"]
async fn controls_publish_named_native_actions_and_editable_values() -> Result<(), String> {
    use iced::advanced::renderer::Headless;
    use reshiki::accessibility::{Activate, Collect};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.naming.name = "ethanol".into();
    let mut source = record("CCO");
    source
        .warnings
        .push("Check this source interpretation".into());
    let preview = prepare_preview(
        app.engine.clone(),
        source.clone(),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    app.tab.naming.set_preview(preview);
    app.tab.naming.structure = Some((
        Ticket {
            epoch: app.tab.file_epoch,
            revision: app.tab.revision,
            serial: 0,
        },
        source,
    ));
    let size = iced::Size::new(340., 2400.);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut ui = iced_runtime::UserInterface::build(
        app.naming_panel(),
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
        "naming.name",
        "naming.smiles",
        "naming.resolve",
        "naming.generate",
        "naming.acknowledge",
        "naming.insert",
        "naming.copy-name",
        "naming.caption",
    ] {
        let node = snapshot
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("Missing {id}"));
        assert!(!node.name.is_empty());
        assert!(node.visible_bounds.is_some(), "Invisible {id}");
    }
    let input = snapshot
        .nodes
        .iter()
        .find(|node| node.id == "naming.name")
        .unwrap();
    assert_eq!(input.value.as_deref(), Some("ethanol"));
    let mut activate = Activate::<Message>::new("naming.resolve");
    ui.operate(
        &renderer,
        &mut iced::advanced::widget::operation::black_box(&mut activate),
    );
    assert!(matches!(
        activate.message(),
        Some(Message::Naming(Action::Resolve))
    ));
    Ok(())
}

#[tokio::test]
#[ignore = "Opt-in real renderer scrolled preview pointer regression"]
async fn scrolled_insert_click_reaches_button_instead_of_preview() -> Result<(), String> {
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::operation::{self, scrollable::AbsoluteOffset};
    use iced::{Event, mouse};
    use reshiki::accessibility::Collect;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let preview = prepare_preview(
        app.engine.clone(),
        record("CCO"),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    app.tab.naming.set_preview(preview);
    let size = iced::Size::new(340., 620.);
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    let mut ui = iced_runtime::UserInterface::build(
        iced::widget::scrollable(app.naming_panel())
            .id("naming-test-scroll")
            .height(620),
        size,
        Default::default(),
        &mut renderer,
    );
    let mut scroll = operation::scrollable::scroll_to::<()>(
        iced::advanced::widget::Id::from("naming-test-scroll"),
        AbsoluteOffset {
            x: Some(0.),
            y: Some(300.),
        },
    );
    ui.operate(&renderer, &mut operation::black_box(&mut scroll));
    let mut collect = Collect::new(iced::Rectangle::with_size(size));
    ui.operate(&renderer, &mut operation::black_box(&mut collect));
    let bounds = collect
        .snapshot()
        .nodes
        .iter()
        .find(|n| n.id == "naming.insert")
        .and_then(|n| n.visible_bounds)
        .ok_or("Insert control not visible after scroll")?;
    let point = bounds.center();
    let mut messages = vec![];
    for event in [
        Event::Mouse(mouse::Event::CursorMoved { position: point }),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        ui.update(
            &[event],
            mouse::Cursor::Available(point),
            &mut renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
    }
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Naming(Action::Insert))),
        "Scrolled click produced {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::Naming(Action::PreviewEdit(Edit::Select(_)))))
    );
    drop(ui);
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(app.tab.doc.atoms.len(), 3);
    assert!(app.tab.history.can_undo());
    Ok(())
}

#[test]
fn local_parse_cancels_obsolete_tickets() {
    let (mut app, _) = App::new();
    let _ = app.naming_action(Action::Name("ethanol".into()));
    let _ = app.naming_action(Action::Resolve);
    assert!(app.tab.naming.pending.is_some());
    assert!(app.tab.naming.local_cancel.is_some());
    let ticket = app.tab.naming.pending.unwrap().ticket();
    let _ = app.naming_action(Action::Cancel);
    assert!(app.tab.naming.pending.is_none());
    assert!(app.tab.naming.local_cancel.is_none());
    let notice = app.tab.naming.notice.as_ref().unwrap();
    assert_eq!(notice.kind, NoticeKind::Retry);
    assert_eq!(notice.detail, "Local name parsing was cancelled");
    assert_eq!(notice.summary(), notice.detail);
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Names(vec![record("CCO")], None))),
    ));
    assert!(app.tab.naming.candidates.is_empty());
    assert_eq!(app.tab.naming.name, "ethanol");
    let _ = app.naming_action(Action::Resolve);
    assert!(app.tab.naming.local_cancel.is_some());
    let _ = app.naming_action(Action::Name("propane".into()));
    assert!(app.tab.naming.pending.is_none());
    assert!(app.tab.naming.local_cancel.is_none());
    assert_eq!(app.tab.naming.name, "propane");
}
