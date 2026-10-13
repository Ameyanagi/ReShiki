use super::*;
use reshiki::document::{Document, Point};

fn expand_drawing(source: Document) -> Result<PreparedExpansion, String> {
    reshiki::generic::expand_prepared(reshiki::generic::prepare_expansion_input(
        &source,
        &[],
        Limits::default(),
    )?)
}

fn prepared_document_for_test(prepared: PreparedExpansion) -> (Document, usize) {
    let insertion = reshiki::transaction::take_expansion(prepared).unwrap();
    let count = insertion.count();
    let mut doc = Document::default();
    let before = doc.clone();
    let accepted = reshiki::transaction::install_expansion(&mut doc, before, insertion).unwrap();
    accepted.commit(&mut doc, &mut reshiki::document::History::default(), false);
    (doc, count)
}

fn pending_expansion() -> (App, Message) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let carbon = app.tab.doc.add_atom("C", Point::default());
    let choice = app.tab.doc.add_atom("*", Point::new(42., 0.));
    app.tab.doc.add_bond(carbon, choice, 1, "plain");
    app.tab.doc = reshiki::atom_text::apply(
        &app.tab.doc,
        choice,
        "[Cl,Br]",
        reshiki::atom_text::Mode::Auto,
    )
    .unwrap();
    app.tab.selected = app.tab.doc.all_ids();
    let task = app.update(Message::Generic(Action::Expand));
    assert!(task.units() > 0);
    let (source, epoch) = app.generic.pending.unwrap();
    let result = expand_drawing(app.tab.doc.clone()).unwrap();
    (
        app,
        Message::Generic(Action::Expanded {
            source,
            epoch,
            result: Box::new(Ok(result)),
        }),
    )
}

fn open_optimization_preview(app: &mut App) {
    use crate::app::optimization;
    use reshiki::geometry::{Conformer, ForceField, Optimized, Point3};
    use std::sync::Arc;
    // Geometry preparation needs a concrete drawing; the expansion source stays behind it.
    let _ = app.update(Message::New);
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a];
    let _ = app.update(Message::Optimization(optimization::Action::Begin));
    let key = app.optimization_key_for_test().unwrap();
    let optimized = Optimized {
        conformer: Conformer {
            positions: vec![
                Point3::default(),
                Point3 {
                    x: 1.5,
                    y: 0.,
                    z: 0.4,
                },
            ],
            original_atom_count: 2,
            hydrogen_parents: vec![],
        },
        initial_energy: 2.,
        energy: 1.,
        gradient: None,
        converged: true,
        iterations: 20,
        force_field: ForceField::Mmff94s,
        diagnostics: vec![],
    };
    let _ = app.update(Message::Optimization(optimization::Action::WorkerDone(
        key,
        Ok(Arc::new(optimized)),
    )));
    assert!(app.tab.optimization.is_some());
    assert!(app.optimization_canvas().unwrap().interactive);
}

#[test]
fn update_discards_expansion_under_each_blocking_state_and_allows_retry() {
    use crate::app::{CleanupPreview, atom_text, cleanup, optimization, tabs, template_library};
    use reshiki::recovery::Recovery;
    for modal in [
        "atom text",
        "cleanup",
        "3D",
        "updates",
        "restarting",
        "exit",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, completion) = pending_expansion();
        let source = app.tab.id;
        let source_document = app.tab.doc.clone();
        match modal {
            "atom text" => {
                let _ = app.update(Message::AtomText(atom_text::Action::Begin(Some(
                    app.tab.doc.atoms[0].id,
                ))));
            }
            "cleanup" => {
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
            }
            "3D" => open_optimization_preview(&mut app),
            "updates" => app.updates.open = true,
            "restarting" => app.updates.restarting = true,
            "exit" => {
                // A real pending library write holds exit and its failure cancels close.
                let mut fragment = Document::default();
                fragment.add_atom("O", Point::default());
                app.templates.path = Some(directory.path().join("templates.json"));
                app.templates.draft = Some(fragment);
                app.templates.editing = true;
                app.templates.name = "Hydroxyl".into();
                let task = app.template_message(template_library::Action::SaveDetails);
                assert!(task.units() > 0 && app.templates.pending());
                app.tab.saved = app.tab.doc.clone();
                app.tab.recovery = Some(Recovery::in_directory(directory.path()).unwrap());
                let _ = app.update(Message::Close(iced::window::Id::unique()));
            }
            _ => unreachable!(),
        }
        assert!(app.generic_expansion_blocked(), "{modal}");
        let tabs = app.strip().count();
        let front = app.tab.id;
        let before = app.tab.doc.clone();
        app.error = true;
        let _ = app.update(completion);
        assert_eq!(app.strip().count(), tabs, "{modal}");
        assert_eq!(app.tab.id, front, "{modal}");
        assert_eq!(app.tab.doc, before, "{modal}");
        assert_eq!(app.status, EXPANSION_DISCARDED, "{modal}");
        assert!(!app.error, "{modal}");
        assert!(app.generic.pending.is_none(), "{modal}");
        assert!(app.tabs.deferred_results.is_empty(), "{modal}");
        match modal {
            "atom text" => {
                assert!(app.tab.atom_text.is_some());
                let _ = app.update(Message::AtomText(atom_text::Action::Cancel));
            }
            "cleanup" => {
                assert!(app.tab.cleanup.is_some());
                let _ = app.update(Message::Cleanup(cleanup::Action::Cancel));
            }
            "3D" => {
                assert!(app.tab.optimization.is_some());
                let _ = app.update(Message::Optimization(optimization::Action::Cancel));
                let _ = app.update(Message::Tabs(tabs::Action::Select(source)));
            }
            "updates" => {
                assert!(app.updates.open);
                let _ = app.update(Message::Escape);
            }
            "restarting" => {
                assert!(app.updates.restarting);
                // A failed restart clears restarting through its ordinary result handler.
                let _ = app.update(Message::Updates(crate::app::updates::Action::Restarted(
                    Err("Controlled restart failure".into()),
                )));
                assert!(!app.updates.restarting);
                assert!(app.updates.open);
                let _ = app.update(Message::Escape);
            }
            "exit" => {
                assert!(app.exit.frozen());
                let _ = app.update(Message::Templates(template_library::Action::Finished(
                    1,
                    Err("Controlled library write failure".into()),
                )));
                assert!(!app.exit.frozen());
            }
            _ => unreachable!(),
        }
        assert!(!app.generic_expansion_blocked(), "{modal}");
        assert_eq!(app.tab.id, source);
        assert_eq!(app.tab.doc, source_document);
        app.tab.selected = app.tab.doc.all_ids();
        let task = app.update(Message::Generic(Action::Expand));
        assert!(task.units() > 0, "{modal}: expand can start again");
        assert_eq!(app.generic.pending, Some((source, app.tab.file_epoch)));
    }
}

#[test]
fn update_installs_one_expansion_tab_and_keeps_later_gate_side_effects() {
    let (mut app, completion) = pending_expansion();
    let source = app.tab.id;
    let before = app.tab.doc.clone();
    let count = app.strip().count();
    app.style_menu = Some(crate::app::color_popover::Menu::Align);
    app.tabs.menu = true;
    app.imports.menu = true;
    let _ = app.update(completion);
    assert_eq!(app.strip().count(), count + 1);
    assert_ne!(app.tab.id, source);
    assert_eq!(app.tab.untitled_name, Some("Generic expansion"));
    assert_eq!(app.tab.doc.atoms.len(), 4);
    assert!(app.tab.doc.generic.is_empty());
    assert_eq!(app.tabs.background[0].doc, before);
    assert!(app.generic.pending.is_none());
    assert_eq!(
        app.status,
        "Expanded 2 concrete structures in a new drawing"
    );
    assert!(!app.error);
    assert!(app.style_menu.is_none());
    assert!(!app.tabs.menu && !app.imports.menu);
}

#[test]
fn blocked_stale_expansion_does_not_clear_newer_pending_or_change_status() {
    let (mut app, _) = pending_expansion();
    let (source, epoch) = app.generic.pending.unwrap();
    app.updates.open = true;
    app.status = "Keep the current status".into();
    app.error = true;
    let _ = app.update(Message::Generic(Action::Expanded {
        source,
        epoch: epoch.wrapping_add(1),
        result: Box::new(Err("Stale completion".into())),
    }));
    assert_eq!(app.generic.pending, Some((source, epoch)));
    assert_eq!(app.status, "Keep the current status");
    assert!(app.error);
    assert!(app.updates.open);
    assert_eq!(app.strip().count(), 1);
}

#[test]
fn generated_layout_preserves_manual_presentation_and_packs_ink_bounds() {
    use reshiki::atom_labels::{Carbons, HydrogenPosition, Number};
    let mut source = Document::default();
    let first = source.add_atom("C", Point::new(0., 0.));
    let middle = source.add_atom("C", Point::new(42., 0.));
    let last = source.add_atom("C", Point::new(84., 0.));
    source.add_bond(first, middle, 1, "plain");
    source.add_bond(middle, last, 1, "plain");
    source = reshiki::atom_text::apply(&source, middle, "[CH2]1-3", reshiki::atom_text::Mode::Auto)
        .unwrap();
    source.drawing_style.set_bond_length(100.);
    source.drawing_style.bond_spacing_ratio = 0.4;
    source.drawing_style.line_width_pt = 0.1;
    for b in &mut source.bonds {
        b.highlight = Some(reshiki::palette::Color::Custom([240, 210, 30]));
    }
    let styled = source.atom_mut(middle).unwrap();
    styled.text_style = Some(reshiki::typography::TextStyle {
        size_pt: 32.,
        bold: true,
        color: reshiki::palette::Color::Custom([34, 90, 180]),
        ..Default::default()
    });
    styled.display.carbons = Some(Carbons::All);
    styled.display.hydrogens = Some(true);
    styled.display.hydrogen_position = HydrogenPosition::Above;
    styled.display.color_override = true;
    styled.display.highlight = Some(reshiki::palette::Color::Custom([240, 210, 30]));
    styled.display.number = Some(Number {
        text: "site".into(),
        offset: Some(Point::new(0., -145.)),
        style: Default::default(),
    });
    let annotation = source.next_id();
    source.annotations.push(reshiki::document::Annotation {
        id: annotation,
        position: Point::new(-80., 140.),
        text: "manual caption".into(),
        format: Default::default(),
    });
    source.groups.push(reshiki::grouping::Group {
        id: source.next_id(),
        members: vec![first, middle, last, annotation],
        integral: false,
    });
    let before = source.clone();
    let products = reshiki::generic::expand_drawings(&source, Limits::default()).unwrap();
    let generated = prepared_document_for_test(expand_drawing(source.clone()).unwrap());
    assert_eq!(generated.1, 3);
    assert_eq!(source, before);
    let wanted = before.atom(middle).unwrap();
    for product in &products {
        for a in product.document.atoms.iter().filter(|a| a.explicit_h == 2) {
            assert_eq!(a.text_style, wanted.text_style);
            assert_eq!(a.display.carbons, wanted.display.carbons);
            assert_eq!(a.display.hydrogens, wanted.display.hydrogens);
            assert_eq!(
                a.display.hydrogen_position,
                wanted.display.hydrogen_position
            );
            assert_eq!(a.display.highlight, wanted.display.highlight);
            assert_eq!(a.display.number, wanted.display.number);
            assert_eq!(a.no_implicit, wanted.no_implicit);
            assert_eq!(a.isotope, wanted.isotope);
            assert_eq!(a.charge, wanted.charge);
        }
        assert_eq!(product.document.annotations, before.annotations);
        assert_eq!(product.document.groups, before.groups);
    }
    // The app must use the exact model-returned ink extents for placement,
    // including forced hydrogens, owned numbers, the manual caption and
    // highlights at the valid 100 pt / 40% / 0.1 pt style extremes.
    let mut offset = 0;
    let mut placed = Vec::new();
    for product in &products {
        let (lo, hi) = product.bounds;
        let anchor = generated.0.atoms[offset].position;
        let original = product.document.atoms[0].position;
        let shift = Point::new(anchor.x - original.x, anchor.y - original.y);
        for (j, a) in product.document.atoms.iter().enumerate() {
            let actual = &generated.0.atoms[offset + j];
            assert!((actual.position.x - a.position.x - shift.x).abs() < 0.001);
            assert!((actual.position.y - a.position.y - shift.y).abs() < 0.001);
            assert_eq!(actual.text_style, a.text_style);
            assert_eq!(actual.display, a.display);
            assert_eq!(actual.explicit_h, a.explicit_h);
            assert_eq!(actual.no_implicit, a.no_implicit);
        }
        placed.push((lo.offset(shift.x, shift.y), hi.offset(shift.x, shift.y)));
        offset += product.document.atoms.len();
    }
    for (i, (lo, hi)) in placed.iter().enumerate() {
        for (zl, zh) in placed.iter().skip(i + 1) {
            assert!(
                hi.x + 83.9 <= zl.x
                    || zh.x + 83.9 <= lo.x
                    || hi.y + 83.9 <= zl.y
                    || zh.y + 83.9 <= lo.y
            );
        }
    }
    assert_eq!(generated.0.annotations.len(), 3);
    assert_eq!(generated.0.groups.len(), 3);
    generated.0.validate().unwrap();
    let (mut app, _) = App::new();
    app.tab.doc = source.clone();
    let key = (app.tab.id, app.tab.file_epoch);
    app.generic.pending = Some(key);
    let _ = app.generic_action(Action::Expanded {
        source: key.0,
        epoch: key.1,
        result: Box::new(Ok(expand_drawing(source.clone()).unwrap())),
    });
    assert_eq!(app.tabs.background.first().unwrap().doc, source);
    let output = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.atoms.is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, output);
    assert_eq!(app.tabs.background.first().unwrap().doc, source);
}
#[test]
fn expansion_creates_new_tab_and_preserves_source_undo() {
    let (mut app, _) = App::new();
    let id = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let a = app.tab.doc.add_atom("*", Point::new(42., 0.));
    app.tab.doc.add_bond(id, a, 1, "plain");
    app.tab.doc =
        reshiki::atom_text::apply(&app.tab.doc, a, "[Cl,Br]", reshiki::atom_text::Mode::Auto)
            .unwrap();
    let before = app.tab.doc.clone();
    let key = (app.tab.id, app.tab.file_epoch);
    app.generic.pending = Some(key);
    let generated = expand_drawing(before.clone()).unwrap();
    let _ = app.generic_action(Action::Expanded {
        source: key.0,
        epoch: key.1,
        result: Box::new(Ok(generated)),
    });
    assert_eq!(app.tabs.background.len(), 1);
    assert_eq!(app.tabs.background.first().unwrap().doc, before);
    assert!(app.tab.doc.generic.is_empty());
    assert!(app.tab.history.can_undo());
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.atoms.is_empty());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.atoms.len(), 4);
}
fn union_publication_source() -> Document {
    let mut source = Document::default();
    for x in [0., 42., 84.] {
        source.add_atom("C", Point::new(x, 0.));
    }
    source.add_bond(1, 2, 1, "plain");
    source.add_bond(2, 3, 1, "plain");
    source =
        reshiki::atom_text::apply(&source, 2, "[CH2]1-3", reshiki::atom_text::Mode::Auto).unwrap();
    source.atom_mut(1).unwrap().map_num = 17;
    source.atom_mut(1).unwrap().display.mapping = reshiki::atom_labels::MappingDisplay {
        show: Some(true),
        offset: Some(Point::new(-80., -80.)),
        style: reshiki::typography::TextStyle {
            size_pt: 24.,
            italic: true,
            underline: true,
            family: "Times New Roman".into(),
            ..Default::default()
        },
    };
    let fixed = source.add_atom("O", Point::new(-240., 80.));
    source.atom_mut(fixed).unwrap().mark_serial = 9;
    source
        .atom_mut(fixed)
        .unwrap()
        .marks
        .push(reshiki::scientific::AtomMark {
            id: Some(7),
            kind: reshiki::scientific::MarkKind::LonePair,
            offset: Point::new(-15., -15.),
            angle: 0.,
            size_pt: Some(10.),
        });
    let caption = source.next_id();
    source.annotations.push(reshiki::document::Annotation {
        id: caption,
        position: Point::new(-330., 180.),
        text: "oxidane".into(),
        format: reshiki::typography::TextFormat {
            style: reshiki::typography::TextStyle {
                size_pt: 28.,
                italic: true,
                underline: true,
                ..Default::default()
            },
            width_pt: Some(60.),
            ..Default::default()
        },
    });
    source
        .molecule_names
        .push(reshiki::molecule_names::MoleculeName {
            annotation: caption,
            atoms: vec![fixed],
            smiles: "inherited-native-claim".into(),
            text: "oxidane".into(),
        });
    let target = reshiki::arrow_anchors::Target::LonePair {
        atom: fixed,
        mark: 7,
    };
    let start = target.center(&source).unwrap().offset(-100., 0.);
    source.arrows.push(reshiki::document::Arrow {
        id: source.next_id(),
        start,
        end: Point::new(-580., 80.),
        start_anchor: Some(reshiki::arrow_anchors::Anchor {
            target,
            offset: Point::new(-100., 0.),
            direction: Point::new(-1., 0.),
            gap: 4.,
        }),
        end_anchor: None,
        kind: "equilibrium".into(),
        control: None,
        cubic: Some([Point::new(-470., -160.), Point::new(-470., 280.)]),
        style: None,
    });
    source.validate().unwrap();
    source
}
fn union_ink_stays_in_grid(doc: &Document, (lo, hi): (Point, Point)) {
    let inside = |p: Point| {
        assert!(
            p.x >= lo.x - 0.02 && p.x <= hi.x + 0.02 && p.y >= lo.y - 0.02 && p.y <= hi.y + 0.02,
            "ink {p:?} outside {lo:?}..{hi:?}"
        )
    };
    for arrow in &doc.arrows {
        for path in arrow.paths() {
            let half = path.style.width() * 0.5;
            for p in path.commands.iter().flat_map(|command| command.points()) {
                inside(p.offset(-half, -half));
                inside(p.offset(half, half));
            }
        }
    }
    for a in &doc.annotations {
        for f in reshiki::typography::layout(&a.text, &a.format).fragments {
            let origin = a.position.offset(f.position.x, f.position.y);
            for (a, b) in reshiki::style::text_ink_boxes(&f.text, f.style.size(), &f.style) {
                inside(origin.offset(a.x, a.y));
                inside(origin.offset(b.x, b.y));
            }
        }
    }
    for indicator in reshiki::atom_labels::indicators(doc) {
        for (a, b) in
            reshiki::style::text_ink_boxes(&indicator.text, indicator.height, &indicator.style)
        {
            inside(indicator.origin.offset(a.x, a.y));
            inside(indicator.origin.offset(b.x, b.y));
        }
    }
}
#[test]
fn prepared_expansion_cloned_action_consumes_once_and_preserves_native_associations() {
    let source = union_publication_source();
    let prepared = expand_drawing(source.clone()).unwrap();
    let bounds = prepared.bounds();
    let (mut app, _) = App::new();
    app.tab.doc = source.clone();
    let key = (app.tab.id, app.tab.file_epoch);
    app.generic.pending = Some(key);
    let action = Action::Expanded {
        source: key.0,
        epoch: key.1,
        result: Box::new(Ok(prepared)),
    };
    let replay = action.clone();
    let _ = app.generic_action(action);
    assert!(!app.error);
    assert_eq!(app.tabs.background.len(), 1);
    assert_eq!(app.tabs.background.first().unwrap().doc, source);
    let output = app.tab.doc.clone();
    assert_eq!(output.molecule_names.len(), 3);
    assert_eq!(output.arrows.len(), 3);
    assert!(
        output
            .molecule_names
            .iter()
            .all(|n| n.smiles == "inherited-native-claim")
    );
    for name in &output.molecule_names {
        assert_eq!(
            output
                .annotations
                .iter()
                .find(|a| a.id == name.annotation)
                .unwrap()
                .text,
            name.text
        );
        assert!(
            name.atoms
                .iter()
                .all(|id| output.atom(*id).is_some_and(|a| a.element == "O"))
        );
    }
    assert_eq!(
        app.tab.camera.center,
        Point::new(
            (bounds.0.x + bounds.1.x) * 0.5,
            (bounds.0.y + bounds.1.y) * 0.5
        )
    );
    union_ink_stays_in_grid(&output, bounds);
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.object_ids().next().is_none());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, output);
    // Genuine label refresh changes only computed fields; it must not move
    // packed captions, anchors, explicit cubic controls or mapping styles.
    let refreshed =
        reshiki::atom_labels::refresh::Refresh::calculate(&app.tab.doc, &Default::default())
            .unwrap();
    refreshed.apply(&mut app.tab.doc);
    assert_eq!(app.tab.doc.annotations, output.annotations);
    assert_eq!(app.tab.doc.arrows, output.arrows);
    assert_eq!(app.tab.doc.molecule_names, output.molecule_names);
    assert!(
        app.tab
            .doc
            .atoms
            .iter()
            .zip(&output.atoms)
            .all(|(a, b)| a.display == b.display && a.position == b.position)
    );
    union_ink_stays_in_grid(&app.tab.doc, bounds);
    let reopened = Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap();
    let mut expected = app.tab.doc.current();
    reshiki::atom_labels::clear_computed(&mut expected);
    assert_eq!(reopened, expected);
    union_ink_stays_in_grid(&reopened, bounds);
    let published = app.tab.doc.clone();
    let undo = app.tab.history.can_undo();
    let redo = app.tab.history.can_redo();
    // Even if an otherwise valid stale host delivery is marked pending,
    // the shared capability rejects replay before creating another tab.
    app.generic.pending = Some(key);
    let _ = app.generic_action(replay);
    assert!(app.error);
    assert!(app.status.contains("already consumed"));
    assert_eq!(app.tabs.background.len(), 1);
    assert_eq!(app.tab.doc, published);
    assert_eq!(app.tab.history.can_undo(), undo);
    assert_eq!(app.tab.history.can_redo(), redo);
    assert_eq!(app.tabs.background.first().unwrap().doc, source);
}
#[test]
fn prepared_expansion_errors_keep_original_tab_history_and_status() {
    let (mut app, _) = App::new();
    app.tab.doc = union_publication_source();
    let caption = app.tab.doc.next_id();
    app.tab.doc.annotations.push(reshiki::document::Annotation {
        id: caption,
        position: Point::new(0., 200.),
        text: "x".repeat(3_000_000),
        format: Default::default(),
    });
    let before = app.tab.doc.clone();
    let id = app.tab.id;
    let _ = app.generic_action(Action::Expand);
    // Presentation + owned-input buffer reservation exhausts the default
    // allowance BEFORE the first snapshot, scheduling or pending marker.
    assert!(app.error);
    assert!(app.status.contains("work limit"));
    assert!(app.generic.pending.is_none());
    assert!(app.tabs.background.is_empty());
    assert_eq!(app.tab.id, id);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
    let key = (app.tab.id, app.tab.file_epoch);
    app.generic.pending = Some(key);
    let _ = app.generic_action(Action::Expanded {
        source: key.0,
        epoch: key.1,
        result: Box::new(Err("controlled expansion failure".into())),
    });
    assert!(app.error);
    assert_eq!(app.status, "controlled expansion failure");
    assert!(app.generic.pending.is_none());
    assert!(app.tabs.background.is_empty());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert!(!app.tab.history.can_redo());
    let (mut huge_app, _) = App::new();
    huge_app.tab.doc = union_publication_source();
    let id = huge_app.tab.doc.next_id();
    huge_app
        .tab
        .doc
        .annotations
        .push(reshiki::document::Annotation {
            id,
            position: Point::new(1e30, 80.),
            text: "remote manual text".into(),
            format: Default::default(),
        });
    let huge_before = huge_app.tab.doc.clone();
    huge_before.validate().unwrap();
    let failure = expand_drawing(huge_before.clone()).unwrap_err();
    let key = (huge_app.tab.id, huge_app.tab.file_epoch);
    huge_app.generic.pending = Some(key);
    let _ = huge_app.generic_action(Action::Expanded {
        source: key.0,
        epoch: key.1,
        result: Box::new(Err(failure.clone())),
    });
    assert!(huge_app.error);
    assert_eq!(huge_app.status, failure);
    assert!(huge_app.generic.pending.is_none());
    assert!(huge_app.tabs.background.is_empty());
    assert_eq!(huge_app.tab.doc, huge_before);
    assert!(!huge_app.tab.history.can_undo());
    assert!(!huge_app.tab.history.can_redo());
}

#[test]
fn generic_card_follows_generic_parts_and_repeat_section_one_selected_bracket() {
    use reshiki::graphics::{Graphic, GraphicKind};
    let (mut app, _) = App::new();
    let carbon = app.tab.doc.add_atom("C", Point::default());
    let mut graphic = |kind| {
        let id = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::dragged(
            id,
            kind,
            Point::new(-20., -20.),
            Point::new(20., 20.),
            Default::default(),
            Default::default(),
            false,
        ));
        id
    };
    let bracket = graphic(GraphicKind::Brackets);
    let rectangle = graphic(GraphicKind::Rectangle);
    assert!(app.generic_structure_section().is_none());
    for (selected, shown) in [
        (vec![bracket], true),
        (vec![rectangle], false),
        (vec![bracket, rectangle], false),
        (vec![], false),
    ] {
        app.tab.selected = selected;
        assert_eq!(
            app.repeating_unit_section().is_some(),
            shown,
            "{:?}",
            app.tab.selected
        );
    }
    let choice = app.tab.doc.add_atom("*", Point::new(42., 0.));
    app.tab.doc.add_bond(carbon, choice, 1, "plain");
    app.tab.doc = reshiki::atom_text::apply(
        &app.tab.doc,
        choice,
        "[Cl,Br]",
        reshiki::atom_text::Mode::Auto,
    )
    .unwrap();
    assert!(app.generic_structure_section().is_some());
}
