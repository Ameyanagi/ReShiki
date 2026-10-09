#[test]
fn atom_drag_and_click_are_separate_undoable_actions() -> Result<(), String> {
    let (mut app, _) = App::new();
    let source = app.tab.doc.add_atom("C", Point::default());
    let initial = app.tab.doc.clone();
    app.tool = Tool::Atom;
    app.element = "O".into();
    app.edit(Edit::Bond(
        Point::default(),
        Point::new(42., 0.),
        Some(source),
        None,
    ));
    assert_eq!(app.tab.doc.atoms.len(), 2);
    assert_eq!(app.tab.doc.atom(source).ok_or("Source")?.element, "C");
    assert_eq!(app.tab.doc.atoms.last().ok_or("Oxygen")?.element, "O");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
    app.edit(Edit::Click(Point::default()));
    assert_eq!(app.tab.doc.atoms.len(), 1);
    assert_eq!(app.tab.doc.atom(source).ok_or("Replacement")?.element, "O");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
    Ok(())
}

use super::*;
use reshiki::document::Annotation;
use reshiki::engine::ChemistryEngine;
use reshiki::graphics::GraphicStyle;

#[test]
fn haworth_tools_and_edge_styles_are_undoable_without_erasing_sugar_stereo() -> anyhow::Result<()> {
    use anyhow::Context;
    use reshiki::{
        bonds::BondPreset as P,
        haworth::{Anomer, Sugar, sugar_document},
        rings::Preset,
    };
    for preset in [Preset::HaworthFive, Preset::HaworthSix] {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        let before = app.tab.doc.clone();
        app.edit(Edit::RingPreset(
            preset,
            Point::default(),
            None,
            false,
            false,
        ));
        assert!(!app.error, "{}", app.status);
        let placed = app.tab.doc.clone();
        assert!(placed.bonds.iter().all(|b| b.projection));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, placed);
    }
    for preset in [P::Wedge, P::HashedWedge, P::Bold, P::Single] {
        let (mut app, _) = App::new();
        app.tab.doc =
            sugar_document(Sugar::Glucose, Anomer::Alpha, 42.).map_err(anyhow::Error::msg)?;
        app.tab.doc.reconcile_molecule_groups();
        let source = app.tab.doc.clone();
        let bond = source
            .bonds
            .iter()
            .find(|b| b.display == "bold")
            .context("Front edge")?;
        app.tab.selected = vec![bond.a, bond.b];
        let _ = app.update(Message::ApplyBondPreset(preset));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.atoms, source.atoms);
        assert!(!chemistry_changed(&source, &app.tab.doc));
        if app.tab.doc != source {
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, source);
        }
        let a = source.atom(bond.a).context("Front atom")?.position;
        let b = source.atom(bond.b).context("Front atom")?.position;
        app.tool = Tool::StyledBond(preset);
        app.edit(Edit::Click(Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.)));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.atoms, source.atoms);
        assert!(!chemistry_changed(&source, &app.tab.doc));
    }
    Ok(())
}

#[test]
fn inspector_changes_reset_scrolling_but_normal_updates_keep_the_position() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    // Reaction handlers previously bypassed the normal tab navigation task.
    let task = app.update(Message::Reaction(reactions::Action::Open));
    assert_eq!(app.inspector_tab, InspectorTab::Reactions);
    assert!(task.units() > 0);
    assert_eq!(app.update(Message::InspectorScroll(180.)).units(), 0);
    assert_eq!(
        app.update(Message::Reaction(reactions::Action::Open))
            .units(),
        0
    );
}

fn subscriptions(app: &App) -> usize {
    iced::advanced::subscription::into_recipes(app.subscription()).len()
}

#[test]
fn idle_windows_stop_polling_and_pending_work_restarts_timers() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let idle = subscriptions(&app);
    // Window close and keyboard/mouse events are always subscribed.
    // macOS adds Finder events; macOS/Windows add the native accessibility
    // action receiver and window/input events; Linux adds window-open
    // events for clipboard initialization. These streams do not poll.
    let event_streams = 2
        + usize::from(cfg!(target_os = "macos"))
        + 2 * usize::from(cfg!(any(target_os = "macos", windows)))
        + usize::from(cfg!(target_os = "linux"));
    assert_eq!(idle, event_streams);
    app.assistant.busy = true;
    assert_eq!(subscriptions(&app), idle + 1);
    app.assistant.busy = false;
    app.tab.labels_dirty = true;
    assert_eq!(subscriptions(&app), idle); // Labels use a task, not a polling timer.
    app.tab.busy = true;
    assert_eq!(subscriptions(&app), idle);
    app.tab.busy = false;
    app.tab.labels_dirty = false;

    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    app.tab.recovery = Some(Recovery::in_directory(directory.path())?);
    app.inspector_open = false;
    assert_eq!(subscriptions(&app), idle);
    app.tab.doc.add_atom("O", Point::default());
    assert_eq!(subscriptions(&app), idle + 1);
    let _ = app.update(Message::Tick);
    assert_eq!(subscriptions(&app), idle);
    autosave::tests::finish_pending(&mut app);
    assert_eq!(subscriptions(&app), idle);
    let path = app
        .tab
        .recovery
        .as_ref()
        .ok_or("Missing recovery")?
        .session
        .clone();
    assert!(path.exists());
    app.tab.saved = app.tab.doc.clone();
    assert_eq!(subscriptions(&app), idle + 1);
    let _ = app.update(Message::Tick);
    autosave::tests::finish_pending(&mut app);
    assert!(!path.exists());
    assert_eq!(subscriptions(&app), idle);
    Ok(())
}

#[test]
fn dismissing_the_recovery_offer_restores_the_ready_message() {
    use reshiki::recovery::Snapshot;
    let (mut app, _) = App::new();
    let candidate = Candidate {
        path: "draft.json".into(),
        snapshot: Snapshot {
            document: Document::default(),
            source: None,
            saved_at: 0,
        },
    };
    // At launch the offer replaces the ready message.
    app.recovered = vec![candidate.clone()];
    app.status.clear();
    let _ = app.update(Message::DismissRecovery);
    assert!(app.recovered.is_empty());
    assert_eq!(app.status, READY);
    // A newer message stays.
    app.recovered = vec![candidate];
    app.status = "Drawing updated".into();
    let _ = app.update(Message::DismissRecovery);
    assert_eq!(app.status, "Drawing updated");
}

fn checked_labels(app: &mut App) {
    let mut checked = app.tab.doc.clone();
    for atom in &mut checked.atoms {
        atom.label_h = 2;
    }
    // A check must not adopt the engine's normalized bond depiction.
    if let Some(bond) = checked.bonds.first_mut() {
        bond.order = 4;
    }
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision,
        kind: Job::Analyze,
        result: Box::new(Ok(Response {
            document: Some(checked),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        })),
    });
}

#[test]
fn double_tool_cycles_only_line_position_and_preserves_chemistry() {
    use reshiki::bonds::DoublePosition as P;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tool = Tool::Bond(2);
    let c = app.tab.doc.add_atom("C", Point::default());
    let o = app.tab.doc.add_atom("O", Point::new(0., -42.));
    let methyl = app.tab.doc.add_atom("C", Point::new(36.373, 21.));
    app.tab.doc.add_bond(c, o, 2, "plain");
    app.tab.doc.add_bond(c, methyl, 1, "plain");
    app.tab.doc.bonds[0].color = reshiki::palette::Color::Custom([32, 80, 145]);
    app.tab.doc.atom_mut(c).unwrap().label_h = 1;
    let original = app.tab.doc.clone();
    let mut scenes = std::collections::BTreeSet::new();
    for position in [P::Left, P::Right, P::Center] {
        app.edit(Edit::Click(Point::new(0., -21.)));
        let mut expected = original.clone();
        expected.bonds[0].double_position = position;
        assert_eq!(app.tab.doc, expected);
        assert!(!chemistry_changed(&original, &app.tab.doc));
        scenes.insert(reshiki::scene::svg(&app.tab.doc));
    }
    assert_eq!(scenes.len(), 3);
    for _ in 0..3 {
        let _ = app.update(Message::Undo);
    }
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.bonds[0].double_position, P::Left);
    app.tab.doc.bonds[0].order = 1;
    app.edit(Edit::Click(Point::new(0., -21.)));
    assert_eq!(app.tab.doc.bonds[0].order, 2);
}

#[test]
fn cancelling_a_running_cleanup_refresh_prevents_late_preview_or_apply() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(80., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![b];
    let original = app.tab.doc.clone();
    let _ = app.update(Message::Cleanup(cleanup::Action::Begin));
    let job = cleanup::CleanupJob {
        options: reshiki::cleanup::Options {
            scope: reshiki::cleanup::Scope::SelectedAtoms,
            ..Default::default()
        },
        selection: vec![b],
        serial: app.tab.cleanup_serial,
        epoch: app.tab.file_epoch,
    };
    let response = || {
        Box::new(Ok(Response {
            document: Some(original.clone()),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        }))
    };
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision,
        kind: Job::Clean(job.clone()),
        result: response(),
    });
    assert_eq!(
        app.tab.cleanup.as_ref().unwrap().job.options.scope,
        reshiki::cleanup::Scope::SelectedAtoms
    );
    let _ = app.update(Message::Cleanup(cleanup::Action::Scope(
        reshiki::cleanup::Scope::SelectedMolecules,
    )));
    assert!(app.tab.busy);
    let mut pending = job;
    pending.serial = app.tab.cleanup_serial;
    let _ = app.update(Message::Cleanup(cleanup::Action::Apply));
    assert_eq!(app.tab.doc, original);
    assert!(app.tab.cleanup.is_some());
    let _ = app.update(Message::Cleanup(cleanup::Action::Cancel));
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision,
        kind: Job::Clean(pending),
        result: response(),
    });
    assert!(app.tab.cleanup.is_none());
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn abbreviation_display_changes_are_unsaved_and_undoable() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    let c = app.tab.doc.add_atom("C", Point::new(63., 36.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(b, c, 1, "plain");
    app.tab.doc.contract(&[b, c], "OMe", "MeO").unwrap();
    app.tab.saved = app.tab.doc.clone();
    let saved = app.tab.doc.clone();
    let _ = app.update(Message::Abbreviations(abbreviations::Action::ExpandAll));
    assert!(app.dirty());
    assert!(app.title().contains('•'));
    assert!(app.tab.doc.abbreviations.is_empty());
    let _ = app.update(Message::Undo);
    assert!(!app.dirty());
    assert_eq!(app.tab.doc, saved);
    let _ = app.update(Message::Redo);
    assert!(app.dirty());
    assert!(app.tab.doc.abbreviations.is_empty());
}

#[test]
fn cleanup_requires_apply_can_cancel_and_rejects_stale_results() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(70., 12.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = vec![a, b];
    let original = app.tab.doc.clone();
    let mut cleaned = original.clone();
    cleaned.atom_mut(b).unwrap().position = Point::new(42., 0.);
    let response = || {
        Box::new(Ok(Response {
            document: Some(cleaned.clone()),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        }))
    };
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision,
        kind: Job::Clean(cleanup::CleanupJob {
            options: Default::default(),
            selection: vec![a, b],
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        }),
        result: response(),
    });
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.display_document(), &cleaned);
    assert!(!app.dirty());
    let _ = app.update(Message::Delete);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Cleanup(cleanup::Action::Original(true)));
    assert_eq!(app.display_document(), &original);
    let _ = app.update(Message::Cleanup(cleanup::Action::Cancel));
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision,
        kind: Job::Clean(cleanup::CleanupJob {
            options: Default::default(),
            selection: vec![a, b],
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        }),
        result: response(),
    });
    let _ = app.update(Message::Cleanup(cleanup::Action::Apply));
    assert_eq!(app.tab.doc, cleaned);
    assert_eq!(app.tab.selected, vec![a, b]);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, cleaned);
    let _ = app.update(Message::EngineDone {
        revision: app.tab.revision.wrapping_sub(1),
        kind: Job::Clean(cleanup::CleanupJob {
            options: Default::default(),
            selection: vec![a, b],
            serial: app.tab.cleanup_serial,
            epoch: app.tab.file_epoch,
        }),
        result: response(),
    });
    assert!(app.tab.cleanup.is_none());
}

#[test]
fn label_edits_and_indicator_drags_are_atomic_and_do_not_change_chemistry() {
    use atom_labels::Action;
    use reshiki::atom_labels::{Carbons, Owner};
    let (mut app, _) = App::new();
    let _ = app.update(Message::New);
    let a = app.tab.doc.add_atom("N", Point::default());
    app.tab.history = History::default();
    let original = app.tab.doc.clone();
    app.label_action(Action::Number);
    assert!(!chemistry_changed(&original, &app.tab.doc));
    let numbered = app.tab.doc.clone();
    app.edit(Edit::AtomIndicator(Owner::Number(a), Point::new(20., -30.)));
    assert_eq!(
        app.tab
            .doc
            .atom(a)
            .unwrap()
            .display
            .number
            .as_ref()
            .unwrap()
            .offset,
        Some(Point::new(20., -30.))
    );
    let _ = app.update(Message::Undo);
    assert!(same_drawing(&app.tab.doc, &numbered));
    let _ = app.update(Message::Undo);
    assert!(same_drawing(&app.tab.doc, &original));
    let _ = app.update(Message::Redo);
    assert!(same_drawing(&app.tab.doc, &numbered));
    app.label_action(Action::Carbons(Carbons::All));
    app.label_action(Action::Hydrogens(false));
    app.label_action(Action::Stereo(true));
    let _ = app.update(Message::New);
    assert_eq!(app.tab.doc.atom_labels, Default::default());
    assert_eq!(app.current_text_style().size_pt, 10.);
    assert_eq!(app.current_text_style().family, "Arial");
}

#[test]
fn invalid_edit_is_rolled_back_without_an_undo_entry() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::New);
    app.tab.doc.add_atom("C", Point::default());
    app.tab.history = History::default();
    let before = app.tab.doc.clone();
    app.tab.doc.atoms[0].position.x = f32::NAN;
    app.changed(before.clone());
    assert_eq!(app.tab.doc, before);
    assert!(app.error);
    assert!(!app.tab.history.undo(&mut app.tab.doc));
    for color in ["αβγ", "💚AB", "#GG0000", "12345", "1234567"] {
        assert!(graphics::parse_color(color).is_none());
    }
}

#[test]
fn chemistry_check_keeps_placement_atomic_and_preserves_redo() {
    use reshiki::rings::Preset;
    let (mut app, _) = App::new();
    let _ = app.update(Message::New);
    app.edit(Edit::RingPreset(
        Preset::ChairUp,
        Point::default(),
        None,
        false,
        false,
    ));
    let first = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let revision = app.tab.revision;
    checked_labels(&mut app);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.selected, selected);
    assert!(same_drawing(&first, &app.tab.doc));
    assert!(app.tab.doc.atoms.iter().all(|atom| atom.label_h == 2));
    let checked_first = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.atoms.is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, checked_first);
    app.edit(Edit::RingPreset(
        Preset::ChairDown,
        Point::new(300., 0.),
        None,
        false,
        false,
    ));
    let second = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    checked_labels(&mut app);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, second);
}

#[test]
fn ring_color_toolbar_changes_only_fills_and_undo_restores_them() -> Result<(), String> {
    let (mut app, _) = App::new();
    app.tab.selected = editing::ring(&mut app.tab.doc, Point::default(), 6, true, 0.);
    let original = app.tab.doc.clone();
    let _ = app.update(Message::ColorScope(typography::ColorScope::Rings));
    let tint =
        reshiki::palette::Color::Palette(reshiki::palette::Hue::Blue, reshiki::palette::Row::Tint);
    let _ = app.update(Message::TextStyle(reshiki::typography::StyleChange::Color(
        tint,
    )));
    assert_eq!(app.tab.doc.ring_fills.len(), 1);
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    assert_eq!(app.current_selection_color(), Some(tint));
    assert!(
        app.tab.doc.recent_colors.is_empty(),
        "palette colors are not recent customs"
    );
    let colored = app.tab.doc.clone();
    let _ = app.update(Message::ClearRingFill);
    assert!(app.tab.doc.ring_fills.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, colored);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    app.tab.doc.validate()?;
    Ok(())
}

#[test]
fn ring_fill_edits_mark_saved_drawings_dirty_and_restore_through_undo_redo() {
    let (mut app, _) = App::new();
    app.tab.selected = editing::ring(&mut app.tab.doc, Point::default(), 6, true, 0.);
    app.tab.saved = app.tab.doc.clone();
    app.tab.history = History::default();
    let unfilled = app.tab.saved.clone();
    assert!(!app.dirty());
    let _ = app.update(Message::ColorScope(typography::ColorScope::Rings));
    let tint =
        reshiki::palette::Color::Palette(reshiki::palette::Hue::Blue, reshiki::palette::Row::Tint);
    let _ = app.update(Message::TextStyle(reshiki::typography::StyleChange::Color(
        tint,
    )));
    let filled = app.tab.doc.clone();
    assert_eq!(filled.ring_fills.len(), 1);
    assert_eq!(filled.atoms, unfilled.atoms);
    assert_eq!(filled.bonds, unfilled.bonds);
    assert!(app.dirty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, unfilled);
    assert!(!app.dirty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, filled);
    assert!(app.dirty());

    app.tab.saved = filled.clone();
    assert!(!app.dirty());
    let _ = app.update(Message::ClearRingFill);
    assert_eq!(app.tab.doc, unfilled);
    assert!(app.dirty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, filled);
    assert!(!app.dirty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, unfilled);
    assert!(app.dirty());
}

#[test]
fn custom_ring_hex_is_exact_in_dark_mode_and_undoable() {
    let (mut app, _) = App::new();
    app.tab.selected = editing::ring(&mut app.tab.doc, Point::default(), 6, false, 0.);
    app.tab.doc.canvas_theme = reshiki::canvas_theme::CanvasTheme::Dark;
    let _ = app.update(Message::ColorScope(typography::ColorScope::Rings));
    let original = app.tab.doc.clone();
    let _ = app.update(Message::TextColor("#C9E0F8".into()));
    let _ = app.update(Message::ApplyTextColor);
    let fill = app.tab.doc.ring_fills.first().unwrap();
    assert_eq!(fill.color, reshiki::palette::Color::Custom([201, 224, 248]));
    assert_eq!(app.tab.doc.recent_colors, [[201, 224, 248]]);
    assert_eq!(app.tab.text_color_input, "#C9E0F8");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
}

#[test]
fn depth_appearance_edits_mark_saved_projections_dirty_and_restore_through_undo_redo()
-> Result<(), String> {
    use depth_appearance::Action;
    for (initial, action) in [
        (0, Action::Enhance(true)),
        (1, Action::Enhance(false)),
        (2, Action::Clear),
    ] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let (mut app, _) = App::new();
        app.tab.recovery = Some(Recovery::in_directory(directory.path())?);
        app.tab.doc = Document::default();
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.atom_mut(a).unwrap().depth = -21.;
        app.tab.doc.atom_mut(b).unwrap().depth = 21.;
        if initial > 0 {
            reshiki::depth_appearance::enable(
                &mut app.tab.doc,
                &[a, b],
                reshiki::depth_appearance::DEFAULT_STRENGTH,
            )?;
        }
        if initial == 2 {
            reshiki::depth_appearance::freeze(&mut app.tab.doc, &[a, b]);
        }
        let saved = Document::from_json(&app.tab.doc.file_json()?)?;
        app.tab.doc = saved.clone();
        app.tab.saved = saved.clone();
        app.tab.history = History::default();
        app.tab.labels_dirty = false;
        app.tab.selected = vec![a];
        let revision = app.tab.revision;
        assert!(!app.dirty());
        assert!(!app.needs_draft(&app.tab));

        let _ = app.update(Message::DepthAppearance(action));
        assert!(!app.error, "{}", app.status);
        let edited = app.tab.doc.clone();
        assert_ne!(edited.depth_appearance, saved.depth_appearance);
        assert_eq!(edited.atoms, saved.atoms);
        assert_eq!(edited.bonds, saved.bonds);
        assert!(!chemistry_changed(&saved, &edited));
        assert!(app.dirty());
        assert!(app.needs_draft(&app.tab));
        assert_eq!(app.tab.revision, revision + 1);
        assert_eq!(Document::from_json(&edited.file_json()?)?, edited);

        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, saved);
        assert!(!app.dirty());
        assert!(!app.needs_draft(&app.tab));
        assert!(!app.tab.history.can_undo());
        assert_eq!(app.tab.revision, revision + 2);

        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, edited);
        assert!(app.dirty());
        assert!(app.needs_draft(&app.tab));
        assert!(!app.tab.history.can_redo());
        assert_eq!(app.tab.revision, revision + 3);
        let _ = app.update(Message::Close(iced::window::Id::unique()));
        assert!(matches!(
            app.pending,
            Some(Pending::CloseWindow(_, id, _)) if id == app.tab.id
        ));
    }
    Ok(())
}

#[test]
fn computed_hydrogen_labels_do_not_make_a_saved_drawing_dirty() {
    use reshiki::rings::Preset;
    let (mut app, _) = App::new();
    let _ = app.update(Message::New);
    app.tab.doc = Preset::ChairUp.document(42., false);
    app.tab.saved = app.tab.doc.clone();
    checked_labels(&mut app);
    assert!(!app.dirty());
    assert!(!app.tab.history.can_undo());
    assert_ne!(app.tab.doc, app.tab.saved);
    app.tab.doc.atoms[0].charge = 1;
    assert!(app.dirty());
    app.tab.doc = app.tab.saved.clone();
    app.tab.doc.atoms[0].position.x += 1.;
    assert!(app.dirty());
}

#[test]
fn circle_palette_and_modifier_share_atomic_attachment_and_history() {
    for modifier in [false, true] {
        let (mut app, _) = App::new();
        app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
        app.aromatic_ring = true;
        app.ring_size = 6;
        let anchor = app.tab.doc.atoms[0].position;
        let edit = if modifier {
            Edit::DelocalizedRing(anchor, None, 6)
        } else {
            Edit::Ring(anchor, None)
        };
        let original = app.tab.doc.clone();
        app.edit(edit.clone());
        assert!(!app.error, "{}", app.status);
        assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (12, 13));
        assert!(!reshiki::aromatic::circles(&app.tab.doc).is_empty());
        reshiki::chemistry::document::prepare(&app.tab.doc).unwrap();
        let placed = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let revision = app.tab.revision;
        app.edit(edit); // The same host carbon has no remaining valence.
        assert!(app.error);
        assert_eq!(app.tab.doc, placed);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.revision, revision);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, placed);
    }
}

#[test]
fn regular_ring_rejection_preserves_selection_history_and_redo() {
    for legacy_click in [false, true] {
        let (mut app, _) = App::new();
        let _ = app.update(Message::New);
        app.tool = Tool::Ring;
        app.aromatic_ring = false;
        app.ring_size = 6;
        app.tab.doc = Document::from_json(include_bytes!(
            "../../tests/fixtures/ui-declutter/ring-rejection.rsk"
        ))
        .unwrap();
        let carbon = app.tab.doc.atoms[0].id;
        let original = app.tab.doc.clone();
        // A valid placement is one history entry; an invalid attempt after
        // Undo must leave that entry available to Redo.
        app.edit(Edit::Ring(Point::new(300., 0.), None));
        assert!(!app.error, "{}", app.status);
        let placed = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        app.tab.selected = vec![carbon];
        let revision = app.tab.revision;
        app.edit(if legacy_click {
            Edit::Click(Point::default())
        } else {
            Edit::Ring(Point::default(), None)
        });
        assert!(app.error);
        assert_eq!(app.tab.doc, original);
        assert_eq!(app.tab.selected, vec![carbon]);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
        assert!(app.tab.history.can_redo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, placed);
    }

    let (mut app, _) = App::new();
    let _ = app.update(Message::New);
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    app.aromatic_ring = false;
    app.ring_size = 6;
    app.tab.selected = app.tab.doc.all_ids();
    let original = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    let a = app.tab.doc.atom(app.tab.doc.bonds[0].a).unwrap().position;
    let b = app.tab.doc.atom(app.tab.doc.bonds[0].b).unwrap().position;
    let p = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
    let revision = app.tab.revision;
    app.edit(Edit::Ring(p, Some(Point::default())));
    assert!(app.error);
    assert_eq!(app.tab.doc, original);
    assert_eq!(app.tab.selected, selected);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    app.edit(Edit::Ring(p, Some(Point::new(p.x * 2., p.y * 2.))));
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (10, 11));
    let placed = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, placed);
}

#[test]
fn ring_presets_use_atomic_history_and_leave_invalid_hosts_untouched() {
    use reshiki::rings::Preset;
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let before = app.tab.doc.clone();
    app.edit(Edit::RingPreset(
        Preset::ChairUp,
        Point::default(),
        Some(Point::new(0., 80.)),
        false,
        false,
    ));
    let placed = app.tab.doc.clone();
    assert_eq!(placed.atoms.len(), 6);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, placed);
    app.edit(Edit::RingPreset(
        Preset::ChairDown,
        Point::new(300., 0.),
        None,
        false,
        false,
    ));
    assert_eq!(app.tab.doc.atoms.len(), 12);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, placed);
    let _ = app.update(Message::Tool(Tool::RingPreset(Preset::ChairUp)));
    let _ = app.update(Message::New);
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.tab.bond_drawing.length, 42.);
    let c = app.tab.doc.add_atom("C", Point::default());
    app.tab.doc.atom_mut(c).unwrap().radical_electrons = 1;
    let before = app.tab.doc.clone();
    app.edit(Edit::RingPreset(
        Preset::ChairUp,
        Point::default(),
        None,
        false,
        false,
    ));
    assert_eq!(app.tab.doc, before);
    assert!(app.error);
}

#[test]
fn arrow_click_places_a_fixed_rightward_arrow_and_repeated_click_reverses_it() {
    use reshiki::arrows::{ArrowStyle, Preset};
    use reshiki::graphics::LinePattern;
    for zoom in [0.5, 2.5] {
        let (mut app, _) = App::new();
        app.tab.camera.zoom = zoom;
        let style = ArrowStyle {
            pattern: LinePattern::Dashed,
            ..ArrowStyle::preset(Preset::Forward)
        };
        let _ = app.update(Message::Palette(palettes::Action::ArrowVariant(
            Preset::Forward,
            style.clone(),
        )));
        let blank = app.tab.doc.clone();
        let start = Point::new(-100., 30.);
        app.edit(Edit::Click(start));
        assert_eq!(app.tab.doc.arrows.len(), 1);
        let arrow = app.tab.doc.arrows[0].clone();
        assert_eq!(arrow.start, start);
        assert_eq!(
            arrow.end,
            start.offset(blank.drawing_style.bond_length_world * 2., 0.)
        );
        assert_eq!(arrow.appearance(), style);
        assert_eq!(app.tab.selected, [arrow.id]);
        let placed = app.tab.doc.clone();
        app.tab.selected.clear();
        app.edit(Edit::Click(arrow.point(0.5)));
        assert_eq!(app.tab.doc.arrows.len(), 1);
        assert_eq!(app.tab.doc.arrows[0].start, arrow.end);
        assert_eq!(app.tab.doc.arrows[0].end, arrow.start);
        assert_eq!(app.tab.selected, [arrow.id]);
        let reversed = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, placed);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, blank);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, placed);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, reversed);
    }
}

#[test]
fn arrow_tool_click_applies_variants_and_cycles_half_heads_without_adding_objects() {
    use reshiki::arrows::{ArrowStyle, Head, Preset};
    let (mut app, _) = App::new();
    let _ = app.update(Message::Tool(Tool::Arrow));
    app.edit(Edit::Click(Point::default()));
    let before = app.tab.doc.clone();
    let style = ArrowStyle {
        head: Head::Left,
        ..ArrowStyle::default()
    };
    let _ = app.update(Message::Palette(palettes::Action::ArrowVariant(
        Preset::Forward,
        style.clone(),
    )));
    let midpoint = app.tab.doc.arrows[0].point(0.5);
    app.edit(Edit::Click(midpoint));
    assert_eq!(app.tab.doc.arrows.len(), 1);
    assert_eq!(app.tab.doc.arrows[0].appearance(), style);
    app.edit(Edit::Click(midpoint));
    assert_eq!(app.tab.doc.arrows[0].appearance().head, Head::Right);
    app.edit(Edit::Click(midpoint));
    assert_eq!(app.tab.doc.arrows[0].appearance().head, Head::Left);
    for _ in 0..3 {
        let _ = app.update(Message::Undo);
    }
    assert_eq!(app.tab.doc, before);
}

#[test]
fn repeated_arrow_click_reverses_reaction_roles_and_undo_restores_them() {
    use reshiki::reactions::{Participant, Reaction};
    let (mut app, _) = App::new();
    let reactant = app.tab.doc.add_atom("O", Point::new(-100., 0.));
    let product = app.tab.doc.add_atom("N", Point::new(200., 0.));
    let _ = app.update(Message::Tool(Tool::Arrow));
    app.edit(Edit::Click(Point::default()));
    let arrow = &app.tab.doc.arrows[0];
    let midpoint = arrow.point(0.5);
    let mut reaction = Reaction::new(arrow.id);
    reaction.reactants.push(Participant {
        atoms: vec![reactant],
        coefficient: 1,
    });
    reaction.products.push(Participant {
        atoms: vec![product],
        coefficient: 1,
    });
    app.tab.doc.reactions.push(reaction);
    let before = app.tab.doc.clone();
    app.edit(Edit::Click(midpoint));
    assert_eq!(
        app.tab.doc.reactions[0].reactants,
        before.reactions[0].products
    );
    assert_eq!(
        app.tab.doc.reactions[0].products,
        before.reactions[0].reactants
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn arrow_edits_keep_bend_history_and_new_resets_jacs_defaults() {
    use arrows::{Action, Field};
    use reshiki::arrows::{ArrowStyle, Head, Preset};
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app.tab.saved = app.tab.doc.clone();
    let _ = app.update(Message::ArrowStyle(Preset::Fishhook));
    app.edit(Edit::Bond(
        Point::new(0., 0.),
        Point::new(120., 0.),
        None,
        None,
    ));
    let id = app.tab.selected[0];
    app.edit(Edit::ArrowHandle(id, 2, Point::new(60., -50.)));
    let bent = app.tab.doc.clone();
    app.arrow_action(Action::Number(Field::Line, "1.5".into()));
    app.arrow_action(Action::ApplyNumber(Field::Line));
    assert_eq!(app.tab.doc.arrows[0].appearance().width_pt, 1.5);
    assert_eq!(app.tab.doc.arrows[0].control, bent.arrows[0].control);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, bent);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.arrows.style.width_pt, 1.5);
    app.arrow_action(Action::Tail(Head::Full));
    app.arrow_action(Action::Reverse);
    assert_eq!(app.tab.doc.arrows[0].end, Point::default());
    app.arrow_action(Action::Number(Field::Length, "NaN".into()));
    let before = app.tab.doc.clone();
    app.arrow_action(Action::ApplyNumber(Field::Length));
    assert_eq!(app.tab.doc, before);
    assert!(app.error);
    let _ = app.update(Message::New);
    assert_eq!(app.tab.arrows.style, ArrowStyle::default());
    assert_eq!(app.tab.arrow_style, Preset::Forward);
    assert_eq!(app.tab.caption_format.style.family, "Arial");
    assert_eq!(app.tab.caption_format.style.size_pt, 10.);
    assert_eq!(
        app.tab.bond_drawing.length,
        reshiki::style::DEFAULT.bond_length_world
    );
    assert!(app.tab.doc.arrows.is_empty());
}

#[test]
fn arrow_width_in_mixed_selection_preserves_bonds_and_other_objects() {
    use arrows::{Action, Field};

    for selected in [vec![10], vec![1, 2, 3, 10, 20]] {
        let (mut app, _) = App::new();
        app.tab.doc = Document::from_json(include_bytes!(
            "../../tests/fixtures/ui-declutter/mixed-arrow-width.rsk"
        ))
        .unwrap();
        app.tab.saved = app.tab.doc.clone();
        let _ = app.update(Message::Canvas(Edit::Select(selected.clone())));
        let before = app.tab.doc.clone();
        assert!(!app.tab.history.can_undo());

        let _ = app.update(Message::ArrowAction(Action::Number(
            Field::Line,
            "1.5".into(),
        )));
        assert_eq!(app.tab.doc, before, "Typing must not change the drawing");
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::ArrowAction(Action::ApplyNumber(Field::Line)));
        assert!(!app.error, "{}", app.status);

        let mut expected = before.clone();
        expected
            .arrows
            .iter_mut()
            .find(|arrow| arrow.id == 10)
            .unwrap()
            .style
            .as_mut()
            .unwrap()
            .width_pt = 1.5;
        assert_eq!(app.tab.doc.drawing_style, before.drawing_style);
        assert_eq!(app.tab.doc.bonds, before.bonds);
        assert_eq!(
            app.tab.doc, expected,
            "Only the selected arrow width changes"
        );
        assert_eq!(app.tab.selected, selected);
        let saved = serde_json::to_vec(&app.tab.doc).unwrap();
        assert_eq!(Document::from_json(&saved).unwrap(), expected);

        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, selected);
        assert!(
            !app.tab.history.can_undo(),
            "Apply is exactly one Undo step"
        );
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, expected);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.arrows.style.width_pt, 1.5);
    }
}

#[test]
fn library_authoring_is_independent_of_drawing_history_and_repeat_placement_keeps_anchor() {
    use reshiki::templates::Anchor;
    use template_library::Action as A;
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a, b];
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::Templates(A::BeginSave));
    app.tab.selected.clear();
    let _ = app.update(Message::Templates(A::Name("Methanol".into())));
    let _ = app.update(Message::Templates(A::SaveDetails));
    assert_eq!(app.templates.library.templates[0].document, before);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    let index = app.template_index;
    let _ = app.update(Message::InsertTemplate(index));
    let _ = app.update(Message::Templates(A::Anchor(Anchor::Atom(b))));
    let _ = app.update(Message::Templates(A::RememberAnchor));
    let _ = app.update(Message::Templates(A::Browse));
    app.templates.connection = reshiki::templates::Connection::FuseBond;
    let _ = app.update(Message::InsertTemplate(index));
    assert_eq!(app.templates.anchor, Anchor::Atom(b));
    assert_eq!(
        app.templates.connection,
        reshiki::templates::Connection::Connect
    );
    let _ = app.update(Message::Templates(A::Repeat(true)));
    app.edit(Edit::Template(Point::new(250., 100.), None));
    assert_eq!(app.tool, Tool::Template);
    assert_eq!(app.tab.doc.atoms.len(), 4);
    assert_eq!(app.tab.doc.atoms[3].position, Point::new(250., 100.));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Templates(A::Remove));
    assert!(app.templates.library.templates.is_empty());
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Templates(A::Restore));
    assert_eq!(app.templates.library.templates.len(), 1);
    assert_eq!(app.templates.library.templates[0].anchor, Anchor::Atom(b));
    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        id: caption,
        position: Point::new(0., 60.),
        text: "Label".into(),
        format: Default::default(),
    });
    app.inspector_tab = InspectorTab::Templates;
    let _ = app.update(Message::Canvas(Edit::Select(vec![caption])));
    assert_eq!(app.inspector_tab, InspectorTab::Templates);
}

#[test]
fn attached_marks_are_single_history_edits_and_removal_updates_chemistry() {
    use reshiki::scientific::{MarkKind, SymbolKind};
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let id = app.tab.doc.add_atom("N", Point::default());
    let before = app.tab.doc.clone();
    app.tool = Tool::Graphic(reshiki::graphics::GraphicKind::Symbol(
        SymbolKind::CirclePlus,
    ));
    app.edit(Edit::Graphic(Point::default(), Point::default(), false));
    assert_eq!(app.tab.selected, vec![id]);
    assert_eq!(app.tab.doc.atoms[0].charge, 1);
    assert_eq!(app.tab.doc.atoms[0].marks[0].kind, MarkKind::CircledCharge);
    let attached = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, attached);
    app.edit(Edit::AtomMark(id, 0, Point::new(-30., 20.)));
    assert_eq!(app.tab.doc.atoms[0].marks[0].offset, Point::new(-30., 20.));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, attached);
    let _ = app.update(Message::RemoveMark(id, 0));
    assert_eq!(app.tab.doc.atoms[0].charge, 0);
    assert!(app.tab.doc.atoms[0].marks.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, attached);
}

#[test]
fn every_new_document_starts_with_jacs_drawing_and_typography_defaults() {
    let (mut app, _) = App::new();
    app.tab.orbital_phase = reshiki::scientific::Phase::Shaded;
    app.tab.phase_flipped = true;
    app.tab.attach_symbols = false;
    app.tab.snap_orbitals = false;
    app.tab.graphic_style.width_pt = 3.;
    app.tab.caption_format.style.family = "Times New Roman".into();
    app.tab.caption_format.style.size_pt = 18.;
    app.tab.caption_format.style.color = reshiki::palette::Color::Custom([190, 30, 40]);
    app.tab.caption_format.style.bold = true;
    let _ = app.update(Message::DrawingLength("30".into()));
    let _ = app.update(Message::FixedLength(false));
    let _ = app.update(Message::FixedAngles(false));
    let _ = app.update(Message::ChainAngle("90".into()));
    let _ = app.update(Message::New);
    assert_eq!(
        app.tab.caption_format.style,
        reshiki::typography::TextStyle::default()
    );
    assert_eq!(app.tab.caption_format.style.family, "Arial");
    assert_eq!(app.tab.font_size_input, "10");
    assert_eq!(app.tab.text_color_input, "#000000");
    assert_eq!(app.tab.drawing_length_input, "14.4");
    assert_eq!(app.tab.bond_drawing.length, 42.);
    assert_eq!(app.tab.graphic_width_input, "0.6");
    assert_eq!(app.tab.orbital_phase, reshiki::scientific::Phase::Solid);
    assert!(!app.tab.phase_flipped && app.tab.attach_symbols);
    assert!(app.tab.snap_orbitals);
    assert_eq!(app.tab.graphic_style.width_pt, 0.6);
    assert_eq!(app.tab.chain_drawing.angle, 120.);
    assert!(app.tab.bond_drawing.fixed_angles && app.tab.bond_drawing.fixed_length);
    assert_eq!(app.tool, Tool::Select);
}

#[test]
fn entire_chain_is_one_history_step_and_draw_settings_do_not_edit_the_document() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::ChainAtoms("8".into()));
    let _ = app.update(Message::DrawingLength("20".into()));
    let _ = app.update(Message::ChainAngle("110".into()));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, 0);
    let points = reshiki::chains::straight(
        Point::default(),
        Point::new(350., 0.),
        false,
        app.tab.bond_drawing,
        app.tab.chain_drawing,
        false,
    );
    app.tool = Tool::Chain(reshiki::chains::ChainMode::Straight);
    app.edit(Edit::Chain {
        points,
        source: None,
        target: None,
    });
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (8, 7));
    let drawn = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, drawn);
    app.edit(Edit::Chain {
        points: vec![Point::default(), Point::new(42., 0.)],
        source: None,
        target: None,
    });
    assert!(app.error);
    assert_eq!(app.tab.doc, drawn);
    app.tool = Tool::Bond(1);
    let last = app.tab.doc.atoms.last().unwrap().position;
    app.edit(Edit::Click(last));
    assert!(
        (app.tab.doc.atoms.last().unwrap().position.distance(last)
            - reshiki::style::DEFAULT.world(20.))
        .abs()
            < 0.001
    );
    let drawing = app.tab.doc.clone();
    let _ = app.update(Message::ResetBondDrawing);
    assert_eq!(
        app.tab.bond_drawing.length,
        reshiki::style::DEFAULT.bond_length_world
    );
    assert_eq!(app.tab.drawing_length_input, "14.4");
    assert_eq!(app.tab.chain_drawing.angle, 120.);
    assert!(app.tab.bond_drawing.fixed_length && app.tab.bond_drawing.fixed_angles);
    assert_eq!(app.tab.doc, drawing);
}

#[test]
fn palette_keeps_text_range_formatting_and_recolors_graphics_only_in_all_scope() {
    use iced::widget::text_editor::{Action, Motion};
    use reshiki::typography::StyleChange;
    use typography::ColorScope;
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    app.tab.doc.annotations.push(Annotation {
        id: 1,
        position: Point::default(),
        text: "AB CD".into(),
        format: Default::default(),
    });
    app.tab.doc.graphics.push(Graphic::dragged(
        2,
        reshiki::graphics::GraphicKind::Rectangle,
        Point::new(0., 80.),
        Point::new(84., 120.),
        GraphicStyle {
            fill: Some(reshiki::palette::Color::Custom([200, 200, 200])),
            ..Default::default()
        },
        Default::default(),
        false,
    ));
    app.tab.selected = vec![1];
    app.sync_typography();
    app.caption_action(Action::Move(Motion::DocumentStart));
    app.caption_action(Action::Select(Motion::Right));
    app.caption_action(Action::Select(Motion::Right));
    assert_eq!(app.text_range(), Some(0..2));
    let original = app.tab.doc.clone();
    let red = reshiki::palette::Color::Custom([180, 50, 55]);
    let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
    assert_eq!(app.tab.doc.annotations[0].format.at(0).color, red);
    assert_eq!(
        app.tab.doc.annotations[0].format.at(3).color,
        reshiki::palette::Color::Ink
    );
    assert_eq!(app.tab.doc.graphics, original.graphics);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    // A stale range in the inspector must not constrain Select All.
    let _ = app.update(Message::SelectAll);
    let _ = app.update(Message::ColorScope(ColorScope::Text));
    let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
    assert_eq!(app.tab.doc.graphics, original.graphics);
    assert_eq!(app.tab.doc.annotations[0].format.at(3).color, red);
    let _ = app.update(Message::ColorScope(ColorScope::All));
    let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
    assert_eq!(app.tab.doc.graphics[0].style.stroke, red);
    assert_eq!(app.tab.doc.graphics[0].style.fill, Some(red));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.graphics, original.graphics);
}

#[test]
fn palette_scopes_recolor_selected_bonds_and_objects_in_one_undo() {
    use reshiki::typography::StyleChange;
    use typography::ColorScope;
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    let c = app.tab.doc.add_atom("N", Point::new(84., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(b, c, 2, "plain");
    app.tab.doc.arrows.push(Arrow::new(
        4,
        Point::new(0., 80.),
        Point::new(84., 80.),
        Default::default(),
        Default::default(),
    ));
    app.tab.doc.annotations.push(Annotation {
        id: 5,
        position: Point::new(0., 120.),
        text: "Label".into(),
        format: Default::default(),
    });
    app.tab.doc.atom_mut(b).unwrap().display.number = Some(reshiki::atom_labels::Number {
        text: "2".into(),
        offset: None,
        style: reshiki::atom_labels::number_style(),
    });
    let original = app.tab.doc.clone();
    let blue = reshiki::palette::Color::Palette(
        reshiki::palette::Hue::Blue,
        reshiki::palette::Row::Strong,
    );
    // Typed colors are custom and exact.
    let red = reshiki::palette::Color::Custom([180, 50, 55]);
    let _ = app.update(Message::SelectAll);
    let _ = app.update(Message::TextStyle(StyleChange::Color(blue)));
    assert!(
        app.tab
            .doc
            .bonds
            .iter()
            .all(|b| b.color == blue && b.indicator.style.color == blue)
    );
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .all(|a| a.text_style.as_ref().unwrap().color == blue)
    );
    assert_eq!(
        app.tab
            .doc
            .atom(b)
            .unwrap()
            .display
            .number
            .as_ref()
            .unwrap()
            .style
            .color,
        blue
    );
    assert_eq!(app.tab.doc.arrows[0].appearance().color, blue);
    assert_eq!(app.tab.doc.annotations[0].format.style.color, blue);
    let recolored = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, recolored);
    app.tab.selected = vec![a, b];
    let _ = app.update(Message::ColorScope(ColorScope::Bonds));
    let _ = app.update(Message::TextColor("#B43237".into()));
    let _ = app.update(Message::ApplyTextColor);
    assert_eq!(app.tab.doc.bonds[0].color, red);
    assert_eq!(app.tab.doc.bonds[1].color, blue);
    assert_eq!(
        app.tab.doc.atoms[1].text_style.as_ref().unwrap().color,
        blue
    );
    assert_eq!(app.tab.doc.arrows, recolored.arrows);
    let _ = app.update(Message::ColorScope(ColorScope::Text));
    let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
    assert_eq!(app.tab.doc.atoms[0].text_style.as_ref().unwrap().color, red);
    assert_eq!(app.tab.doc.atoms[2], recolored.atoms[2]);
    assert_eq!(app.tab.doc.bonds[1].color, blue);
    assert_eq!(app.tab.doc.arrows, recolored.arrows);
    // Selecting only one end of a bond does not recolor that bond.
    app.tab.selected = vec![c];
    let _ = app.update(Message::ColorScope(ColorScope::All));
    let _ = app.update(Message::TextStyle(StyleChange::Color(red)));
    assert_eq!(app.tab.doc.bonds[1].color, blue);
}

#[test]
fn bond_styles_position_color_and_direction_are_undoable() {
    use reshiki::bonds::{BondPreset, DoublePosition};
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("C", Point::new(0., 0.));
    app.tab.doc.add_atom("C", Point::new(84., 0.));
    app.tab.doc.add_atom("C", Point::new(168., 0.));
    app.tab.doc.add_bond(1, 2, 2, "plain");
    app.tab.doc.add_bond(2, 3, 1, "plain");
    app.tab.selected = vec![1, 2];
    let other = app.tab.doc.bonds[1].clone();
    let _ = app.update(Message::BondPosition(DoublePosition::Left));
    let _ = app.update(Message::BondColor("#205091".into()));
    let _ = app.update(Message::ApplyBondColor);
    assert_eq!(
        app.tab.doc.bonds[0].color,
        reshiki::palette::Color::Custom([32, 80, 145])
    );
    assert_eq!(app.tab.doc.bonds[1], other);
    let _ = app.update(Message::ApplyBondPreset(BondPreset::HollowWedge));
    assert_eq!(app.tab.doc.bonds[0].display, "hollow_wedge");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.bonds[0].order, 2);
    let _ = app.update(Message::Redo);
    app.tool = Tool::StyledBond(BondPreset::HollowWedge);
    app.edit(Edit::Click(Point::new(42., 0.)));
    assert_eq!((app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b), (2, 1));
    let _ = app.update(Message::Undo);
    assert_eq!((app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b), (1, 2));
    let before = app.tab.doc.clone();
    app.tool = Tool::StyledBond(BondPreset::Dotted);
    app.edit(Edit::Bond(
        Point::new(0., 0.),
        Point::new(168., 0.),
        Some(1),
        Some(3),
    ));
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn aromatic_bond_tools_preserve_circles_and_undo_direction_changes() -> anyhow::Result<()> {
    use anyhow::Context;
    use reshiki::bonds::BondPreset as P;
    for preset in [P::Wedge, P::HashedWedge, P::HollowWedge, P::Bold, P::Hashed] {
        let (mut app, _) = App::new();
        let ids = editing::ring(&mut app.tab.doc, Point::default(), 6, true, 0.);
        reshiki::projection::tilt(&mut app.tab.doc, &ids, 35., true);
        app.tab.doc.reconcile_molecule_groups();
        let source = app.tab.doc.clone();
        let bond = source.bonds.first().context("Missing ring edge")?;
        let a = source.atom(bond.a).context("Missing ring atom")?.position;
        let b = source.atom(bond.b).context("Missing ring atom")?.position;
        let midpoint = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
        app.tab.camera.zoom = 2.;
        app.tool = match preset {
            P::Wedge => Tool::Wedge,
            P::HashedWedge => Tool::Hash,
            p => Tool::StyledBond(p),
        };
        app.edit(Edit::Click(midpoint));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.bonds[0].display, preset.parts().1);
        assert!(app.tab.doc.bonds.iter().all(|b| b.order == 4));
        assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
        assert!(!chemistry_changed(&source, &app.tab.doc));
        assert_eq!(app.tab.doc.atoms, source.atoms);
        let styled = app.tab.doc.clone();
        app.edit(Edit::Click(midpoint));
        assert_eq!(
            (app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b),
            (bond.b, bond.a)
        );
        assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
        assert!(!chemistry_changed(&source, &app.tab.doc));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, styled);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, source);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, styled);
        app.tool = Tool::Bond(1);
        app.edit(Edit::Click(midpoint));
        assert_eq!(
            app.tab.doc, source,
            "Plain appearance retains aromatic order"
        );
        app.tool = Tool::Bond(2);
        app.edit(Edit::Click(midpoint));
        assert_eq!(
            app.tab.doc.bonds[0].order, 2,
            "Explicit double order still works"
        );
    }
    Ok(())
}

#[test]
fn aromatic_bond_properties_and_dragging_keep_ring_chemistry() -> anyhow::Result<()> {
    use anyhow::Context;
    use reshiki::bonds::BondPreset as P;
    for preset in [
        P::Wedge,
        P::HashedWedge,
        P::HollowWedge,
        P::Bold,
        P::Hashed,
        P::Wavy,
        P::Single,
    ] {
        let (mut app, _) = App::new();
        app.tab.selected = editing::ring(&mut app.tab.doc, Point::default(), 6, true, 0.);
        reshiki::projection::tilt(&mut app.tab.doc, &app.tab.selected, 65., true);
        app.tab.doc.reconcile_molecule_groups();
        let source = app.tab.doc.clone();
        let _ = app.update(Message::ApplyBondPreset(preset));
        assert!(!app.error, "{}", app.status);
        assert!(app.tab.doc.bonds.iter().all(|b| b.order == 4));
        assert!(
            app.tab
                .doc
                .bonds
                .iter()
                .all(|b| b.display == preset.parts().1)
        );
        assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
        assert!(!chemistry_changed(&source, &app.tab.doc));
        assert_eq!(app.tab.doc.atoms, source.atoms);
        if preset != P::Single {
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc, source);
        }
        let bond = source.bonds.first().context("Missing ring bond")?;
        let a = source.atom(bond.a).context("Missing ring atom")?.position;
        let b = source.atom(bond.b).context("Missing ring atom")?.position;
        app.tool = Tool::StyledBond(preset);
        app.edit(Edit::Bond(b, a, Some(bond.b), Some(bond.a)));
        assert!(!app.error, "{}", app.status);
        assert_eq!(
            (app.tab.doc.bonds[0].a, app.tab.doc.bonds[0].b),
            (bond.b, bond.a)
        );
        assert!(app.tab.doc.bonds.iter().all(|b| b.order == 4));
        assert_eq!(reshiki::aromatic::circles(&app.tab.doc).len(), 1);
        assert!(!chemistry_changed(&source, &app.tab.doc));
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, source);
    }
    Ok(())
}

#[test]
fn group_frame_and_ungroup_are_individually_undoable() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a];
    let initial = app.tab.doc.clone();
    let _ = app.update(Message::AddFrame(reshiki::graphics::GraphicKind::Brackets));
    assert_eq!(app.tab.doc.groups.len(), 1);
    assert_eq!(app.tab.doc.graphics.len(), 1);
    assert_eq!(app.tab.selected.len(), 3);
    assert_eq!(app.tab.doc.atoms, initial.atoms);
    let framed = app.tab.doc.clone();
    let _ = app.update(Message::Ungroup);
    assert!(app.tab.doc.groups.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, framed);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, framed);
    assert_eq!(app.tab.selected.len(), 3);
    let _ = app.update(Message::InvertSelection);
    assert!(app.tab.selected.is_empty());
}

#[tokio::test]
async fn graphic_style_point_edits_and_undo_retain_editable_selection() {
    use reshiki::graphics::GraphicKind;
    let (mut app, _) = App::new();
    let result = app
        .engine
        .execute(Request::import_smiles("CCO"))
        .await
        .unwrap();
    app.tab.doc = result.document.unwrap();
    app.tab.analysis = result.analysis;
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Curve)));
    app.edit(Edit::Graphic(
        Point::default(),
        Point::new(100., 40.),
        false,
    ));
    let id = app.tab.doc.graphics[0].id;
    assert_eq!(app.tool, Tool::Select);
    app.apply_graphic_style(GraphicChange::Stroke(reshiki::palette::Color::Custom([
        32, 80, 145,
    ])));
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Tool(Tool::EditPoints));
    app.edit(Edit::GraphicPoint(id, 1, Point::new(20., -50.)));
    assert_eq!(app.tab.doc.graphics[0].kind, GraphicKind::Path);
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, vec![id]);
    assert_eq!(app.tab.analysis.as_ref().unwrap().formula, "C2H6O");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
    assert_eq!(app.tab.selected, vec![id]);
    assert!(app.tab.analysis.is_some());
    let _ = app.update(Message::Tool(Tool::Graphic(GraphicKind::Ellipse)));
    assert!(app.tab.selected.is_empty());
    app.apply_graphic_style(GraphicChange::Stroke(reshiki::palette::Color::Custom([
        180, 50, 55,
    ])));
    assert_eq!(
        app.tab.doc, after,
        "new drawing style must not change the previous object"
    );
}

#[test]
fn partial_typography_edit_and_repeated_backspace_restore_with_undo() {
    use iced::widget::text_editor::{Action, Edit as TextEdit, Motion};
    use reshiki::typography::{StyleChange, TextFormat};
    let (mut app, _) = App::new();
    app.tab.doc.annotations.push(Annotation {
        id: 1,
        position: Point::default(),
        text: "AAA".into(),
        format: TextFormat::default(),
    });
    app.tab.selected = vec![1];
    app.sync_typography();
    app.caption_action(Action::Move(Motion::DocumentStart));
    app.caption_action(Action::Move(Motion::Right));
    app.caption_action(Action::Select(Motion::Right));
    assert_eq!(app.text_range(), Some(1..2));
    app.apply_text_style(StyleChange::Bold(true));
    assert!(!app.tab.doc.annotations[0].format.at(0).bold);
    assert!(app.tab.doc.annotations[0].format.at(1).bold);
    let styled = app.tab.doc.clone();
    app.caption_action(Action::Move(Motion::DocumentStart));
    app.caption_action(Action::Move(Motion::Right));
    app.caption_action(Action::Edit(TextEdit::Backspace));
    assert_eq!(app.tab.doc.annotations[0].text, "AA");
    assert!(app.tab.doc.annotations[0].format.at(0).bold);
    assert!(!app.tab.doc.annotations[0].format.at(1).bold);
    let edited = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, styled);
    assert_eq!(app.tab.caption, "AAA");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, edited);
    assert_eq!(app.tab.caption, "AA");
}

#[test]
fn journal_template_placement_matches_preview_and_has_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.update(Message::QuickDrawingStyle(
        document_styles::Choice::Journal(reshiki::document_styles::Preset::Nature),
    ));
    let before = app.tab.doc.clone();
    let index = reshiki::templates::LIBRARY
        .iter()
        .position(|t| t.name == "Cyclohexane")
        .unwrap();
    let _ = app.update(Message::InsertTemplate(index));
    let point = Point::new(160., 120.);
    let direction = Some(point.offset(35., 60.));
    let (preview, ids) = app
        .templates
        .library
        .get(index)
        .unwrap()
        .place(
            &before,
            point,
            direction,
            10. / app.tab.camera.zoom,
            app.templates.anchor,
            app.templates.connection,
        )
        .unwrap();
    app.edit(Edit::Template(point, direction));
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.selected, ids);
    let placed = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, placed);
}

#[test]
fn one_off_template_choice_is_nonmutating_and_attachment_is_one_undo_step() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::new(-30.0, 0.0));
    let b = app.tab.doc.add_atom("C", Point::new(30.0, 0.0));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.saved = app.tab.doc.clone();
    let before = app.tab.doc.clone();
    let index = reshiki::templates::LIBRARY
        .iter()
        .position(|t| t.name == "Cyclopentane")
        .unwrap();
    let _ = app.update(Message::InsertTemplate(index));
    assert_eq!(app.tab.doc, before);
    assert!(!app.dirty());
    assert_eq!(app.tool, Tool::Template);
    let _ = app.update(Message::Templates(template_library::Action::Repeat(false)));
    app.templates.connection = reshiki::templates::Connection::FuseBond;
    app.edit(Edit::Template(Point::default(), None));
    assert_eq!(app.tool, Tool::Select);
    let placed = app.tab.doc.clone();
    assert_eq!(placed.atoms.len(), 5);
    assert_eq!(placed.bonds.len(), 5);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, placed);
    let _ = app.update(Message::Tool(Tool::Select));
    app.edit(Edit::Template(Point::new(500.0, 500.0), None));
    assert_eq!(app.tab.doc, placed);
}

#[test]
fn templates_repeat_bond_fusion_by_default_until_cancelled() {
    use reshiki::templates::{Anchor, Connection};
    use template_library::Action as A;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::new(0., -21.));
    let b = app.tab.doc.add_atom("C", Point::new(0., 21.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let index = reshiki::templates::LIBRARY
        .iter()
        .position(|t| t.name == "Cyclohexane")
        .unwrap();
    let source = &reshiki::templates::LIBRARY[index].document.bonds[0];
    let anchor = Anchor::Bond(source.a, source.b);
    let _ = app.update(Message::InsertTemplate(index));
    let _ = app.update(Message::Templates(A::Anchor(anchor)));
    assert!(app.templates.repeat);
    let mut snapshots = vec![app.tab.doc.clone()];
    for (atoms, bonds) in [(6, 6), (10, 11), (14, 16)] {
        let point = app
            .tab
            .doc
            .bonds
            .iter()
            .map(|bond| {
                let p = app.tab.doc.atom(bond.a).unwrap().position;
                let q = app.tab.doc.atom(bond.b).unwrap().position;
                Point::new((p.x + q.x) / 2., (p.y + q.y) / 2.)
            })
            .max_by(|p, q| p.x.total_cmp(&q.x))
            .unwrap();
        app.edit(Edit::Template(point, None));
        assert!(!app.error, "{}", app.status);
        assert_eq!(
            (app.tab.doc.atoms.len(), app.tab.doc.bonds.len()),
            (atoms, bonds)
        );
        assert_eq!(app.tool, Tool::Template);
        assert_eq!(app.templates.anchor, anchor);
        assert_eq!(app.templates.connection, Connection::FuseBond);
        app.tab.doc.validate().unwrap();
        snapshots.push(app.tab.doc.clone());
    }
    // A misplaced click must preserve both the drawing and placement mode.
    app.edit(Edit::Template(app.tab.doc.atoms[0].position, None));
    assert!(app.error);
    assert_eq!(app.tab.doc, snapshots[3]);
    assert_eq!(app.tool, Tool::Template);
    for document in snapshots[..3].iter().rev() {
        let _ = app.update(Message::Undo);
        assert_eq!(&app.tab.doc, document);
        assert_eq!(app.tool, Tool::Template);
    }
    for document in &snapshots[1..] {
        let _ = app.update(Message::Redo);
        assert_eq!(&app.tab.doc, document);
    }
    let _ = app.update(Message::Escape);
    assert_eq!(app.tool, Tool::Select);
    app.edit(Edit::Template(Point::new(500., 500.), None));
    assert_eq!(app.tab.doc, snapshots[3]);
}

#[test]
fn axis_resize_has_one_step_undo_and_ignores_invalid_scales() -> Result<(), String> {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::new(-20., -10.));
    let b = app.tab.doc.add_atom("C", Point::new(20., 10.));
    let remote = app.tab.doc.add_atom("O", Point::new(150., 80.));
    app.tab.doc.add_bond(a, b, 1, "wedge");
    let original = app.tab.doc.clone();
    app.edit(Edit::ScaleAxes {
        ids: vec![a, b],
        pivot: Point::new(-20., -10.),
        x: 2.,
        y: 1.,
    });
    assert_eq!(
        app.tab.doc.atom(b).ok_or("Atom")?.position,
        Point::new(60., 10.)
    );
    assert_eq!(app.tab.doc.atom(remote), original.atom(remote));
    assert_eq!(app.tab.doc.bonds, original.bonds);
    let resized = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, resized);
    for (x, y) in [(1., 1.), (f32::NAN, 1.), (0., 1.), (1., -1.)] {
        app.edit(Edit::ScaleAxes {
            ids: vec![a, b],
            pivot: Point::default(),
            x,
            y,
        });
        assert_eq!(app.tab.doc, resized);
    }
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc, original,
        "No-op drags do not consume undo steps"
    );
    Ok(())
}

#[test]
fn selection_handle_transforms_preserve_other_objects_and_undo_in_one_step() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::new(-20.0, -10.0));
    let b = app.tab.doc.add_atom("C", Point::new(20.0, 10.0));
    let other = app.tab.doc.add_atom("O", Point::new(150.0, 80.0));
    app.tab.doc.add_bond(a, b, 1, "wedge");
    let original = app.tab.doc.clone();
    app.edit(Edit::Transform {
        ids: vec![a, b],
        pivot: Point::new(-20.0, -10.0),
        scale: 2.0,
        rotation: 0.0,
    });
    let resized = app.tab.doc.clone();
    assert_eq!(
        resized.atom(a).unwrap().position,
        original.atom(a).unwrap().position
    );
    assert_eq!(resized.atom(b).unwrap().position, Point::new(60.0, 30.0));
    app.edit(Edit::Transform {
        ids: vec![a, b],
        pivot: Point::new(20.0, 10.0),
        scale: 1.0,
        rotation: 90.0,
    });
    let rotated = app.tab.doc.clone();
    assert!(
        rotated
            .atom(b)
            .unwrap()
            .position
            .distance(Point::new(0.0, 50.0))
            < 0.001
    );
    assert_eq!(rotated.atom(other), original.atom(other));
    assert_eq!(rotated.bonds, original.bonds);
    assert_eq!(app.tab.selected, vec![a, b]);
    for expected in [&resized, &original] {
        let _ = app.update(Message::Undo);
        assert_eq!(&app.tab.doc, expected);
    }
    for expected in [&resized, &rotated] {
        let _ = app.update(Message::Redo);
        assert_eq!(&app.tab.doc, expected);
    }
    app.edit(Edit::Transform {
        ids: vec![a, b],
        pivot: Point::default(),
        scale: 1.0,
        rotation: 0.0,
    });
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc, resized,
        "clicking a handle without dragging adds no history"
    );
    app.tool = Tool::Ring;
    let _ = app.update(Message::SelectAll);
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.tab.selected, app.tab.doc.all_ids());
}

#[test]
fn snapping_a_ring_is_one_undoable_edit_with_original_atom_ids_restored() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(60.0, 0.0));
    app.tab.doc.add_bond(a, b, 1, "plain");
    let ids = editing::ring(&mut app.tab.doc, Point::new(200.0, 200.0), 5, false, 5.0);
    let p = app.tab.doc.atom(ids[0]).unwrap().position;
    let q = app.tab.doc.atom(ids[1]).unwrap().position;
    let before = app.tab.doc.clone();
    app.edit(Edit::Move(
        ids,
        30.0 - (p.x + q.x) / 2.0,
        -(p.y + q.y) / 2.0,
    ));
    let snapped = app.tab.doc.clone();
    assert_eq!((snapped.atoms.len(), snapped.bonds.len()), (5, 5));
    assert_eq!(app.tab.selected.len(), 5);
    assert!(app.tab.selected.contains(&a) && app.tab.selected.contains(&b));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, snapped);
}

#[test]
fn a_smart_guide_drag_is_one_undoable_edit() {
    use reshiki::graphics::{Graphic, GraphicKind};
    let (mut app, _) = App::new();
    for (lo, hi) in [((-150., -100.), (-90., -60.)), ((0., 0.), (60., 40.))] {
        let id = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::dragged(
            id,
            GraphicKind::Rectangle,
            Point::new(lo.0, lo.1),
            Point::new(hi.0, hi.1),
            Default::default(),
            Default::default(),
            false,
        ));
    }
    let moving = app.tab.doc.graphics[1].id;
    let before = app.tab.doc.clone();
    let edits = crate::canvas::select_drag(
        &before,
        &[],
        Point::new(0., 20.),
        Point::new(-3., -77.),
        Default::default(),
    );
    for edit in edits {
        let _ = app.update(Message::Canvas(edit));
    }
    let mut expected = before.clone();
    expected.translate(&[moving], -3., -100.);
    assert_eq!(app.tab.doc, expected, "the top edges snapped together");
    assert_eq!(app.tab.selected, [moving]);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, expected);
}

#[test]
fn drag_duplicate_keeps_the_original_and_is_one_undoable_edit() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("O", Point::new(42.0, 0.0));
    let c = app.tab.doc.add_atom("N", Point::new(84.0, 0.0));
    app.tab.doc.add_bond(a, b, 2, "plain");
    app.tab.doc.add_bond(b, c, 1, "plain");
    let before = app.tab.doc.clone();
    app.edit(Edit::Duplicate(vec![a, b], 0.0, 90.0));
    let copied = app.tab.doc.clone();
    assert_eq!((copied.atoms.len(), copied.bonds.len()), (5, 3));
    for id in [a, b, c] {
        assert_eq!(copied.atom(id), before.atom(id), "originals stay in place");
    }
    assert_eq!(
        app.tab.selected.len(),
        2,
        "the copy of the two atoms is selected"
    );
    assert!(!app.tab.selected.iter().any(|id| [a, b, c].contains(id)));
    let symbols: Vec<_> = app
        .tab
        .selected
        .iter()
        .filter_map(|id| copied.atom(*id))
        .map(|atom| (atom.element.as_str(), atom.position))
        .collect();
    assert_eq!(
        symbols,
        vec![("C", Point::new(0.0, 90.0)), ("O", Point::new(42.0, 90.0))]
    );
    copied.validate().unwrap();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, copied);
}

#[test]
fn clicking_existing_bonds_cycles_order_and_can_be_undone() {
    for tool in [Tool::Bond(1), Tool::Bond(3)] {
        let (mut app, _) = App::new();
        app.tool = tool;
        let a = app.tab.doc.add_atom("C", Point::default());
        let b = app.tab.doc.add_atom("C", Point::new(42.0, 0.0));
        app.tab.doc.add_bond(a, b, 1, "plain");
        let original = app.tab.doc.clone();
        for order in [2, 3, 1] {
            app.edit(Edit::Click(Point::new(21.0, 0.0)));
            assert_eq!(app.tab.doc.atoms, original.atoms);
            assert_eq!(app.tab.doc.bonds.len(), 1);
            assert_eq!(app.tab.doc.bonds[0].order, order);
            assert_eq!(app.tab.doc.bonds[0].display, "plain");
        }
        assert_eq!(app.tab.doc, original);
        for order in [3, 2, 1] {
            let _ = app.update(Message::Undo);
            assert_eq!(app.tab.doc.bonds[0].order, order);
        }
        assert_eq!(app.tab.doc, original);
        app.tool = Tool::Wedge;
        app.edit(Edit::Click(Point::new(21.0, 0.0)));
        assert_eq!(app.tab.doc.bonds[0].order, 1);
        assert_eq!(app.tab.doc.bonds[0].display, "wedge");
    }
}

#[tokio::test]
async fn endpoint_clicks_grow_a_connected_zigzag_with_undo_and_redo() {
    let (mut app, _) = App::new();
    app.tool = Tool::Bond(1);
    app.edit(Edit::Click(Point::default()));
    for _ in 0..5 {
        let endpoint = app
            .tab
            .doc
            .atom(*app.tab.selected.first().unwrap())
            .unwrap()
            .position;
        app.edit(Edit::Click(endpoint));
    }
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (7, 6));
    for three in app.tab.doc.atoms.windows(3) {
        let a = three[0].position;
        let b = three[1].position;
        let c = three[2].position;
        let cosine = ((a.x - b.x) * (c.x - b.x) + (a.y - b.y) * (c.y - b.y))
            / (a.distance(b) * c.distance(b));
        assert!((cosine + 0.5).abs() < 0.001, "chain needs 120° junctions");
        assert!(c.x > b.x && b.x > a.x, "chain must keep extending forward");
        assert!((a.y - c.y).abs() < 0.001, "successive turns must alternate");
    }
    let complete = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (6, 5));
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, complete);
    let analysis = app
        .engine
        .execute(Request::molecule("analyze", complete))
        .await
        .unwrap()
        .analysis
        .unwrap();
    assert_eq!(analysis.smiles, "CCCCCCC");
    assert_eq!(analysis.formula, "C7H16");
}

#[test]
fn aromatic_plane_bond_matches_preview_and_undo_restores_xyz() -> Result<(), String> {
    use reshiki::projection::growth::{self, Plane};
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let ids = app.tab.doc.all_ids();
    reshiki::projection::tilt(&mut app.tab.doc, &ids, 55., false);
    let id = app.tab.doc.atoms.get(1).ok_or("Carbon")?.id;
    let end = Plane::at(&app.tab.doc, id)
        .ok_or("Plane")?
        .outward(42.)
        .ok_or("Endpoint")?;
    let before = app.tab.doc.clone();
    app.tool = Tool::Atom;
    app.element = "O".into();
    let (preview, added) =
        growth::place(&before, id, end, "O", reshiki::bonds::BondPreset::Single)?;
    app.edit(Edit::PlaneBond(id, end));
    assert_eq!(app.tab.doc, preview);
    assert_eq!(app.tab.selected, vec![added]);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, preview);
    Ok(())
}

#[test]
fn bond_tools_grow_carbon_after_using_an_atom_label_and_still_edit_bonds() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::Element("O".into()));
    app.edit(Edit::Click(Point::default()));
    let oxygen = app.tab.doc.atoms[0].id;
    let _ = app.update(Message::Tool(Tool::Bond(1)));
    app.edit(Edit::Click(Point::default()));
    let carbon = app.tab.doc.atoms[1].clone();
    assert_eq!(carbon.element, "C");
    assert_eq!(app.tab.doc.atom(oxygen).unwrap().element, "O");
    app.edit(Edit::Bond(
        carbon.position,
        carbon.position.offset(36.373066, 21.0),
        Some(carbon.id),
        None,
    ));
    assert_eq!(app.tab.doc.atoms[2].element, "C");
    app.tool = Tool::Bond(2);
    app.edit(Edit::Click(Point::new(
        carbon.position.x / 2.0,
        carbon.position.y / 2.0,
    )));
    assert_eq!((app.tab.doc.atoms.len(), app.tab.doc.bonds.len()), (3, 2));
    assert_eq!(app.tab.doc.bonds[0].order, 2);
}

#[test]
fn blank_drawings_keep_starting_zoom_through_resize_and_first_edits() {
    let (mut app, _) = App::new();
    assert_eq!(app.tab.camera.zoom, 1.0);
    let _ = app.update(Message::Viewport(iced::Size::new(1000., 700.)));
    assert_eq!(app.tab.camera.zoom, 1.0);

    let _ = app.update(Message::Tool(Tool::Atom));
    app.edit(Edit::Click(Point::default()));
    assert!(!app.tab.doc.all_ids().is_empty());
    let _ = app.update(Message::Viewport(iced::Size::new(700., 500.)));
    assert_eq!(app.tab.camera.zoom, 1.0);

    let _ = app.update(Message::Zoom(1.2));
    let _ = app.update(Message::Viewport(iced::Size::new(1000., 700.)));
    assert_eq!(app.tab.camera.zoom, 1.2);

    let _ = app.update(Message::New);
    assert!(app.tab.doc.all_ids().is_empty());
    assert_eq!(app.tab.camera.zoom, 1.0);
    let _ = app.update(Message::Viewport(iced::Size::new(700., 500.)));
    assert_eq!(app.tab.camera.zoom, 1.0);
}

#[test]
fn fitting_an_empty_drawing_restores_starting_view() {
    let (mut app, _) = App::new();
    app.edit(Edit::Pan(60., -20.));
    let _ = app.update(Message::Zoom(2.));
    let _ = app.update(Message::Fit);
    assert_eq!(app.tab.camera.zoom, 1.0);
    assert_eq!(app.tab.camera.center, Point::default());
    let _ = app.update(Message::Viewport(iced::Size::new(1000., 700.)));
    assert_eq!(app.tab.camera.zoom, 1.0);
}

#[test]
fn fit_uses_available_canvas_and_respects_manual_pan() {
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("C", Point::new(-250.0, -100.0));
    app.tab.doc.add_atom("O", Point::new(250.0, 100.0));
    let document = app.tab.doc.clone();
    let _ = app.update(Message::Viewport(iced::Size::new(600.0, 400.0)));
    let _ = app.update(Message::Fit);
    let small_zoom = app.tab.camera.zoom;
    let _ = app.update(Message::Viewport(iced::Size::new(1000.0, 700.0)));
    assert!(app.tab.camera.zoom > small_zoom);
    app.edit(Edit::Pan(60.0, -20.0));
    let camera = app.tab.camera;
    let _ = app.update(Message::Viewport(iced::Size::new(700.0, 500.0)));
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.doc, document);
}

#[test]
fn view_aids_preserve_drawing_selection_history_and_manual_camera() {
    let (mut app, _) = App::new();
    let id = app.tab.doc.add_atom("O", Point::new(50., 20.));
    app.tab.selected = vec![id];
    app.edit(Edit::Pan(60., -20.));
    let document = app.tab.doc.clone();
    let camera = app.tab.camera;
    let revision = app.tab.revision;
    let history = app.tab.history.can_undo();
    let export = reshiki::export::drawing(&app.tab.doc, "svg").expect("SVG before view change");
    for message in [
        Message::View(view_settings::Action::Toggle),
        Message::View(view_settings::Action::Rulers(true)),
        Message::View(view_settings::Action::Crosshair(true)),
        Message::View(view_settings::Action::RulerUnit(
            canvas::guides::Unit::Inches,
        )),
        Message::View(view_settings::Action::Grid),
        Message::View(view_settings::Action::SmartGuides(false)),
    ] {
        let _ = app.update(message);
    }
    // Smart guides are on by default, also for settings saved before them.
    assert!(!app.appearance.smart_guides);
    let legacy: crate::appearance::Settings =
        serde_json::from_str(r#"{"mode":"light","arrange_controls":false}"#).unwrap();
    assert!(legacy.smart_guides && !legacy.arrange_controls);
    assert_eq!(app.tab.doc, document);
    assert_eq!(app.tab.selected, [id]);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.history.can_undo(), history);
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(
        reshiki::export::drawing(&app.tab.doc, "svg").expect("SVG after view change"),
        export
    );
}

#[test]
fn view_messages_and_shortcuts_set_their_view_aids() {
    let (mut app, _) = App::new();
    let (grid, view_open) = (app.grid, app.view_open);
    let _ = app.update(Message::View(view_settings::Action::Grid));
    assert_eq!(app.grid, !grid);
    let _ = app.update(Message::View(view_settings::Action::Toggle));
    assert_eq!(app.view_open, !view_open);
    let _ = app.update(Message::View(view_settings::Action::Rulers(true)));
    assert!(app.guides.rulers);
    let _ = app.update(Message::View(view_settings::Action::Crosshair(true)));
    assert!(app.guides.crosshair);
    let _ = app.update(Message::View(view_settings::Action::RulerUnit(
        canvas::guides::Unit::Inches,
    )));
    assert_eq!(app.guides.unit, canvas::guides::Unit::Inches);
    let _ = app.update(Message::Shortcut(shortcuts::Action::Rulers));
    assert!(!app.guides.rulers);
    let _ = app.update(Message::Shortcut(shortcuts::Action::Crosshair));
    assert!(!app.guides.crosshair);
}

#[test]
fn cleanup_orientation_restarts_the_previewed_job() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.cleanup = Some(CleanupPreview {
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
    });
    let serial = app.tab.cleanup_serial;
    let _ = app.update(Message::Cleanup(cleanup::Action::Orientation(true)));
    assert!(app.tab.busy);
    assert_eq!(app.tab.cleanup_serial, serial.wrapping_add(1));
    assert!(app.tab.cleanup.as_ref().is_some_and(|p| !p.original));
}

#[test]
fn native_extensions_open_the_same_editable_document_and_keep_the_path() {
    let mut document: Document = serde_json::from_str(include_str!(
        "../../tests/fixtures/ui-drawn-ethanol.reshiki"
    ))
    .unwrap();
    document.version = 15;
    reshiki::atom_labels::clear_computed(&mut document);
    let contents = serde_json::to_string_pretty(&document).unwrap();
    for extension in ["rsk", "RSK", "reshiki", "moruno"] {
        let (mut app, _) = App::new();
        let path = PathBuf::from(format!("Ethanol.{extension}"));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let opened = runtime.block_on(files::prepare_contents(
            path.clone(),
            Ok(contents.clone().into_bytes()),
        ));
        let _ = app.update(Message::FilePrepared(opened));
        assert!(!app.error && !app.dirty(), "{extension}: {}", app.status);
        assert_eq!(app.tab.doc, document);
        assert_eq!(app.tab.path, Some(path));
        assert_eq!(app.status, "Document opened");
        assert!(app.tab.fit_to_view);
        assert!(app.tab.camera.zoom > Camera::default().zoom);
        assert!(app.tab.camera.zoom <= 2.5);
    }
}

#[test]
fn drawings_from_a_newer_reshiki_ask_for_an_update_and_keep_the_current_drawing() {
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("O", Point::default());
    let before = app.tab.doc.clone();
    let newer = format!(
        r#"{{"version": {}, "atoms": [], "bonds": [], "future": "blue.strong"}}"#,
        reshiki::document::VERSION + 1
    );
    let path = PathBuf::from("newer.rsk");
    let task = app.update(Message::Opened(Some((
        path.clone(),
        Ok(newer.clone().into_bytes()),
    ))));
    assert!(task.units() > 0);
    files::finish_dispatched_open(&mut app, path, Ok(newer.into_bytes()));
    assert!(app.error);
    assert!(
            app.status.ends_with(&format!(
                "This drawing was made with a newer version of ReShiki (document version {}). Update ReShiki to open it.",
                reshiki::document::VERSION + 1
            )),
            "{}",
            app.status
        );
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.path, None);
}

#[test]
fn late_save_does_not_mark_newer_edits_as_saved() {
    let (mut app, _) = App::new();
    let snapshot = app.tab.doc.clone();
    app.tab.doc.add_atom("O", Point::default());
    let _ = app.update(Message::Saved(
        0,
        Box::new(snapshot),
        Ok(Some("example.reshiki".into())),
    ));
    assert!(app.dirty());
}

#[test]
fn unsaved_changes_wait_for_one_save_dialog_answer() {
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("O", Point::default());
    let edited = app.tab.doc.clone();
    let window = iced::window::Id::unique();
    assert!(app.update(Message::Close(window)).units() > 0);
    assert!(matches!(app.pending, Some(Pending::CloseWindow(id, ..)) if id == window));
    // Further requests are ignored while the dialog is open.
    for message in [
        Message::Close(window),
        Message::New,
        Message::Open,
        Message::Tabs(tabs::Action::Close(None)),
    ] {
        assert_eq!(app.update(message).units(), 0);
    }
    assert!(matches!(app.pending, Some(Pending::CloseWindow(..))));
    assert_eq!(app.strip().count(), 1);
    let _ = app.update(Message::Cancel);
    assert!(app.pending.is_none());
    assert_eq!(app.tab.doc, edited);
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    assert!(matches!(app.pending, Some(Pending::CloseTab(_))));
    let _ = app.update(Message::Discard);
    assert!(app.pending.is_none());
    assert!(
        app.tab.doc.all_ids().is_empty(),
        "The last tab gives way to an empty one"
    );
}

#[test]
fn save_answer_closes_the_tab_only_after_it_is_saved() {
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("O", Point::default());
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    // Cancelling Save As cancels the close too, so a later save does not continue it.
    let epoch = app.tab.file_epoch;
    let _ = app.update(Message::Saved(
        epoch,
        Box::new(app.tab.doc.clone()),
        Ok(None),
    ));
    assert!(app.pending.is_none());
    let saved = Ok(Some(PathBuf::from("drawing.reshiki")));
    let _ = app.update(Message::Saved(
        epoch,
        Box::new(app.tab.doc.clone()),
        saved.clone(),
    ));
    assert_eq!(app.tab.doc.atoms.len(), 1);
    app.tab.doc.add_atom("N", Point::new(80., 0.));
    let _ = app.update(Message::Tabs(tabs::Action::Close(None)));
    let _ = app.update(Message::Saved(epoch, Box::new(app.tab.doc.clone()), saved));
    assert!(app.pending.is_none());
    assert!(app.tab.doc.all_ids().is_empty() && app.tab.path.is_none());
}

#[test]
fn save_answer_passes_an_open_atom_editor() {
    let (mut app, _) = App::new();
    let atom = app.tab.doc.add_atom("C", Point::default());
    app.tab.selected = vec![atom];
    let _ = app.update(Message::AtomText(atom_text::Action::Begin(None)));
    assert_eq!(app.update(Message::Save).units(), 0);
    let _ = app.update(Message::Close(iced::window::Id::unique()));
    assert!(app.pending.is_some());
    assert!(app.update(Message::Save).units() > 0);
}

#[test]
fn late_save_does_not_retarget_another_document() {
    let (mut app, _) = App::new();
    let snapshot = app.tab.doc.clone();
    let _ = app.update(Message::New);
    let _ = app.update(Message::Saved(
        0,
        Box::new(snapshot),
        Ok(Some("previous.reshiki".into())),
    ));
    assert!(app.tab.path.is_none());
}

#[test]
fn startup_is_a_blank_saved_canvas_without_an_import_job() {
    let (app, _) = App::new();
    assert_eq!(app.tab.doc, Document::default());
    assert!(app.tab.doc.all_ids().is_empty());
    assert!(!app.tab.busy && !app.dirty() && !app.tab.history.can_undo());
    assert!(app.tab.analysis.is_none() && app.tab.path.is_none());
    assert!(app.imports.is_blank());
}

#[test]
fn new_document_invalidates_inflight_import_even_when_empty() {
    let (mut app, _) = App::new();
    let revision = app.tab.revision;
    let _ = app.update(Message::New);
    let mut old = Document::default();
    old.add_atom("O", Point::default());
    let _ = app.update(Message::EngineDone {
        revision,
        kind: Job::Import,
        result: Box::new(Ok(Response {
            document: Some(old),
            analysis: None,
            output: None,
            engine_version: "test".into(),
            warnings: vec![],
        })),
    });
    assert!(app.tab.doc.atoms.is_empty());
}

#[test]
fn ring_tool_click_commits_exactly_one_history_step() {
    // The Ring click re-enters edit() with Edit::Ring, which commits on its own;
    // the click handler must not add a second step for the same change.
    let (mut app, _) = App::new();
    let _ = app.update(Message::New);
    app.tab.history = History::default();
    let blank = app.tab.doc.clone();
    app.tool = Tool::Ring;
    app.edit(Edit::Click(Point::new(120., 80.)));
    assert!(!app.tab.doc.atoms.is_empty());
    assert!(app.tab.history.undo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, blank);
    assert!(
        !app.tab.history.can_undo(),
        "one ring click is one undo step"
    );
}

#[test]
fn dotted_click_reports_the_drag_hint_without_editing_or_dropping_redo() {
    use reshiki::bonds::BondPreset;
    let (mut app, _) = App::new();
    let _ = app.update(Message::New);
    // A stale centroid makes a fall-through observable: changed() would re-sync
    // it, edit the drawing, commit and drop redo.
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(100., 0.));
    let centroid = reshiki::projection::add_centroid(&mut app.tab.doc, &[a, b]).unwrap();
    app.tab.doc.atom_mut(centroid).unwrap().position = Point::new(0., 30.);
    app.tab.history = History::default();
    app.tool = Tool::Atom;
    app.edit(Edit::Click(Point::new(300., 0.)));
    let _ = app.update(Message::Undo);
    assert!(app.tab.history.can_redo());
    assert_eq!(
        app.tab.doc.atom(centroid).unwrap().position,
        Point::new(0., 30.),
        "undo restores the stale centroid"
    );
    let before = app.tab.doc.clone();
    app.tool = Tool::StyledBond(BondPreset::Dotted);
    app.edit(Edit::Click(Point::new(42., 0.)));
    assert!(app.error);
    assert_eq!(
        app.status,
        "Drag from a bonded explicit H to an existing acceptor"
    );
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert!(app.tab.history.can_redo(), "a rejected click keeps redo");
}

#[test]
fn text_click_on_an_existing_label_selects_it_without_a_history_step() {
    let (mut app, _) = App::new();
    let _ = app.update(Message::New);
    let label = app.tab.doc.next_id();
    let position = Point::new(0., 60.);
    app.tab.doc.annotations.push(Annotation {
        id: label,
        position,
        text: "Label".into(),
        format: Default::default(),
    });
    app.tab.history = History::default();
    let before = app.tab.doc.clone();
    app.tab.caption = "New text".into();
    app.tool = Tool::Text;
    app.edit(Edit::Click(position));
    assert_eq!(app.tab.selected, [label]);
    assert_eq!(app.tab.doc, before, "no second label is placed");
    assert_eq!(app.tool, Tool::Text);
    assert!(!app.tab.history.can_undo());
}
