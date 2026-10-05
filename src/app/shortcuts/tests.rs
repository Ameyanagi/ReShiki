#[tokio::test]
async fn ring_display_in_the_gallery_preserves_unrelated_ligands_and_captions() -> anyhow::Result<()>
{
    let doc: reshiki::document::Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))?;
    let benzene = reshiki::editing::groups(&doc, &doc.all_ids())
        .into_iter()
        .find(|ids| {
            let part = reshiki::editing::selection(&doc, ids);
            part.atoms.len() == 6
                && part.bonds.len() == 6
                && part.bonds.iter().filter(|b| b.order == 2).count() == 3
        })
        .ok_or_else(|| anyhow::anyhow!("Missing benzene sample"))?;
    let mut request = reshiki::engine::Request::molecule("aromatic", doc.clone());
    request.selected_ids = Some(benzene.clone());
    let result = aromatic_selection(reshiki::engine::LocalEngine::default(), request)
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(result.analysis.is_none());
    let changed = result
        .document
        .ok_or_else(|| anyhow::anyhow!("Missing drawing"))?;
    assert_eq!(changed.annotations, doc.annotations);
    assert_eq!(changed.page_layout, doc.page_layout);
    assert_eq!(changed.abbreviations, doc.abbreviations);
    for atom in doc.atoms.iter().filter(|a| !benzene.contains(&a.id)) {
        assert_eq!(changed.atom(atom.id), Some(atom));
    }
    assert!(
        changed
            .bonds
            .iter()
            .filter(|b| benzene.contains(&b.a) && benzene.contains(&b.b))
            .all(|b| b.order == 4)
    );
    Ok(())
}

#[tokio::test]
async fn aromatic_request_fallbacks_keep_the_original_request_and_validation_error() {
    use reshiki::{
        document::Document,
        engine::{ChemistryEngine, LocalEngine, Request},
    };
    let engine = LocalEngine::default();
    let mut document = Document::default();
    let ids = reshiki::editing::ring(&mut document, Default::default(), 6, true, 0.);
    for selected in [None, Some(vec![]), Some(vec![u64::MAX]), Some(ids)] {
        let mut request = Request::molecule("aromatic", document.clone());
        request.selected_ids = selected;
        let expected = engine.execute(request.clone()).await;
        let actual = aromatic_selection(engine.clone(), request).await;
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }
    let mut request = Request::molecule("aromatic", document.clone());
    request.document = None;
    let expected = engine.execute(request.clone()).await;
    let actual = aromatic_selection(engine.clone(), request).await;
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );

    document.atoms[0].position.x = f32::NAN;
    let expected = document.validate().unwrap_err();
    let actual = aromatic_selection(engine, Request::molecule("aromatic", document)).await;
    assert_eq!(actual.unwrap_err(), expected);
}

#[test]
fn a_toggles_a_selected_ring_when_the_pointer_is_off_the_structure() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    app.edit(Edit::Hover(Some(Point::new(200., 200.))));
    let before = app.tab.doc.clone();
    let _ = app.context_key("a");
    assert!(
        app.tab.busy,
        "The selection-only display shortcut remains available"
    );
    assert_eq!(app.tab.doc, before);
}

#[test]
fn repeated_phenyl_shortcuts_ignore_the_automatic_ring_selection() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let start = app.tab.doc.add_atom("C", Point::default());
    let end = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(start, end, 1, "plain");
    app.edit(Edit::Hover(Some(Point::new(42., 0.))));
    let _ = app.context_key("a");
    assert!(!app.error);
    assert_eq!(app.tab.selected.len(), 6);
    let mut steps = vec![app.tab.doc.clone()];
    for expected_atoms in [13, 19] {
        let original = app.tab.doc.clone();
        let target = app
            .tab
            .selected
            .iter()
            .filter_map(|id| app.tab.doc.atom(*id))
            .max_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .unwrap()
            .position;
        app.edit(Edit::Hover(Some(target)));
        let _ = app.context_key("a");
        assert!(
            !app.tab.busy,
            "Hovering a selected atom must not start aromatic display conversion"
        );
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.atoms.len(), expected_atoms);
        assert!(app.tab.doc.bonds.starts_with(&original.bonds));
        reshiki::chemistry::document::prepare(&app.tab.doc).unwrap();
        steps.push(app.tab.doc.clone());
    }
    for original in steps.iter().rev().skip(1) {
        let _ = app.update(Message::Undo);
        assert_eq!(&app.tab.doc, original);
    }
    for added in steps.iter().skip(1) {
        let _ = app.update(Message::Redo);
        assert_eq!(&app.tab.doc, added);
    }
}

#[test]
fn a_fuses_at_a_hovered_bond_even_when_its_ring_is_selected() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    let a = app.tab.doc.atoms[0].position;
    let b = app.tab.doc.atoms[1].position;
    app.edit(Edit::Hover(Some(Point::new(
        (a.x + b.x) / 2.,
        (a.y + b.y) / 2.,
    ))));
    let _ = app.context_key("a");
    assert!(!app.tab.busy);
    assert!(!app.error, "{}", app.status);
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (10, 11));
    reshiki::chemistry::document::prepare(&app.tab.doc).unwrap();
}

use super::*;
use crate::canvas::Edit;
use reshiki::atom_labels::Carbons;
use reshiki::document::{Document, Point};

#[test]
fn hover_atom_shortcuts_reveal_carbon_replace_elements_and_undo_once() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.atom_mut(a).unwrap().label_h = 3;
    app.tab.selected = vec![b];
    let original = app.tab.doc.clone();
    app.edit(Edit::Hover(Some(Point::default())));
    let _ = app.context_key("c");
    assert_eq!(app.tab.selected, vec![a]);
    assert_eq!(
        app.tab.doc.atom(a).unwrap().display.carbons,
        Some(Carbons::All)
    );
    assert_eq!(app.tab.doc.atom(a).unwrap().label_h, 3);
    assert!(!super::super::chemistry_changed(&original, &app.tab.doc));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.context_key("n");
    assert_eq!(app.tab.doc.atom(a).unwrap().element, "N");
    assert_eq!(app.tab.doc.atom(b).unwrap().element, "C");
    assert!(app.tab.labels_dirty);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn selected_and_hovered_bond_shortcuts_set_order_and_cycle_only_double_position() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a, b];
    for (key, order) in [("2", 2), ("3", 3), ("1", 1)] {
        let _ = app.context_key(key);
        assert_eq!(app.tab.doc.bonds[0].order, order);
    }
    app.tab.selected.clear();
    app.edit(Edit::Hover(Some(Point::new(21., 0.))));
    let _ = app.context_key("2");
    let before = app.tab.doc.clone();
    let _ = app.context_key("2");
    assert_eq!(app.tab.doc.bonds[0].order, 2);
    assert_ne!(
        app.tab.doc.bonds[0].double_position,
        before.bonds[0].double_position
    );
    assert!(!super::super::chemistry_changed(&before, &app.tab.doc));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    app.edit(Edit::Hover(Some(Point::default())));
    let _ = app.context_key("s");
    assert_eq!(app.tab.doc.atom(a).unwrap().element, "S");
    assert_eq!(app.tab.doc.bonds[0].order, 2);
}

#[test]
fn stale_hover_and_empty_cleanup_do_not_modify_the_drawing() {
    let (mut app, _) = App::new();
    // Verify the classic hover epoch guard. Hybrid mode intentionally
    // initializes a fresh visible hotspot when the file epoch changes.
    let _ = app.update(Message::KeyboardDrawing(
        super::super::keyboard_drawing::Action::Leave,
    ));
    assert!(!app.tab.keyboard_drawing.enabled());
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.add_atom("C", Point::default());
    app.edit(Edit::Hover(Some(Point::default())));
    app.tab.file_epoch = app.tab.file_epoch.wrapping_add(1);
    let before = app.tab.doc.clone();
    let _ = app.context_key("o");
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tool, Tool::Atom);
    let _ = app.update(Message::Clean);
    assert!(!app.tab.busy);
    assert!(app.tab.cleanup.is_none());
    assert!(app.status.contains("Select"));
    assert_eq!(app.tab.doc, before);
}

#[tokio::test]
async fn aromatic_shortcut_commits_once_keeps_hydrogens_and_ignores_late_results() {
    use super::super::{Job, Request};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = app
        .engine
        .request(Request::import_smiles("c1cc[nH]c1"))
        .await
        .unwrap()
        .document
        .unwrap();
    app.tab.selected = app.tab.doc.all_ids();
    let original = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::AromaticDisplay);
    assert!(app.tab.busy);
    let mut request = Request::molecule("aromatic", app.tab.doc.clone());
    request.selected_ids = Some(app.tab.selected.clone());
    let response = app.engine.request(request).await.unwrap();
    let _ = app.update(Message::EngineDone {
        revision,
        kind: Job::AromaticDisplay,
        result: Box::new(Ok(response.clone())),
    });
    assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
    assert!(app.tab.doc.atoms.iter().all(|a| a.label_h == 1));
    assert!(!app.tab.labels_dirty);
    assert_eq!(app.tab.selected, original.all_ids());
    let circled = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert!(super::super::same_drawing(&app.tab.doc, &original));
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert!(super::super::same_drawing(&app.tab.doc, &circled));
    let _ = app.update(Message::EngineDone {
        revision,
        kind: Job::AromaticDisplay,
        result: Box::new(Ok(response)),
    });
    assert!(super::super::same_drawing(&app.tab.doc, &circled));
}
