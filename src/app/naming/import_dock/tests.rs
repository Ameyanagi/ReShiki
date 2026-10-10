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
        canonical_smiles: "CCO".into(),
        smiles: "CCO".into(),
        synonyms: vec![],
        warnings: vec![],
        provenance: Provenance::OpsinRust,
    }
}

async fn preview(app: &App, smiles: &str) -> Result<Preview, String> {
    prepare_preview(app.engine.clone(), record(), smiles.into()).await
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
async fn one_insert_resolves_and_commits_structure_and_verified_caption_together()
-> Result<(), String> {
    let mut app = ready();
    let before = app.tab.doc.clone();
    let _ = app.insert_input();
    let pending = app.tab.naming.pending.unwrap();
    assert!(matches!(pending, Pending::Import(_, true)));
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
            .as_deref()
            .unwrap()
            .contains("omitted")
    );
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
    let detail = "The OPSIN Rust port could not interpret this name unambiguously: unsupported optical rotation";
    let _ = app.naming_action(Action::Finished(ticket, Box::new(Err(detail.into()))));
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert_eq!(app.tab.naming.notice.as_deref(), Some(detail));
    assert_eq!(
        compact_notice(detail),
        "Unsupported or ambiguous name. Try a more specific name."
    );
    assert_eq!(
        compact_notice("Could not start bounded local naming worker"),
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
    .await?;
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
        "import-name-caption",
        "import-name-preview",
        "import-name-insert",
        "import-name-details",
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
            .all(|node| node.id != "import-name-smiles" && !node.id.starts_with("naming."))
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
    let outcome =
        resolve_preview(app.engine.clone(), name.into(), naming::Cancel::default()).await?;
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
