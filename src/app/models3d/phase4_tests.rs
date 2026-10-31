use super::*;

fn drag_selected_model(app: &App) -> Edit {
    use crate::canvas::{Camera, MoleculeCanvas, PreviewStyles, State};
    use iced::{Event, Vector, mouse, widget::canvas::Program};
    let center = models3d::frame_origin(&app.tab.doc, &app.tab.doc.models[0]).unwrap();
    let camera = Camera { center, zoom: 1. };
    let bounds = iced::Rectangle::with_size(iced::Size::new(800., 600.));
    let canvas = MoleculeCanvas::preview(
        &app.tab.doc,
        &app.tab.selected,
        camera,
        PreviewStyles {
            bond_drawing: app.tab.bond_drawing,
            chain_drawing: app.tab.chain_drawing,
            arrow_preset: app.tab.arrow_style,
            arrow_style: &app.tab.arrows.style,
            orbital_phase: app.tab.orbital_phase,
            graphic_style: &app.tab.graphic_style,
            bracket_sides: app.tab.bracket_sides,
        },
    );
    let from = camera.screen(center, bounds);
    let mut state = State::default();
    [
        mouse::Event::CursorMoved { position: from },
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::CursorMoved {
            position: from + Vector::new(20., -8.),
        },
        mouse::Event::ButtonReleased(mouse::Button::Left),
    ]
    .into_iter()
    .filter_map(|event| {
        canvas
            .update(
                &mut state,
                &Event::Mouse(event),
                bounds,
                mouse::Cursor::Unavailable,
            )
            .and_then(|action| action.into_inner().0)
    })
    .last()
    .expect("a model turn from the canvas")
}

#[test]
fn chemistry_marks_the_arranged_model_stale_and_undo_clears_it() {
    for edit in ["element", "charge", "bond"] {
        let (mut app, atoms) = with_model(Display::Both);
        let before = app.tab.doc.clone();
        app.tab.doc.atom_mut(atoms[0]).unwrap().position.x -= 20.;
        app.changed(before);
        set(&mut app, Setting::Style(Style::Wireframe));
        set(&mut app, Setting::Labels(Labels::Heteroatoms));
        assert!(!app.tab.doc.models[0].is_stale(&app.tab.doc));

        let original = app.tab.doc.clone();
        let count = frames(&app);
        match edit {
            "element" => app.tab.doc.atom_mut(atoms[2]).unwrap().element = "N".into(),
            "charge" => app.tab.doc.atom_mut(atoms[2]).unwrap().charge = -1,
            "bond" => app.tab.doc.bonds[1].order = 2,
            _ => unreachable!(),
        }
        app.changed(original.clone());
        assert!(!app.error, "{edit}: {}", app.status);
        assert_eq!(app.tab.doc.models, original.models, "{edit}");
        assert!(app.tab.doc.models[0].is_stale(&app.tab.doc), "{edit}");
        assert!(app.stale_model().is_some());
        assert_eq!(frames(&app), count + 1);
        let edited = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
        assert!(!app.tab.doc.models[0].is_stale(&app.tab.doc));
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, edited);
        assert!(app.tab.doc.models[0].is_stale(&app.tab.doc));
    }
}

#[test]
fn updating_replaces_only_the_conformer_and_is_one_undo_step() {
    let (mut app, atoms) = with_model(Display::Both);
    let _ = app.update(Message::Models3d(Action::Flatten));
    show(&mut app, Display::Both);
    assert!(app.tab.doc.models[0].drawn.is_some());
    let before = app.tab.doc.clone();
    let model = &mut app.tab.doc.models[0];
    model.rotation = models3d::turn(model.rotation, 35., -18., true);
    model.offset = Point::new(90., -40.);
    model.size *= 1.4;
    model.style = Style::SpaceFilling;
    model.look = Look::Glossy;
    model.show_hydrogens = Hydrogens::Polar;
    model.labels = Labels::All;
    model.depth_fade = 0.65;
    model.perspective = 0.25;
    model.copy_opacity = 0.3;
    model.background = Background::Card;
    model.layout = Layout::View;
    model.force_field = models3d::ForceField::Uff;
    app.changed(before);
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(atoms[2]).unwrap().element = "N".into();
    app.changed(before);
    assert!(!app.error, "{}", app.status);
    let original = app.tab.doc.clone();
    let old = &original.models[0];
    let count = frames(&app);
    let _ = app.update(Message::Models3d(Action::Update));
    assert_eq!(app.tab.doc, original);
    assert_eq!(frames(&app), count);
    let job = app.tab.model_job.as_ref().unwrap();
    assert_eq!(job.target, Some(0));
    assert_eq!(job.force_field, ForceField::Uff);
    assert_eq!(job.seed, old.seed);
    let (key, mut result) = generated(&app);
    Arc::make_mut(&mut result).conformer.positions[1].z += 0.4;
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(!app.error, "{}", app.status);
    assert_eq!(frames(&app), count + 1);
    assert_eq!(app.tab.doc.atoms, original.atoms);
    assert_eq!(app.tab.doc.bonds, original.bonds);
    let model = &app.tab.doc.models[0];
    assert!(!model.is_stale(&app.tab.doc));
    assert_ne!(model.atoms, old.atoms);
    assert_ne!(model.fingerprint, old.fingerprint);
    assert_eq!(model.force_field, models3d::ForceField::Uff);
    assert_eq!(model.seed, old.seed);
    assert_eq!(model.copy_opacity, old.copy_opacity);
    assert_eq!(
        (
            model.display,
            model.rotation,
            model.offset,
            model.size,
            model.style,
            model.look,
            model.show_hydrogens,
            model.labels,
            model.depth_fade,
            model.perspective,
            model.background,
            model.layout,
        ),
        (
            old.display,
            old.rotation,
            old.offset,
            old.size,
            old.style,
            old.look,
            old.show_hydrogens,
            old.labels,
            old.depth_fade,
            old.perspective,
            old.background,
            old.layout,
        )
    );
    assert_eq!(model.drawn, old.drawn);
    let updated = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(app.tab.doc.models[0].is_stale(&app.tab.doc));
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, updated);
}

#[test]
fn regeneration_uses_a_fresh_seed_and_commits_one_undo_step() {
    let (mut app, _) = with_model(Display::ThreeD);
    let original = app.tab.doc.clone();
    let count = frames(&app);
    let _ = app.update(Message::Models3d(Action::Update));
    assert!(
        app.tab.model_job.is_none(),
        "An unchanged model needs no update"
    );
    let _ = app.update(Message::Models3d(Action::Regenerate));
    let seed = app.tab.model_job.as_ref().unwrap().seed;
    assert_eq!(
        seed,
        ((i64::from(original.models[0].seed) + 1_000_003) % i64::from(GENERATION_SEED_MAX)) as i32
    );
    assert_ne!(seed, original.models[0].seed);
    assert_eq!(app.tab.doc, original);
    assert_eq!(frames(&app), count);
    let (key, mut result) = generated(&app);
    Arc::make_mut(&mut result).conformer.positions[1].y += 0.3;
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(!app.error, "{}", app.status);
    assert_eq!(frames(&app), count + 1);
    assert_eq!(app.tab.doc.models[0].seed, seed);
    assert_ne!(app.tab.doc.models[0].atoms, original.models[0].atoms);
    assert!(!app.tab.doc.models[0].is_stale(&app.tab.doc));
    let regenerated = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, regenerated);
}

#[test]
fn regeneration_wraps_the_seed_inside_the_ensemble_range() {
    let (mut app, _) = with_model(Display::ThreeD);
    app.tab.doc.models[0].seed = GENERATION_SEED_MAX - 2;
    let original = app.tab.doc.clone();
    let count = frames(&app);
    let _ = app.update(Message::Models3d(Action::Regenerate));
    let seed = app.tab.model_job.as_ref().unwrap().seed;
    assert_eq!(seed, 1_000_001);
    assert!((1..GENERATION_SEED_MAX).contains(&seed));
    assert_eq!(app.tab.doc, original);
    assert_eq!(frames(&app), count);
}

#[test]
fn update_and_regenerate_align_rotated_heavy_atoms_and_hydrogens_to_the_arranged_view() {
    for fresh in [false, true] {
        let (mut app, atoms) = with_model(Display::Both);
        let before = app.tab.doc.clone();
        app.tab.doc.models[0].rotation =
            models3d::turn(app.tab.doc.models[0].rotation, 47., -26., true);
        if !fresh {
            app.tab.doc.atom_mut(atoms[2]).unwrap().element = "N".into();
        }
        app.changed(before);
        let original = app.tab.doc.clone();
        let old = &original.models[0];
        let _ = app.update(Message::Models3d(if fresh {
            Action::Regenerate
        } else {
            Action::Update
        }));
        let (key, mut result) = generated(&app);
        // A rigidly equivalent worker result in a different global frame.
        for point in &mut Arc::make_mut(&mut result).conformer.positions {
            let (x, y, z) = (point.x, point.y, point.z);
            (point.x, point.y, point.z) = (-z + 5., x - 3., -y + 7.);
        }
        let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
        assert!(!app.error, "{}", app.status);
        let model = &app.tab.doc.models[0];
        assert_eq!(model.rotation, old.rotation);
        for (now, before) in model
            .atoms
            .iter()
            .map(|a| a.xyz)
            .chain(model.hydrogens.iter().map(|h| h.xyz))
            .zip(
                old.atoms
                    .iter()
                    .map(|a| a.xyz)
                    .chain(old.hydrogens.iter().map(|h| h.xyz)),
            )
        {
            assert!(
                now.into_iter()
                    .zip(before)
                    .all(|(a, b)| (a - b).abs() < 1e-5),
                "{now:?} != {before:?}"
            );
        }
        assert_eq!(app.tab.doc.atoms, original.atoms);
    }
}

#[test]
fn alignment_finds_the_full_rank_least_squares_fit_without_scaling_the_conformer() {
    let mut doc = Document::default();
    let points = [
        [0., 0., 0.],
        [2., 0.3, 0.1],
        [-0.4, 1.5, 0.2],
        [0.2, -0.4, 1.3],
    ];
    let atoms: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(i, _)| doc.add_atom("C", Point::new(i as f32 * 36., 0.)))
        .collect();
    for pair in atoms.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    let old = models3d::Model3d::new(
        &doc,
        atoms
            .iter()
            .zip(points)
            .map(|(&id, xyz)| models3d::Placed { id, xyz })
            .collect(),
        vec![models3d::Hydrogen {
            parent: atoms[0],
            xyz: [0.1, -0.7, 0.3],
        }],
        models3d::ForceField::Mmff94s,
    );
    let centre: [f32; 3] = std::array::from_fn(|i| points.iter().map(|p| p[i]).sum::<f32>() / 4.);
    // Uniform scale noise has a known least-squares rigid fit: recover the
    // original frame and centre, while preserving the worker's changed size.
    let expected = |xyz: [f32; 3]| std::array::from_fn(|i| centre[i] + 1.07 * (xyz[i] - centre[i]));
    let place = |xyz: [f32; 3]| {
        let [x, y, z] = expected(xyz);
        let (s, c) = 0.73_f32.sin_cos();
        let (x, y) = (c * x - s * y, s * x + c * y);
        let (s, c) = (-0.51_f32).sin_cos();
        [x + 5., c * y - s * z - 3., s * y + c * z + 7.]
    };
    let mut model = old.clone();
    for atom in &mut model.atoms {
        atom.xyz = place(atom.xyz);
    }
    for hydrogen in &mut model.hydrogens {
        hydrogen.xyz = place(hydrogen.xyz);
    }
    align_conformer(&mut model, &old, &atoms);
    for (now, old) in model
        .atoms
        .iter()
        .map(|atom| atom.xyz)
        .chain(model.hydrogens.iter().map(|hydrogen| hydrogen.xyz))
        .zip(
            old.atoms
                .iter()
                .map(|atom| atom.xyz)
                .chain(old.hydrogens.iter().map(|hydrogen| hydrogen.xyz)),
        )
    {
        let expected = expected(old);
        assert!(
            now.into_iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-5),
            "{now:?} != {expected:?}"
        );
    }
}

#[test]
fn alignment_handles_one_point_and_parallel_or_opposite_two_point_sources() {
    let (app, atoms) = with_model(Display::ThreeD);
    for kind in ["one", "quarter", "half"] {
        let mut old = app.tab.doc.models[0].clone();
        for (i, atom) in old.atoms.iter_mut().enumerate() {
            atom.xyz = [i as f32 * 2., 0., 0.];
        }
        old.hydrogens[0].xyz = [0., 1., 0.];
        let mut model = old.clone();
        let place = |[x, y, z]: [f32; 3]| match kind {
            "one" => [x + 5., y - 3., z + 7.],
            "quarter" => [-y + 5., x - 3., z + 7.],
            _ => [-x + 5., y - 3., -z + 7.],
        };
        for atom in &mut model.atoms {
            atom.xyz = place(atom.xyz);
        }
        for hydrogen in &mut model.hydrogens {
            hydrogen.xyz = place(hydrogen.xyz);
        }
        let n = if kind == "one" { 1 } else { 2 };
        align_conformer(&mut model, &old, &atoms[..n]);
        for (now, before) in model.atoms.iter().zip(&old.atoms).take(n) {
            assert!(
                now.xyz
                    .into_iter()
                    .zip(before.xyz)
                    .all(|(a, b)| (a - b).abs() < 1e-5)
            );
        }
        let hydrogen = model.hydrogens[0].xyz;
        assert!(hydrogen.into_iter().all(f32::is_finite));
        let bond: f32 = hydrogen
            .into_iter()
            .zip(model.atoms[0].xyz)
            .map(|(a, b)| (a - b).powi(2))
            .sum();
        assert!(
            (bond - 1.).abs() < 1e-5,
            "A rigid fit preserves the hydrogen bond"
        );
        if kind != "half" {
            assert!(
                hydrogen
                    .into_iter()
                    .zip(old.hydrogens[0].xyz)
                    .all(|(a, b)| (a - b).abs() < 1e-5)
            );
        }
    }
}

#[test]
fn stale_topology_gates_view_layout_without_changing_the_drawing_or_history() {
    for edit in ["add", "delete"] {
        let (mut app, atoms) = with_model(Display::Both);
        let before = app.tab.doc.clone();
        if edit == "add" {
            let added = app.tab.doc.add_atom("C", Point::new(110., -21.));
            app.tab.doc.add_bond(atoms[2], added, 1, "plain");
        } else {
            app.tab.doc.delete(&[atoms[2]]);
        }
        app.changed(before);
        let original = app.tab.doc.clone();
        let count = frames(&app);
        for action in [Action::Flatten, Action::Layout(Layout::View)] {
            let _ = app.update(Message::Models3d(action));
            assert_eq!(app.status, "Update the model first", "{edit}");
            assert!(!app.error);
            assert_eq!(app.tab.doc, original);
            assert_eq!(frames(&app), count);
        }
        set(&mut app, Setting::Style(Style::Wireframe));
        assert_eq!(app.tab.doc.models[0].style, Style::Wireframe);
    }
}

#[test]
fn stale_checks_are_reused_until_the_document_revision_or_identity_changes() {
    let (mut app, atoms) = with_model(Display::Both);
    let start = app.tab.model_stale.borrow().comparisons;
    for _ in 0..3 {
        assert!(app.stale_model().is_none());
        let _ = app.structure_section();
        let _ = app.model_section();
        let _ = app.with_model_notice(Space::new().into());
    }
    assert_eq!(app.tab.model_stale.borrow().comparisons, start + 1);
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(atoms[2]).unwrap().element = "N".into();
    app.changed(before);
    assert!(app.stale_model().is_some());
    let mut count = app.tab.model_stale.borrow().comparisons;
    for identity in ["epoch", "tab"] {
        if identity == "epoch" {
            app.tab.file_epoch += 1;
        } else {
            app.tab.id.0 += 1;
        }
        assert!(app.stale_model().is_some());
        count += 1;
        assert_eq!(app.tab.model_stale.borrow().comparisons, count);
    }
}

#[test]
fn cancelled_failed_modal_and_outdated_updates_keep_the_previous_model() {
    for outcome in ["cancel", "failure", "modal", "outdated"] {
        let (mut app, atoms) = with_model(Display::Both);
        let before = app.tab.doc.clone();
        app.tab.doc.atom_mut(atoms[2]).unwrap().element = "N".into();
        app.changed(before);
        let _ = app.update(Message::Models3d(Action::Update));
        let (key, result) = generated(&app);
        match outcome {
            "cancel" => {
                let _ = app.update(Message::Escape);
                assert!(app.tab.model_job.is_none());
            }
            "modal" => {
                let _ = app.update(Message::AtomText(
                    super::super::super::atom_text::Action::Begin(Some(atoms[0])),
                ));
                assert!(app.tab.atom_text.is_some());
            }
            "outdated" => {
                let before = app.tab.doc.clone();
                app.tab.doc.atom_mut(atoms[0]).unwrap().position.x += 8.;
                app.changed(before);
            }
            _ => {}
        }
        let original = app.tab.doc.clone();
        let count = frames(&app);
        let response = if outcome == "failure" {
            Err("No conformer".into())
        } else {
            Ok(result)
        };
        let _ = app.update(Message::Models3d(Action::Generated(key, response)));
        assert_eq!(app.tab.doc, original, "{outcome}: {}", app.status);
        assert_eq!(frames(&app), count, "{outcome}");
        assert!(app.tab.model_job.is_none(), "{outcome}");
        assert!(app.tab.doc.models[0].is_stale(&app.tab.doc), "{outcome}");
    }
}

#[test]
fn replacement_results_belong_to_the_requested_model_and_latest_job() {
    let (mut app, _) = with_model(Display::Both);
    let water = app.tab.doc.atoms[3].id;
    let original = app.tab.doc.clone();
    let count = frames(&app);
    let _ = app.update(Message::Models3d(Action::Regenerate));
    let (old_key, old_result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Regenerate));
    let (key, mut result) = generated(&app);
    assert_ne!(key, old_key);
    let _ = app.update(Message::Models3d(Action::Generated(
        old_key,
        Ok(old_result),
    )));
    assert_eq!(app.tab.doc, original);
    assert!(app.tab.model_job.is_some());
    app.tab.selected = vec![water];
    Arc::make_mut(&mut result).conformer.positions[1].z += 0.25;
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(!app.error, "{}", app.status);
    assert_eq!(frames(&app), count + 1);
    assert_eq!(app.tab.selected, [water]);
    assert_eq!(app.tab.doc.models.len(), 1);
    assert_ne!(app.tab.doc.models[0].atoms, original.models[0].atoms);
    assert_eq!(app.tab.doc.atom(water), original.atom(water));
}

#[test]
fn added_and_deleted_atoms_stay_linked_until_explicit_update() {
    let (mut app, atoms) = with_model(Display::Both);
    let before = app.tab.doc.clone();
    let added = app.tab.doc.add_atom("C", Point::new(108., -21.));
    app.tab.doc.add_bond(atoms[2], added, 1, "plain");
    app.changed(before.clone());
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.models[0].atoms, before.models[0].atoms);
    assert!(app.tab.doc.models[0].is_stale(&app.tab.doc));
    app.tab.selected = vec![added];
    let (linked, index) = app.selected_model().unwrap();
    assert_eq!(index, Some(0));
    assert_eq!(linked, [atoms.clone(), vec![added]].concat());
    let _ = app.update(Message::Models3d(Action::Update));
    assert_eq!(app.tab.model_job.as_ref().unwrap().prepared.ids(), linked);
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.models[0].atoms.len(), 4);
    assert!(!app.tab.doc.models[0].is_stale(&app.tab.doc));

    let before = app.tab.doc.clone();
    app.tab.doc.delete(&[atoms[0]]);
    app.changed(before.clone());
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.models[0].atoms, before.models[0].atoms);
    assert!(app.tab.doc.models[0].is_stale(&app.tab.doc));
    let (linked, index) = app.selected_model().unwrap();
    assert_eq!(index, Some(0));
    assert_eq!(linked, [atoms[1], atoms[2], added]);
    let _ = app.update(Message::Models3d(Action::Update));
    assert_eq!(app.tab.model_job.as_ref().unwrap().prepared.ids(), linked);
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.models[0].atoms.len(), 3);
    assert!(!app.tab.doc.models[0].is_stale(&app.tab.doc));
    assert!(
        !app.tab.doc.models[0]
            .linked_ids(&app.tab.doc)
            .contains(&atoms[0])
    );
}

#[test]
fn a_split_source_retains_all_links_and_refuses_a_partial_update() {
    let (mut app, atoms) = with_model(Display::ThreeD);
    let before = app.tab.doc.clone();
    app.tab
        .doc
        .bonds
        .retain(|bond| !(bond.a == atoms[1] && bond.b == atoms[2]));
    app.changed(before.clone());
    assert!(!app.error, "{}", app.status);
    assert_eq!(app.tab.doc.models[0].atoms, before.models[0].atoms);
    assert_eq!(app.tab.doc.models[0].linked_ids(&app.tab.doc), atoms);
    assert!(app.tab.doc.models[0].is_stale(&app.tab.doc));
    let split = app.tab.doc.clone();
    let count = frames(&app);
    for selected in [atoms[0], atoms[2]] {
        app.tab.selected = vec![selected];
        assert_eq!(app.selected_model(), Some((atoms.clone(), Some(0))));
        for action in [Action::Update, Action::Regenerate] {
            let _ = app.update(Message::Models3d(action));
            assert!(app.tab.model_job.is_none());
            assert_eq!(app.tab.doc, split);
            assert_eq!(frames(&app), count);
            assert_eq!(
                app.status,
                "3D model not generated: the source must be one connected structure"
            );
        }
    }
}

#[test]
fn updating_a_joined_fragment_refuses_to_drop_another_models_disconnected_source() {
    let (mut app, atoms) = with_model(Display::Both);
    let water = app.tab.doc.atoms[3].id;
    app.tab.selected = vec![water];
    show(&mut app, Display::Both);
    let (key, result) = generated(&app);
    let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
    let before = app.tab.doc.clone();
    app.tab
        .doc
        .bonds
        .retain(|bond| !(bond.a == atoms[1] && bond.b == atoms[2]));
    app.tab.doc.add_bond(atoms[2], water, 1, "plain");
    app.changed(before);
    let split = app.tab.doc.clone();
    let count = frames(&app);
    for action in [Action::Update, Action::Regenerate] {
        let _ = app.update(Message::Models3d(action));
        assert!(app.tab.model_job.is_none());
        assert_eq!(app.tab.doc, split);
        assert_eq!(frames(&app), count);
        assert_eq!(
            app.status,
            "3D model not generated: the source must be one connected structure"
        );
    }
}

#[test]
fn updating_either_joined_view_keeps_unrelated_models_in_order() {
    for target in [1, 2] {
        let (mut app, atoms) = with_model(Display::Both);
        let part = reshiki::editing::selection(&app.tab.doc, &atoms);
        let water = app.tab.doc.atoms[3].id;
        app.tab.selected = vec![water];
        show(&mut app, Display::Both);
        let (key, result) = generated(&app);
        let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
        let before = app.tab.doc.clone();
        for offset in [Point::new(400., 0.), Point::new(800., 0.)] {
            reshiki::editing::append(&mut app.tab.doc, &part, offset);
        }
        app.tab.doc.models[..3].rotate_right(1);
        app.tab.doc.add_bond(atoms[2], water, 1, "plain");
        app.changed(before);
        app.tab.selected = app.tab.doc.models[target].linked_ids(&app.tab.doc);
        let joined = app.tab.doc.clone();
        let _ = app.update(Message::Models3d(Action::Update));
        let (key, result) = generated(&app);
        let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.models.len(), 3);
        assert_eq!(app.tab.doc.models[0], joined.models[0]);
        assert_eq!(app.tab.doc.models[2], joined.models[3]);
        assert_eq!(
            app.tab.doc.models[1].linked_ids(&app.tab.doc),
            [atoms, vec![water]].concat()
        );
    }
}

#[test]
fn updating_a_joined_model_replaces_both_views_and_keeps_both_original_drawings() {
    for target in [0, 1] {
        let (mut app, atoms) = with_model(Display::Both);
        let original_drawing = app.tab.doc.atoms.clone();
        let _ = app.update(Message::Models3d(Action::Flatten));
        show(&mut app, Display::Both);
        let water = app.tab.doc.atoms[3].id;
        app.tab.selected = vec![water];
        show(&mut app, Display::ThreeD);
        let (key, result) = generated(&app);
        let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
        set(&mut app, Setting::Style(Style::Wireframe));
        set(&mut app, Setting::Look(Look::Glossy));
        set(&mut app, Setting::Labels(Labels::All));
        app.edit(Edit::ModelView {
            atoms: vec![water],
            rotation: models3d::turn(app.tab.doc.models[1].rotation, 45., -15., true),
        });
        let _ = app.update(Message::Models3d(Action::Flatten));
        show(&mut app, Display::Both);
        assert!(app.tab.doc.models.iter().all(|model| model.drawn.is_some()));
        let before = app.tab.doc.clone();
        app.tab.doc.add_bond(atoms[2], water, 1, "plain");
        app.changed(before.clone());
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.models, before.models);
        assert!(
            app.tab
                .doc
                .models
                .iter()
                .all(|model| model.is_stale(&app.tab.doc))
        );
        assert_eq!(app.tab.doc.models[0].linked_ids(&app.tab.doc), atoms);
        assert_eq!(app.tab.doc.models[1].linked_ids(&app.tab.doc), [water]);
        let selected = if target == 0 {
            atoms.clone()
        } else {
            vec![water]
        };
        app.tab.selected = selected.clone();
        assert_eq!(app.selected_model(), Some((selected, Some(target))));
        let merged = app.tab.doc.clone();
        let chosen = &merged.models[target];
        let origin = models3d::frame_origin(&merged, chosen).unwrap();
        let count = frames(&app);
        let _ = app.update(Message::Models3d(Action::Update));
        assert_eq!(app.tab.model_job.as_ref().unwrap().target, Some(target));
        assert_eq!(
            app.tab.model_job.as_ref().unwrap().prepared.ids(),
            [atoms.clone(), vec![water]].concat()
        );
        let (key, result) = generated(&app);
        let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
        assert!(!app.error, "{}", app.status);
        assert_eq!(frames(&app), count + 1);
        assert_eq!(app.tab.doc.models.len(), 1);
        let model = &app.tab.doc.models[0];
        assert_eq!(model.atoms.len(), 4);
        assert!(!model.is_stale(&app.tab.doc));
        assert_eq!(
            (
                model.display,
                model.rotation,
                model.size,
                model.style,
                model.look,
                model.labels
            ),
            (
                chosen.display,
                chosen.rotation,
                chosen.size,
                chosen.style,
                chosen.look,
                chosen.labels
            )
        );
        let current = models3d::frame_origin(&app.tab.doc, model).unwrap();
        assert!(
            (current.x - origin.x).abs() < 1e-4 && (current.y - origin.y).abs() < 1e-4,
            "{current:?} != {origin:?}"
        );
        assert_eq!(app.tab.doc.atoms, merged.atoms);
        assert_eq!(app.tab.doc.bonds, merged.bonds);
        let drawn = model.drawn.as_ref().unwrap();
        for source in &merged.models {
            let saved = source.drawn.as_ref().unwrap();
            for atom in &saved.atoms {
                assert!(drawn.atoms.contains(atom));
            }
            for bond in &saved.bonds {
                assert!(drawn.bonds.contains(bond));
            }
            for scope in &saved.depth_appearance {
                assert!(drawn.depth_appearance.contains(scope));
            }
        }
        let linked = [atoms.clone(), vec![water]].concat();
        for id in &linked {
            assert_eq!(models3d::model_of(&app.tab.doc, *id), Some(model));
        }
        // Selecting the remaining frame must resolve the same model for its
        // inspector, display controls and turn gesture.
        app.tab.selected = models3d::frame_hit(&app.tab.doc, current).unwrap();
        assert_eq!(app.selected_model(), Some((linked.clone(), Some(0))));
        assert!(app.model_section().is_some());
        set(&mut app, Setting::Labels(Labels::Heteroatoms));
        assert_eq!(app.tab.doc.models[0].labels, Labels::Heteroatoms);
        let _ = app.update(Message::Undo);
        let turn = drag_selected_model(&app);
        let Edit::ModelView {
            atoms: turned,
            rotation,
        } = &turn
        else {
            panic!("Expected the joined model's turn, got {turn:?}");
        };
        assert_eq!(turned, &linked);
        let rotation = *rotation;
        assert_ne!(rotation, app.tab.doc.models[0].rotation);
        app.edit(turn);
        assert_eq!(app.tab.doc.models[0].rotation, rotation);
        let _ = app.update(Message::Undo);
        let updated = app.tab.doc.clone();
        let _ = app.update(Message::Models3d(Action::Layout(Layout::Original)));
        assert!(!app.error, "{}", app.status);
        for atom in &original_drawing {
            assert!(
                app.tab
                    .doc
                    .atom(atom.id)
                    .unwrap()
                    .position
                    .distance(atom.position)
                    < 1e-4
            );
        }
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, updated);
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, merged);
        assert_eq!(app.tab.doc.models.len(), 2);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, updated);
    }
}

#[test]
fn file_derived_models_with_cell_sites_or_marker_never_generate_but_accept_labels() {
    for kind in ["cell", "sites", "marker"] {
        let (mut app, atoms) = with_model(Display::ThreeD);
        let mut model = app.tab.doc.models.remove(0);
        let lattice = (kind == "cell").then_some(models3d::Cell {
            vectors: [[8., 0., 0.], [0., 8., 0.], [0., 0., 8.]],
            crossing: vec![],
        });
        let sites = if kind == "sites" {
            vec![models3d::Site {
                id: atoms[0],
                label: "C1".into(),
                adp: None,
                asymmetric: true,
            }]
        } else {
            vec![]
        };
        model = model.with_cell(&app.tab.doc, lattice, sites);
        model.seed = 0;
        app.tab.doc.models.push(model);
        if kind == "cell" {
            for bond in &mut app.tab.doc.bonds {
                bond.distance_derived = true;
            }
        }
        let original = app.tab.doc.clone();
        let count = frames(&app);
        for action in [Action::Update, Action::Regenerate] {
            let _ = app.update(Message::Models3d(action));
            assert!(app.tab.model_job.is_none());
            assert_eq!(app.tab.doc, original);
            assert_eq!(frames(&app), count);
        }
        assert!(!app.tab.doc.models[0].is_stale(&app.tab.doc));
        set(&mut app, Setting::Labels(Labels::All));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.tab.doc.models[0].labels, Labels::All);
        assert_eq!(frames(&app), count + 1);
        assert!(app.tab.model_job.is_none());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original);
    }
}

#[test]
fn updating_a_flattened_model_after_atom_edits_keeps_the_original_layout() {
    for edit in ["add", "delete"] {
        let (mut app, atoms) = with_model(Display::Both);
        let original = app.tab.doc.clone();
        let _ = app.update(Message::Models3d(Action::Flatten));
        show(&mut app, Display::Both);
        assert!(app.tab.doc.models[0].drawn.is_some());
        let before = app.tab.doc.clone();
        let mut added = None;
        if edit == "add" {
            let id = app.tab.doc.add_atom("C", Point::new(120., -25.));
            app.tab.doc.add_bond(atoms[2], id, 1, "plain");
            added = Some(id);
        } else {
            app.tab.doc.delete(&[atoms[0]]);
        }
        app.changed(before);
        assert!(!app.error, "{edit}: {}", app.status);
        app.tab.selected = vec![atoms[1]];
        let edited = app.tab.doc.clone();
        let count = frames(&app);
        let _ = app.update(Message::Models3d(Action::Update));
        let (key, result) = generated(&app);
        let _ = app.update(Message::Models3d(Action::Generated(key, Ok(result))));
        assert!(!app.error, "{edit}: {}", app.status);
        assert_eq!(frames(&app), count + 1, "{edit}");
        assert!(!app.tab.doc.models[0].is_stale(&app.tab.doc), "{edit}");
        assert!(app.tab.doc.models[0].drawn.is_some());
        let updated = app.tab.doc.clone();
        let _ = app.update(Message::Models3d(Action::Layout(Layout::Original)));
        assert!(!app.error, "{edit}: {}", app.status);
        for atom in &original.atoms {
            if let Some(restored) = app.tab.doc.atom(atom.id) {
                assert!(
                    restored.position.distance(atom.position) < 1e-4,
                    "{edit}: atom {}",
                    atom.id
                );
            }
        }
        if let Some(id) = added {
            assert_eq!(
                app.tab.doc.atom(id).unwrap().position,
                edited.atom(id).unwrap().position
            );
        }
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, updated, "{edit}");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, edited, "{edit}");
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer phase-4 linked model controls"]
async fn phase4_controls_activate_only_for_the_appropriate_models() -> Result<(), String> {
    use iced::advanced::{renderer::Headless, widget::Operation, widget::operation};
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .ok_or("No renderer")?;
    for kind in ["fresh", "stale", "file", "marker"] {
        let (mut app, atoms) = with_model(Display::ThreeD);
        if kind == "stale" {
            let before = app.tab.doc.clone();
            app.tab.doc.atom_mut(atoms[2]).unwrap().element = "N".into();
            app.changed(before);
        } else if matches!(kind, "file" | "marker") {
            let sites = if kind == "file" {
                vec![models3d::Site {
                    id: atoms[0],
                    label: "C1".into(),
                    adp: None,
                    asymmetric: true,
                }]
            } else {
                vec![]
            };
            let model = app
                .tab
                .doc
                .models
                .remove(0)
                .with_cell(&app.tab.doc, None, sites);
            app.tab.doc.models.push(model);
        }
        for (section, id, expected) in [
            (
                "model",
                "model-update",
                (kind == "stale").then_some("update"),
            ),
            (
                "structure",
                "structure-update-model",
                (kind == "stale").then_some("update"),
            ),
            (
                "floating",
                "model-floating-update",
                (kind == "stale").then_some("update"),
            ),
            (
                "model",
                "model-regenerate",
                (!matches!(kind, "file" | "marker")).then_some("regenerate"),
            ),
            (
                "structure",
                "structure-layout-view",
                (kind != "stale").then_some("view"),
            ),
            ("model", "model-labels-all", Some("labels")),
        ] {
            let content = match section {
                "model" => app.model_section().ok_or("No 3D model section")?,
                "structure" => app.structure_section().ok_or("No Structure section")?,
                "floating" => app.with_model_notice(Space::new().into()),
                _ => unreachable!(),
            };
            let mut ui = iced_runtime::UserInterface::build(
                content,
                iced::Size::new(900., 1000.),
                iced_runtime::user_interface::Cache::new(),
                &mut renderer,
            );
            let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
            ui.operate(&renderer, &mut operation::black_box(&mut activate));
            let outcome = activate.finish();
            let activated = match outcome {
                operation::Outcome::Some(Message::Models3d(Action::Update)) => Some("update"),
                operation::Outcome::Some(Message::Models3d(Action::Regenerate)) => {
                    Some("regenerate")
                }
                operation::Outcome::Some(Message::Models3d(Action::Layout(Layout::View))) => {
                    Some("view")
                }
                operation::Outcome::Some(Message::Models3d(Action::Set(Setting::Labels(
                    Labels::All,
                )))) => Some("labels"),
                operation::Outcome::Some(_) => {
                    return Err(format!("{kind}: {id} sent an unexpected message"));
                }
                _ => None,
            };
            assert_eq!(activated, expected, "{kind}: {id}");
        }
    }
    Ok(())
}
