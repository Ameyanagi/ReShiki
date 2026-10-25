//! Characterizes `App::changed`: what an edit commits or rolls back, the order of
//! its reconciliation stages, and what undo and redo restore afterwards.
//!
//! Edits go only through `changed` / `changed_continuing` and history only through
//! `step_history`; routed `update()` adds label refresh, autosave and modal gates.
use super::*;
use reshiki::palette::{Color, Hue, Row};

const CHEMISTRY: &str = "Drawing changed · Check structure to refresh properties";
const DISPLAY: &str = "Drawing updated";
const RESTORED: &str = "History restored";
const JOINS: &str =
    "This joins separate reaction participants. Clear their reaction roles before joining them.";

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app.tab.busy = false;
    app
}

fn analysis() -> reshiki::engine::Analysis {
    reshiki::engine::Analysis {
        smiles: "CC".into(),
        formula: "C2H6".into(),
        mass: 30.07,
        exact_mass: 30.047,
        logp: 1.02,
        tpsa: 0.,
        donors: 0,
        acceptors: 0,
        rings: 0,
        unpaired_electrons: 0,
        inchi: "InChI=1S/C2H6/c1-2/h1-2H3".into(),
        inchikey: "OTMSDBZUPAUEDD-UHFFFAOYSA-N".into(),
    }
}

fn preview(app: &App) -> CleanupPreview {
    CleanupPreview {
        job: cleanup::CleanupJob {
            options: Default::default(),
            selection: vec![],
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        },
        warnings: vec![],
        document: app.tab.doc.clone(),
        analysis: None,
        revision: app.tab.revision,
        epoch: app.tab.file_epoch,
        original: false,
    }
}

/// Two carbons joined by one bond of `order`.
fn ethane(app: &mut App, order: u8) -> [u64; 2] {
    let doc = &mut app.tab.doc;
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a, b, order, "plain");
    [a, b]
}

/// Cached properties and UI state that only a chemistry edit discards.
fn cache_properties(app: &mut App) {
    app.error = true;
    app.tab.analysis = Some(analysis());
    app.tab.labels_dirty = false;
    app.tab.chemistry_notice = Some("n".into());
    let cleanup = preview(app);
    app.tab.cleanup = Some(cleanup);
}

/// Reactant a–b and product c on one arrow, as in `app/reactions/tests.rs`.
fn reaction(app: &mut App) -> [u64; 3] {
    let doc = &mut app.tab.doc;
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("O", Point::new(42., 0.));
    doc.add_bond(a, b, 1, "plain");
    let c = doc.add_atom("C", Point::new(300., 0.));
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(100., 0.),
        Point::new(240., 0.),
        Default::default(),
        Default::default(),
    ));
    reshiki::reactions::assign(doc, arrow, &[a], reshiki::reactions::Role::Reactant).unwrap();
    reshiki::reactions::assign(doc, arrow, &[c], reshiki::reactions::Role::Product).unwrap();
    assert_eq!(doc.reactions[0].reactants[0].atoms, [a, b]);
    assert_eq!(doc.reactions[0].products[0].atoms, [c]);
    app.tab.saved = app.tab.doc.clone();
    [a, b, c]
}

/// The ether a–b–c of `tests/abbreviations.rs` with b–c contracted to OMe.
/// The anchor is b; c is the hidden member.
fn methoxy(app: &mut App) -> [u64; 3] {
    let doc = &mut app.tab.doc;
    let a = doc.add_atom("C", Point::new(0., 0.));
    let b = doc.add_atom("O", Point::new(42., 0.));
    let c = doc.add_atom("C", Point::new(63., 36.373));
    doc.add_bond(a, b, 1, "plain");
    doc.add_bond(b, c, 1, "plain");
    doc.contract(&[b, c], "OMe", "MeO").unwrap();
    assert_eq!(doc.abbreviations.len(), 1);
    assert_eq!(doc.abbreviations[0].anchor, b);
    assert_eq!(doc.abbreviations[0].members, [b, c]);
    [a, b, c]
}

#[test]
fn display_only_edit_keeps_cached_properties_and_undo_is_exact() {
    let mut app = app();
    let [a, _] = ethane(&mut app, 1);
    cache_properties(&mut app);
    app.tab.doc.atom_mut(a).unwrap().cip_label = Some("R".into());
    let revision = app.tab.revision;
    let before = app.tab.doc.clone();
    assert!(app.tab.cleanup.is_some());
    assert!(!app.tab.history.can_undo());

    app.tab.doc.bonds[0].color = Color::Palette(Hue::Blue, Row::Strong);
    app.changed(before.clone());
    assert_eq!(app.status, DISPLAY);
    assert!(!app.error);
    assert_eq!(app.tab.revision, revision + 1);
    assert!(app.tab.history.can_undo());
    assert!(app.tab.analysis.is_some());
    assert!(!app.tab.labels_dirty);
    assert_eq!(app.tab.chemistry_notice.as_deref(), Some("n"));
    assert!(app.tab.cleanup.is_none());
    assert_eq!(app.tab.doc.atom(a).unwrap().cip_label.as_deref(), Some("R"));

    app.error = true;
    app.step_history(false);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.doc.atom(a).unwrap().cip_label.as_deref(), Some("R"));
    assert_eq!(app.status, RESTORED);
    assert!(!app.error);
    assert_eq!(app.tab.revision, revision + 2);
    assert!(app.tab.analysis.is_some());
}

#[test]
fn chemistry_edits_clear_computed_labels_and_cached_properties() {
    for translate in [false, true] {
        let context = if translate { "translate" } else { "element" };
        let mut app = app();
        let [a, _] = ethane(&mut app, 2);
        cache_properties(&mut app);
        app.tab.doc.atom_mut(a).unwrap().cip_label = Some("R".into());
        app.tab.doc.bonds[0].cip_label = Some("E".into());
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();
        assert!(app.tab.cleanup.is_some(), "{context}");
        assert!(app.tab.analysis.is_some(), "{context}");

        if translate {
            // A move is a chemistry edit: chemistry_changed keeps atom positions.
            app.tab.doc.translate(&[a], 10., 0.);
        } else {
            app.tab.doc.atom_mut(a).unwrap().element = "N".into();
        }
        assert_eq!(
            app.tab.doc.atom(a).unwrap().cip_label.as_deref(),
            Some("R"),
            "{context}"
        );
        app.changed(before.clone());
        let doc = &app.tab.doc;
        assert!(doc.atoms.iter().all(|t| t.cip_label.is_none()), "{context}");
        assert!(doc.bonds.iter().all(|t| t.cip_label.is_none()), "{context}");
        assert!(app.tab.analysis.is_none(), "{context}");
        assert!(app.tab.labels_dirty, "{context}");
        assert!(app.tab.chemistry_notice.is_none(), "{context}");
        assert_eq!(app.status, CHEMISTRY, "{context}");
        assert!(!app.error, "{context}");
        assert_eq!(app.tab.revision, revision + 1, "{context}");
        assert!(app.tab.cleanup.is_none(), "{context}");
        let committed = app.tab.doc.clone();

        // History keeps the labeled `before`; undo clears its labels again.
        app.tab.labels_dirty = false;
        app.tab.analysis = Some(analysis());
        app.step_history(false);
        let mut cleared = before.clone();
        reshiki::atom_labels::clear_computed(&mut cleared);
        assert_ne!(cleared, before, "{context}");
        assert_eq!(app.tab.doc, cleared, "{context}");
        assert_eq!(app.status, RESTORED, "{context}");
        assert!(app.tab.labels_dirty, "{context}");
        assert!(app.tab.analysis.is_none(), "{context}");

        app.step_history(true);
        assert_eq!(app.tab.doc, committed, "{context}");
        assert_eq!(app.status, RESTORED, "{context}");
    }
}

#[test]
fn unchanged_document_neither_commits_nor_touches_status() {
    let mut app = app();
    let existing = app.tab.doc.add_atom("C", Point::default());
    app.status = "sentinel".into();
    app.error = true;
    app.tab.cleanup = Some(preview(&app));
    app.tab.erase_stroke = true;
    app.tab.selected = vec![existing, 999_999];
    let revision = app.tab.revision;
    let before = app.tab.doc.clone();

    app.changed(app.tab.doc.clone());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.status, "sentinel");
    assert!(app.error);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.cleanup.is_none());
    assert!(app.tab.erase_stroke);
    assert_eq!(app.tab.selected, [existing]);
}

#[test]
fn invalid_edit_rolls_back_with_edit_cancelled_status() {
    let mut app = app();
    let [a, _] = ethane(&mut app, 1);
    app.tab.cleanup = Some(preview(&app));
    app.tab.erase_stroke = true;
    app.tab.selected = vec![a, 999_999];
    let revision = app.tab.revision;
    let before = app.tab.doc.clone();

    app.tab.doc.bonds[0].order = 8;
    app.changed(before.clone());
    assert_eq!(app.tab.doc, before);
    assert!(app.error);
    assert_eq!(
        app.status,
        "Edit cancelled: Invalid bond endpoints or order"
    );
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.cleanup.is_none());
    // changed() ends the erase stroke before the edit is rejected.
    assert!(!app.tab.erase_stroke);
    // A rejection returns before the selection is pruned.
    assert_eq!(app.tab.selected, [a, 999_999]);
}

#[test]
fn reaction_join_rolls_back_with_unprefixed_reaction_message() {
    // Order 1 is the plain join. Order 8 is also invalid, but reactions::molecules
    // counts every bond with order != 0, so reaction reconciliation rejects the
    // join before validation can report the bond.
    for order in [1, 8] {
        let mut app = app();
        let [_, b, c] = reaction(&mut app);
        app.tab.cleanup = Some(preview(&app));
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();

        app.tab.doc.add_bond(b, c, order, "plain");
        assert_eq!(app.tab.doc.bonds.len(), 2, "order {order}");
        app.changed(before.clone());
        assert_eq!(app.status, JOINS, "order {order}");
        assert!(app.error, "order {order}");
        assert_eq!(app.tab.doc, before, "order {order}");
        assert_eq!(app.tab.revision, revision, "order {order}");
        assert!(!app.tab.history.can_undo(), "order {order}");
        assert!(app.tab.cleanup.is_none(), "order {order}");
    }
}

#[test]
fn unchanged_invalid_document_is_not_revalidated() {
    let mut app = app();
    ethane(&mut app, 1);
    app.tab.doc.bonds[0].order = 8;
    assert!(app.tab.doc.validate().is_err());
    app.status = "sentinel".into();
    app.error = false;
    let revision = app.tab.revision;
    let before = app.tab.doc.clone();

    app.changed(before.clone());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.status, "sentinel");
    assert!(!app.error);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn reaction_participant_grows_with_its_molecule() {
    let mut app = app();
    let [a, b, c] = reaction(&mut app);
    let revision = app.tab.revision;
    let before = app.tab.doc.clone();

    let d = app.tab.doc.add_atom("C", Point::new(42., 42.));
    app.tab.doc.add_bond(b, d, 1, "plain");
    assert_eq!(app.tab.doc.reactions[0].reactants[0].atoms, [a, b]);
    app.changed(before);
    let mut grown = vec![a, b, d];
    grown.sort_unstable();
    assert_eq!(app.tab.doc.reactions[0].reactants[0].atoms, grown);
    assert_eq!(app.tab.doc.reactions[0].products[0].atoms, [c]);
    assert_eq!(app.status, CHEMISTRY);
    assert!(!app.error);
    assert_eq!(app.tab.revision, revision + 1);
    assert!(app.tab.history.can_undo());
}

#[test]
fn abbreviations_reconcile_before_validation() {
    // (a) Moving the abbreviation keeps it; the move is still a chemistry edit.
    {
        let mut app = app();
        let [_, b, c] = methoxy(&mut app);
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();
        app.tab.doc.translate(&[b], 5., 0.);
        assert_eq!(app.tab.doc.atom(c).unwrap().position.x, 68.);
        app.changed(before.clone());
        assert_eq!(app.tab.doc.abbreviations, before.abbreviations);
        assert_eq!(app.status, CHEMISTRY);
        assert!(!app.error);
        assert_eq!(app.tab.revision, revision + 1);
    }
    // (b) Changing a member's element drops the abbreviation; undo restores it.
    {
        let mut app = app();
        let [_, _, c] = methoxy(&mut app);
        let before = app.tab.doc.clone();
        app.tab.doc.atom_mut(c).unwrap().element = "N".into();
        app.changed(before.clone());
        assert!(app.tab.doc.abbreviations.is_empty());
        assert_eq!(app.status, CHEMISTRY);
        assert!(!app.error);
        app.step_history(false);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.doc.abbreviations.len(), 1);
        assert_eq!(app.status, RESTORED);
    }
    // (c) Bonding an outside atom to the hidden member breaks the abbreviation.
    // Reconciliation drops it before validation, which would reject it.
    {
        let mut app = app();
        let [_, _, c] = methoxy(&mut app);
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();
        let d = app.tab.doc.add_atom("C", Point::new(84., 0.));
        app.tab.doc.add_bond(c, d, 1, "plain");
        assert_eq!(
            app.tab.doc.validate(),
            Err("Only the abbreviation's attachment atom can connect outside it".into())
        );
        app.changed(before);
        assert!(app.tab.doc.abbreviations.is_empty());
        assert!(!app.error);
        assert_ne!(
            app.status,
            "Edit cancelled: Only the abbreviation's attachment atom can connect outside it"
        );
        assert_eq!(app.status, CHEMISTRY);
        assert_eq!(app.tab.revision, revision + 1);
        assert!(app.tab.history.can_undo());
    }
}

#[test]
fn molecule_groups_reconcile_after_validation() {
    // (a) A bond grows the group with the newly connected atom.
    {
        let mut app = app();
        let doc = &mut app.tab.doc;
        let a = doc.add_atom("C", Point::default());
        let b = doc.add_atom("C", Point::new(42., 0.));
        doc.add_bond(a, b, 1, "plain");
        let c = doc.add_atom("C", Point::new(84., 0.));
        doc.group_selection(&[a, b]).unwrap();
        assert!(!doc.groups[0].members.contains(&c));
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();
        app.tab.doc.add_bond(b, c, 1, "plain");
        app.changed(before);
        assert!(app.tab.doc.groups[0].members.contains(&c));
        assert!(!app.error);
        assert_eq!(app.tab.revision, revision + 1);
        assert!(app.tab.history.can_undo());
    }
    // (b) Validation runs before reconcile_molecule_groups, whose prune_groups
    // would otherwise repair the stale member.
    {
        let mut app = app();
        let doc = &mut app.tab.doc;
        let x = doc.add_atom("C", Point::default());
        let y = doc.add_atom("C", Point::new(42., 0.));
        let z = doc.add_atom("C", Point::new(84., 0.));
        doc.group_selection(&[x, y, z]).unwrap();
        let groups = doc.groups.clone();
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();
        app.tab.doc.atoms.retain(|t| t.id != z);
        assert!(app.tab.doc.groups[0].members.contains(&z));
        app.changed(before.clone());
        assert_eq!(app.status, "Edit cancelled: Invalid group ID or membership");
        assert!(app.error);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.doc.groups, groups);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn centroids_ring_fills_and_depth_scopes_follow_raw_edits() {
    // Raw mutations only: Document::delete prunes and translate syncs centroids.
    // (a) A centroid follows its moved member.
    {
        let mut app = app();
        let p = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let q = app.tab.doc.add_atom("C", Point::new(40., 0.));
        let centroid = reshiki::projection::add_centroid(&mut app.tab.doc, &[p, q]).unwrap();
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();
        app.tab.doc.atom_mut(q).unwrap().position.x = 80.;
        assert_eq!(
            app.tab.doc.atom(centroid).unwrap().position,
            Point::new(20., 0.)
        );
        app.changed(before);
        let atom = app.tab.doc.atom(centroid).unwrap();
        assert_eq!(atom.position, Point::new(40., 0.));
        assert_eq!(atom.depth, 0.);
        assert_eq!(app.status, CHEMISTRY);
        assert!(!app.error);
        assert_eq!(app.tab.revision, revision + 1);
    }
    // (b) A ring fill whose cycle lost a bond is pruned, not rejected.
    {
        let mut app = app();
        app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
        let ids = app.tab.doc.all_ids();
        let painted = reshiki::ring_fills::apply(
            &mut app.tab.doc,
            &ids,
            Some(Color::Palette(Hue::Blue, Row::Strong)),
        );
        assert_eq!(painted, 1);
        assert_eq!(app.tab.doc.ring_fills.len(), 1);
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();
        app.tab.doc.bonds.remove(0);
        assert_eq!(app.tab.doc.ring_fills.len(), 1);
        assert!(reshiki::ring_fills::validate(&app.tab.doc).is_err());
        app.changed(before);
        assert!(app.tab.doc.ring_fills.is_empty());
        assert!(!app.error);
        assert_eq!(app.tab.revision, revision + 1);
        assert!(app.tab.history.can_undo());
    }
    // (c) A depth scope over removed atoms is pruned.
    {
        let mut app = app();
        let doc = &mut app.tab.doc;
        let first = [
            doc.add_atom("C", Point::new(0., 0.)),
            doc.add_atom("C", Point::new(42., 0.)),
        ];
        doc.add_bond(first[0], first[1], 1, "plain");
        let second = [
            doc.add_atom("C", Point::new(0., 100.)),
            doc.add_atom("C", Point::new(42., 100.)),
        ];
        doc.add_bond(second[0], second[1], 1, "plain");
        assert_eq!(
            reshiki::depth_appearance::enable(doc, &first, 0.5).unwrap(),
            1
        );
        assert_eq!(doc.depth_appearance.len(), 1);
        let revision = app.tab.revision;
        let before = app.tab.doc.clone();
        app.tab.doc.atoms.retain(|a| !first.contains(&a.id));
        app.tab
            .doc
            .bonds
            .retain(|b| !first.contains(&b.a) && !first.contains(&b.b));
        assert_eq!(app.tab.doc.depth_appearance.len(), 1);
        app.changed(before);
        assert!(app.tab.doc.depth_appearance.is_empty());
        assert!(!app.error);
        assert_eq!(app.tab.revision, revision + 1);
        assert!(app.tab.history.can_undo());
    }
}

#[test]
fn continuing_gesture_is_one_undo_step() {
    let mut app = app();
    let a = app.tab.doc.add_atom("C", Point::default());
    let initial = app.tab.doc.clone();
    let revision = app.tab.revision;

    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(a).unwrap().position.y += 10.;
    app.changed(before);
    let first = app.tab.doc.clone();
    for continuing in [false, true, true] {
        let before = app.tab.doc.clone();
        app.tab.doc.atom_mut(a).unwrap().position.x += 1.;
        app.changed_continuing(before, continuing);
    }
    assert_eq!(app.tab.revision, revision + 4);

    app.step_history(false);
    assert_eq!(app.tab.doc, first);
    app.step_history(false);
    assert_eq!(app.tab.doc, initial);
    assert!(!app.tab.history.can_undo());

    app.step_history(true);
    assert_eq!(app.tab.doc, first);
    assert!(app.tab.history.can_redo());
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(a).unwrap().position.y += 10.;
    app.changed(before);
    assert!(!app.tab.history.can_redo());
}

#[test]
fn rejected_continuing_frame_restores_previous_frame_not_gesture_start() {
    let mut app = app();
    let a = app.tab.doc.add_atom("C", Point::default());
    let start = app.tab.doc.clone();

    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(a).unwrap().position.x += 5.;
    app.changed_continuing(before, false);
    let frame = app.tab.doc.clone();
    let revision = app.tab.revision;

    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(a).unwrap().position.x = f32::NAN;
    app.changed_continuing(before, true);
    assert_eq!(app.tab.doc, frame);
    assert!(app.error);
    assert_eq!(app.status, "Edit cancelled: Non-finite atom position");
    assert_eq!(app.tab.revision, revision);

    app.step_history(false);
    assert_eq!(app.tab.doc, start);
}

#[test]
fn drawing_style_change_resyncs_drawing_defaults() {
    let mut app = app();
    let [a, _] = ethane(&mut app, 1);
    let style = reshiki::document_styles::Preset::ALL
        .into_iter()
        .map(|preset| preset.style())
        .find(|style| *style != app.tab.doc.drawing_style)
        .unwrap();
    app.tab.caption_target = Some(a);
    app.tab.drawing_length_input = String::new();
    app.tab.graphic_width_input = String::new();

    let before = app.tab.doc.clone();
    app.tab.doc.drawing_style = style.clone();
    app.changed(before);
    assert_eq!(app.status, DISPLAY);
    assert!(!app.error);
    assert_eq!(app.tab.doc.drawing_style, style);
    assert_eq!(
        app.tab.drawing_length_input,
        style.bond_length_pt.to_string()
    );
    assert_eq!(app.tab.graphic_width_input, style.line_width_pt.to_string());
    assert_eq!(app.tab.caption_target, None);

    // An edit that keeps the drawing style leaves the caption target alone.
    app.tab.caption_target = Some(a);
    let before = app.tab.doc.clone();
    app.tab.doc.bonds[0].color = Color::Palette(Hue::Blue, Row::Strong);
    app.changed(before);
    assert_eq!(app.status, DISPLAY);
    assert_eq!(app.tab.caption_target, Some(a));
}

#[test]
fn step_history_restores_status_revision_and_selection() {
    // (a) Undo and redo each restore a frame and filter the selection.
    {
        let mut app = app();
        let before = app.tab.doc.clone();
        let a = app.tab.doc.add_atom("C", Point::default());
        app.changed(before);
        let before = app.tab.doc.clone();
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.changed(before);
        assert_eq!(app.tab.caption_target, None);
        for (redo, selected, kept) in [
            (false, vec![a, b], vec![a]),
            (true, vec![a, b, 999_999], vec![a, b]),
        ] {
            app.tab.selected = selected;
            app.status = "sentinel".into();
            app.error = true;
            let revision = app.tab.revision;
            app.step_history(redo);
            assert_eq!(app.status, RESTORED, "redo {redo}");
            assert!(!app.error, "redo {redo}");
            assert_eq!(app.tab.revision, revision + 1, "redo {redo}");
            assert_eq!(app.tab.selected, kept, "redo {redo}");
        }
    }
    // (b) Undo with empty history still clears the stroke and cleanup preview.
    {
        let mut app = app();
        app.tab.doc.add_atom("C", Point::default());
        app.tab.erase_stroke = true;
        app.tab.cleanup = Some(preview(&app));
        app.status = "sentinel".into();
        app.error = true;
        app.tab.selected = vec![999];
        let revision = app.tab.revision;
        let doc = app.tab.doc.clone();
        assert!(!app.tab.history.can_undo());
        app.step_history(false);
        assert!(!app.tab.erase_stroke);
        assert!(app.tab.cleanup.is_none());
        assert_eq!(app.status, "sentinel");
        assert!(app.error);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, [999]);
        assert_eq!(app.tab.doc, doc);
    }
}

#[test]
fn observer_history_alignment_survives_cip_only_frames() {
    use reshiki::keyboard_drawing::Target;
    let mut app = app();
    let doc = &mut app.tab.doc;
    let a1 = doc.add_atom("C", Point::new(0., 0.));
    let a2 = doc.add_atom("C", Point::new(42., 0.));
    doc.add_bond(a1, a2, 1, "plain");
    let b1 = doc.add_atom("C", Point::new(0., 100.));
    let b2 = doc.add_atom("C", Point::new(42., 100.));
    doc.add_bond(b1, b2, 1, "plain");

    // E1 moves chain A.
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(a1).unwrap().position.y -= 10.;
    app.changed(before);
    assert_eq!(app.status, CHEMISTRY);
    app.tab
        .keyboard_drawing
        .set_target(Target::Atom(b1), &app.tab.doc);

    // E2 changes only a CIP label: a display frame that History still records.
    app.tab.labels_dirty = false;
    let revision = app.tab.revision;
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(a1).unwrap().cip_label = Some("R".into());
    app.changed(before);
    assert_eq!(app.status, DISPLAY);
    assert!(app.tab.history.can_undo());
    assert!(!app.tab.labels_dirty);
    assert_eq!(app.tab.revision, revision + 1);
    app.tab
        .keyboard_drawing
        .set_target(Target::Atom(a1), &app.tab.doc);

    // E3 moves chain B.
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(b1).unwrap().position.y += 10.;
    app.changed(before);

    // Clear the selection before each probe: an empty recent context leaves it
    // untouched, so a stale [a1, a2] would otherwise hide a misaligned frame.
    const SELECTED: &str = "Selected the most recently edited molecule(s)";
    app.step_history(false);
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(a1));
    app.tab.selected.clear();
    app.select_recent_shortcut();
    assert_eq!(app.status, SELECTED);
    assert_eq!(app.tab.selected, [a1, a2]);

    app.step_history(false);
    assert_eq!(app.tab.keyboard_drawing.target(), Target::Atom(b1));
    app.tab.selected.clear();
    app.select_recent_shortcut();
    assert_eq!(app.status, SELECTED);
    assert_eq!(app.tab.selected, [a1, a2]);
}

#[test]
fn generic_repeat_define_clear_undo_redo_invalidate_cached_properties_and_notices() {
    let mut app = app();
    for (index, element) in ["O", "C", "C", "N"].into_iter().enumerate() {
        let id = app
            .tab
            .doc
            .add_atom(element, Point::new(index as f32 * 42., 0.));
        if index > 0 {
            app.tab.doc.add_bond(id - 1, id, 1, "plain");
        }
    }
    let bracket = app.tab.doc.next_id();
    app.tab
        .doc
        .graphics
        .push(reshiki::graphics::Graphic::dragged(
            bracket,
            reshiki::graphics::GraphicKind::Brackets,
            Point::new(20., -20.),
            Point::new(105., 20.),
            Default::default(),
            reshiki::graphics::BracketSides::Both,
            false,
        ));
    let _ = app.generic_action(generic::Action::RepeatLabel("1-3".into()));
    for (step, expected_generic) in [true, false, true, false].into_iter().enumerate() {
        cache_properties(&mut app);
        app.tab.doc.atoms[1].cip_label = Some("R".into());
        app.tab.doc.bonds[1].cip_label = Some("E".into());
        let revision = app.tab.revision;
        match step {
            0 => {
                let _ = app.generic_action(generic::Action::DefineRepeat(bracket));
            }
            1 => {
                let _ = app.generic_action(generic::Action::ClearRepeat(bracket));
            }
            2 => app.step_history(false),
            3 => app.step_history(true),
            _ => unreachable!(),
        }
        assert_eq!(reshiki::generic::present(&app.tab.doc), expected_generic);
        assert!(
            app.tab.analysis.is_none(),
            "step {step} must discard cached concrete properties"
        );
        assert!(
            app.tab.chemistry_notice.is_none(),
            "step {step} must discard the old notice"
        );
        assert!(
            app.tab.labels_dirty,
            "step {step} must request a new fail-closed refresh"
        );
        assert!(
            app.tab
                .doc
                .atoms
                .iter()
                .all(|atom| atom.cip_label.is_none())
        );
        assert!(
            app.tab
                .doc
                .bonds
                .iter()
                .all(|bond| bond.cip_label.is_none())
        );
        assert_eq!(app.tab.revision, revision + 1);
        assert!(!app.error);
        app.tab.doc.validate().unwrap();
    }
}

#[test]
fn crystal_substitution_and_charge_shortcut_are_single_undo_steps() {
    for (source, label) in [("Na", "K+"), ("Cl", "Br")] {
        let mut app = app();
        app.tab.doc =
            reshiki::crystal::read("cif", include_str!("../../tests/fixtures/crystal/nacl.cif"))
                .unwrap()
                .0;
        app.tab.doc.models[0].repeat = [2, 1, 1];
        let id = app
            .tab
            .doc
            .atoms
            .iter()
            .find(|a| a.element == source)
            .unwrap()
            .id;
        let before = app.tab.doc.clone();
        let _ = app.atom_text_action(atom_text::Action::Begin(Some(id)));
        let _ = app.atom_text_action(atom_text::Action::Input(label.into()));
        let _ = app.atom_text_action(atom_text::Action::Apply);
        assert_eq!(app.tab.doc.models, before.models);
        assert!(app.tab.doc.atom(id).unwrap().no_implicit);
        assert!(!app.status.contains("removed"));
        app.step_history(false);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        app.tab.selected = vec![id];
        app.change_charge(1);
        assert_eq!(app.tab.doc.models, before.models);
        assert!(app.tab.doc.atom(id).unwrap().no_implicit);
        app.step_history(false);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn an_edit_removing_the_unit_cell_reports_it_and_preserves_analysis_provenance() {
    let mut app = app();
    app.tab.doc =
        reshiki::crystal::read("cif", include_str!("../../tests/fixtures/crystal/nacl.cif"))
            .unwrap()
            .0;
    let before = app.tab.doc.clone();
    app.tab.doc.bonds[0].order = 2;
    app.changed(before.clone());
    assert_eq!(
        app.status,
        "Unit cell removed by the edit · Undo restores it"
    );
    assert!(!app.error);
    assert!(app.tab.doc.models.is_empty());
    assert!(reshiki::models3d::crystal::touches(&app.tab.doc, &[]));
    assert!(app.tab.doc.bonds.iter().all(|b| b.distance_derived));
    app.step_history(false);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn element_tool_keeps_crystal_sites_explicit_and_retains_the_cell() {
    let mut app = app();
    app.tab.doc =
        reshiki::crystal::read("cif", include_str!("../../tests/fixtures/crystal/nacl.cif"))
            .unwrap()
            .0;
    let atom = app
        .tab
        .doc
        .atoms
        .iter()
        .find(|a| a.element == "Na")
        .unwrap()
        .clone();
    let before = app.tab.doc.clone();
    app.tool = Tool::Atom;
    app.element = "K".into();
    app.edit(Edit::Click(atom.position));
    assert_eq!(app.tab.doc.models, before.models);
    assert_eq!(app.tab.doc.atom(atom.id).unwrap().element, "K");
    assert!(app.tab.doc.atom(atom.id).unwrap().no_implicit);
    app.step_history(false);
    assert_eq!(app.tab.doc, before);
}
