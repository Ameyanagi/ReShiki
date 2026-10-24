use super::*;
use crate::app::same_drawing;
use reshiki::document::{Document, Point};

fn fixture() -> (App, u64) {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let carbon = app.tab.doc.add_atom("C", Point::default());
    let oxygen = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(carbon, oxygen, 1, "plain");
    app.tab.selected = vec![oxygen];
    (app, oxygen)
}

fn compute(app: &App) -> Result<Arc<Refresh>, String> {
    Refresh::calculate(&app.tab.doc, &app.tab.label_refresh.cache).map(Arc::new)
}

#[test]
fn background_labels_refresh_without_adding_an_undo_step() {
    use crate::app::tabs::{Action, tests::Front};
    let (mut app, oxygen) = fixture();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::ContextKey("o".into()));
    let (id, key) = (app.tab.id, app.tab.label_refresh.pending.unwrap());
    let result = compute(&app);
    let front = Front::new(&mut app);
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::LabelsReady(key, result)),
    ));
    front.assert_unchanged(&app);
    let tab = &app.tabs.background[0];
    assert!(tab.label_refresh.pending.is_none());
    assert_eq!(tab.doc.atom(oxygen).unwrap().label_h, 1);
    assert_eq!(tab.revision, key.revision);
    let _ = app.update(Message::Tabs(Action::Select(id)));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before, "Undo still reverts the original edit");
    assert!(!app.tab.history.can_undo());
}

#[test]
fn stale_background_labels_schedule_their_next_refresh_in_the_same_tab() {
    use crate::app::tabs::tests::Front;
    let (mut app, oxygen) = fixture();
    let _ = app.update(Message::ContextKey("o".into()));
    let (id, key) = (app.tab.id, app.tab.label_refresh.pending.unwrap());
    let result = compute(&app);
    let _ = app.update(Message::ContextKey("n".into()));
    let revision = app.tab.revision;
    let front = Front::new(&mut app);
    let task = app.update(Message::Tab(
        id,
        Box::new(Message::LabelsReady(key, result)),
    ));
    assert!(task.units() > 0);
    front.assert_unchanged(&app);
    assert!(app.tab.label_refresh.pending.is_none());
    let tab = &app.tabs.background[0];
    assert_eq!(tab.doc.atom(oxygen).unwrap().element, "N");
    assert_eq!(tab.doc.atom(oxygen).unwrap().label_h, 0);
    let next = tab.label_refresh.pending.unwrap();
    assert_eq!(next.revision, revision);
    assert_eq!(next.epoch, key.epoch);
    let result = app.in_tab(id, |app| compute(app)).unwrap();
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::LabelsReady(next, result)),
    ));
    front.assert_unchanged(&app);
    assert_eq!(app.tabs.background[0].doc.atom(oxygen).unwrap().label_h, 2);
    assert!(app.tabs.background[0].label_refresh.pending.is_none());
}

#[test]
fn oxygen_starts_immediately_and_labels_do_not_edit_history_or_selection() {
    let (mut app, oxygen) = fixture();
    let task = app.update(Message::ContextKey("o".into()));
    assert!(task.units() > 0, "No timer before starting chemistry");
    assert!(!app.tab.busy, "Labels must not disable editing commands");
    let key = app.tab.label_refresh.pending.unwrap();
    let drawing = app.tab.doc.clone();
    app.tab.saved = drawing.clone();
    let selected = app.tab.selected.clone();
    app.edit(crate::canvas::Edit::ContextMenu {
        position: iced::Point::ORIGIN,
        hit: vec![],
        selected: selected.clone(),
    });
    let labels = compute(&app);
    let _ = app.update(Message::LabelsReady(key, labels));
    assert!(
        app.context_menu.is_some(),
        "Background labels leave menus open"
    );
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().label_h, 1);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.revision, key.revision);
    assert!(same_drawing(&drawing, &app.tab.doc));
    assert!(!app.dirty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().element, "C");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().label_h, 1);
}

#[test]
fn rapid_edits_coalesce_and_stale_success_and_errors_cannot_publish() {
    for failure in [false, true] {
        let (mut app, id) = fixture();
        let _ = app.update(Message::ContextKey("o".into()));
        let old = app.tab.label_refresh.pending.unwrap();
        let result = if failure {
            Err("stale error".into())
        } else {
            compute(&app)
        };
        let _ = app.update(Message::ContextKey("n".into()));
        let _ = app.update(Message::ContextKey("s".into()));
        assert_eq!(app.tab.label_refresh.pending, Some(old));
        let latest = app.tab.revision;
        let _ = app.update(Message::LabelsReady(old, result));
        assert_eq!(app.tab.doc.atom(id).unwrap().element, "S");
        assert_eq!(app.tab.doc.atom(id).unwrap().label_h, 0);
        assert!(app.tab.chemistry_notice.is_none());
        let key = app.tab.label_refresh.pending.unwrap();
        assert_eq!(key.revision, latest);
        let result = compute(&app);
        let _ = app.update(Message::LabelsReady(key, result));
        assert_eq!(app.tab.doc.atom(id).unwrap().label_h, 1);
        assert!(app.tab.label_refresh.pending.is_none());
    }
}

#[test]
fn document_replacement_rejects_old_labels_even_with_reused_ids_and_revision() {
    let (mut app, _) = fixture();
    let _ = app.update(Message::ContextKey("o".into()));
    let old = app.tab.label_refresh.pending.unwrap();
    let result = compute(&app);
    let _ = app.update(Message::New);
    let id = app.tab.doc.add_atom("N", Point::default());
    app.tab.revision = old.revision;
    let _ = app.update(Message::LabelsReady(old, result));
    assert_eq!(app.tab.doc.atom(id).unwrap().label_h, 0);
    assert!(app.tab.chemistry_notice.is_none());
}

/// C=O with a curved arrow attached at O, settled with its labels, saved.
fn carbonyl_with_arrow() -> (App, u64) {
    use reshiki::arrow_anchors::{Anchor, Target};
    use reshiki::arrows::{ArrowStyle, Preset};
    use reshiki::document::Arrow;
    let (mut app, _) = App::new();
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::new(-42., 0.));
    let oxygen = doc.add_atom("O", Point::default());
    doc.add_bond(carbon, oxygen, 2, "plain");
    let target = doc.add_atom("C", Point::new(200., -140.));
    let mut arrow = Arrow::new(
        doc.next_id(),
        Point::default(),
        Point::new(200., -140.),
        Preset::Curved,
        ArrowStyle::preset(Preset::Curved),
    );
    arrow.start_anchor = Some(Anchor {
        target: Target::Atom { atom: oxygen },
        offset: Point::default(),
        direction: Point::new(0.75, -0.66),
        gap: 0.,
    });
    arrow.end_anchor = Some(Anchor {
        target: Target::Atom { atom: target },
        offset: Point::default(),
        direction: Point::new(-0.75, 0.66),
        gap: 0.,
    });
    doc.arrows.push(arrow);
    Refresh::calculate(&doc, &Refresh::default())
        .unwrap()
        .apply(&mut doc);
    let _ = reshiki::transaction::reconcile(&mut doc, Document::default()).unwrap();
    assert_eq!(doc.atom(oxygen).unwrap().label_h, 0);
    app.tab.doc = doc.clone();
    app.tab.saved = doc;
    (app, oxygen)
}

/// What the next edit's reconcile used to make of the drawing.
fn reconciled(doc: &Document) -> Document {
    let mut expected = doc.clone();
    let _ = reshiki::transaction::reconcile(&mut expected, doc.clone()).unwrap();
    expected
}

/// Changes C=O to C–O as an edit and returns the labels request it starts.
fn single_bond(app: &mut App) -> Key {
    let before = app.tab.doc.clone();
    app.tab.doc.bonds[0].order = 1;
    app.changed(before);
    let _ = app.start_label_refresh();
    app.tab.label_refresh.pending.unwrap()
}

#[test]
fn new_hydrogens_move_attached_arrows_clear_as_the_labels_land() {
    let (mut app, oxygen) = carbonyl_with_arrow();
    let original = app.tab.doc.clone();
    let key = single_bond(&mut app);
    let stale = app.tab.doc.arrows[0].start;
    let frames = app.tab.history.undo_frames().count();
    let labels = compute(&app);
    let mut landed = app.tab.doc.clone();
    labels.as_ref().unwrap().apply(&mut landed);
    let _ = app.update(Message::LabelsReady(key, labels));
    // No pan or edit: the labels themselves move the arrow clear of the H.
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().label_h, 1);
    assert_eq!(app.tab.doc, reconciled(&landed));
    assert!(app.tab.doc.arrows[0].start.distance(stale) > 1.);
    assert_eq!(app.tab.revision, key.revision.wrapping_add(1));
    assert_eq!(app.tab.history.undo_frames().count(), frames);
    assert!(app.dirty());
    let repaired = app.tab.doc.clone();
    // The repair belongs to the edit: one Undo restores the drawing before it.
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    assert!(!app.dirty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.arrows, repaired.arrows);
    assert!(!app.tab.history.can_redo());
}

#[test]
fn a_linked_caption_moves_clear_of_stereo_labels_as_they_land() {
    let (mut app, _) = App::new();
    let mut doc = Document::default();
    doc.atom_labels.stereo = true;
    // (R)/(S)-butan-2-ol with its stereocenter at the bottom.
    let center = doc.add_atom("C", Point::default());
    let methyl = doc.add_atom("C", Point::new(-36., -21.));
    let ethyl = doc.add_atom("C", Point::new(36., -21.));
    let end = doc.add_atom("C", Point::new(72., -42.));
    let oxygen = doc.add_atom("O", Point::new(0., -42.));
    doc.add_bond(center, methyl, 1, "plain");
    doc.add_bond(center, ethyl, 1, "plain");
    doc.add_bond(ethyl, end, 1, "plain");
    doc.add_bond(center, oxygen, 1, "wedge");
    let atoms = vec![center, methyl, ethyl, end, oxygen];
    let smiles = reshiki::naming::document_identity(&doc).unwrap().smiles;
    reshiki::molecule_names::show(
        &mut doc,
        &atoms,
        "butan-2-ol".into(),
        smiles,
        Default::default(),
    )
    .unwrap();
    app.tab.doc = doc;
    // Move the molecule before its stereo labels are known, as an edit made
    // right after opening a large drawing would.
    let before = app.tab.doc.clone();
    app.tab.doc.translate(&atoms, 10., 0.);
    app.changed(before);
    let caption = app.tab.doc.annotations[0].position;
    let _ = app.start_label_refresh();
    let key = app.tab.label_refresh.pending.unwrap();
    let labels = compute(&app);
    let mut landed = app.tab.doc.clone();
    labels.as_ref().unwrap().apply(&mut landed);
    assert!(landed.atom(center).unwrap().cip_label.is_some());
    let frames = app.tab.history.undo_frames().count();
    let _ = app.update(Message::LabelsReady(key, labels));
    assert_eq!(app.tab.doc, reconciled(&landed));
    assert_eq!(
        app.tab.doc.molecule_names.len(),
        1,
        "The caption stays linked"
    );
    assert!(app.tab.doc.annotations[0].position.y > caption.y + 1.);
    assert_eq!(app.tab.history.undo_frames().count(), frames);
}

#[test]
fn labels_for_a_drawing_that_undo_replaced_are_not_applied() {
    let (mut app, oxygen) = carbonyl_with_arrow();
    let key = single_bond(&mut app);
    let labels = compute(&app);
    let _ = app.update(Message::Undo);
    let (doc, revision) = (app.tab.doc.clone(), app.tab.revision);
    let _ = app.update(Message::LabelsReady(key, labels));
    assert_eq!(app.tab.doc, doc);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().label_h, 0);
    assert!(!app.dirty());
}

#[test]
fn opening_a_drawing_whose_arrow_overlaps_new_hydrogens_leaves_it_saved() {
    let (mut app, oxygen) = carbonyl_with_arrow();
    // A file whose arrow was drawn before its O gained an H.
    let mut file = app.tab.doc.clone();
    file.bonds[0].order = 1;
    app.show_opened(file.clone(), Some("old.rsk".into()));
    let _ = app.start_label_refresh();
    let key = app.tab.label_refresh.pending.unwrap();
    let _ = app.update(Message::LabelsReady(key, compute(&app)));
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().label_h, 1);
    assert_ne!(app.tab.doc.arrows, file.arrows, "The arrow moved clear");
    assert!(!app.dirty(), "Opening alone leaves nothing to save");
    assert!(!app.tab.history.can_undo());
}

#[test]
fn a_save_written_while_labels_landed_leaves_the_drawing_saved() {
    let (mut app, _) = carbonyl_with_arrow();
    let key = single_bond(&mut app);
    // Save starts before the labels land; its write, and even its
    // completion message, finish before they land too.
    let snapshot = Arc::new(app.tab.doc.clone());
    app.saving(&snapshot);
    let written = Arc::unwrap_or_clone(snapshot);
    let _ = app.update(Message::LabelsReady(key, compute(&app)));
    assert!(app.dirty());
    let _ = app.file_saved(
        app.tab.file_epoch,
        Box::new(written),
        Ok(Some("a.rsk".into())),
    );
    assert!(!app.dirty());
}

#[test]
fn undoing_display_edits_made_before_the_labels_landed_leaves_the_drawing_saved() {
    let (mut app, _) = carbonyl_with_arrow();
    let mut file = app.tab.doc.clone();
    file.bonds[0].order = 1;
    app.show_opened(file, Some("old.rsk".into()));
    // Two display-only edits that cancel out, before any labels land.
    for hide in [true, false] {
        let before = app.tab.doc.clone();
        app.tab.doc.atoms[2].display.hide_charge = hide;
        app.changed(before);
    }
    assert!(!app.dirty());
    let _ = app.start_label_refresh();
    let key = app.tab.label_refresh.pending.unwrap();
    let _ = app.update(Message::LabelsReady(key, compute(&app)));
    assert!(!app.dirty());
    let repaired = app.tab.doc.arrows.clone();
    let _ = app.update(Message::Undo);
    assert!(app.dirty(), "The hidden charge is not saved");
    let _ = app.update(Message::Undo);
    // The opened snapshot gets the labels and the layout they need.
    assert!(!app.tab.history.can_undo());
    assert_eq!(app.tab.doc.arrows, repaired);
    assert!(!app.dirty());
}

#[test]
fn labels_that_change_no_layout_change_nothing_else() {
    let (mut app, oxygen) = fixture();
    app.tab.saved = app.tab.doc.clone();
    let _ = app.update(Message::ContextKey("o".into()));
    let key = app.tab.label_refresh.pending.unwrap();
    let labels = compute(&app);
    let mut landed = app.tab.doc.clone();
    labels.as_ref().unwrap().apply(&mut landed);
    let saved = app.tab.saved.clone();
    let _ = app.update(Message::LabelsReady(key, labels));
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().label_h, 1);
    assert_eq!(app.tab.doc, landed);
    assert_eq!(app.tab.revision, key.revision);
    assert_eq!(app.tab.saved, saved);
}

#[test]
fn move_and_attach_keeps_its_drawing_until_it_closes() {
    let (mut app, _) = carbonyl_with_arrow();
    let key = single_bond(&mut app);
    let stale = app.tab.doc.arrows.clone();
    app.tab.selected = vec![app.tab.doc.atoms[2].id];
    let _ = app.update(Message::Join(crate::app::joining::Action::Begin));
    assert!(app.tab.joining.is_some());
    let _ = app.update(Message::LabelsReady(key, compute(&app)));
    // Its preview belongs to this revision, so the repair waits.
    assert!(app.tab.joining.is_some());
    assert_eq!(app.tab.doc.arrows, stale);
    assert_eq!(app.tab.revision, key.revision);
    let frames = app.tab.history.undo_frames().count();
    let _ = app.update(Message::Escape);
    assert!(app.tab.joining.is_none());
    assert_eq!(app.tab.doc, reconciled(&app.tab.doc));
    assert_ne!(app.tab.doc.arrows, stale);
    assert_eq!(app.tab.history.undo_frames().count(), frames);
}

#[test]
fn labels_for_the_opened_gallery_change_no_layout() {
    let gallery: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    let (mut app, _) = App::new();
    app.show_opened(gallery, Some("shortcut-examples.rsk".into()));
    let _ = app.start_label_refresh();
    let key = app.tab.label_refresh.pending.unwrap();
    let labels = compute(&app);
    let mut landed = app.tab.doc.clone();
    labels.as_ref().unwrap().apply(&mut landed);
    let _ = app.update(Message::LabelsReady(key, labels));
    assert_eq!(app.tab.doc, landed);
    assert_eq!(app.tab.revision, key.revision);
    assert!(!app.dirty());
}
