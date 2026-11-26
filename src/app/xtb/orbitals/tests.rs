//! The Orbitals tab on a water energy result, with the worker's orbitals
//! and the meshes written by hand.
use super::super::tests::{START, event, water};
use super::super::{
    Action as XtbAction, CalcTask,
    run::{self, Event, Report},
};
use super::*;
use reshiki::qcdata::{BasisSet, Beta, OrbitalSet, Shell};
use reshiki::scene::Primitive;

fn act(app: &mut App, action: Action) {
    let _ = app.update(send(action));
}

/// Water's levels: four doubly occupied, two empty.
fn levels() -> qcdata::Orbitals {
    serde_json::from_value(serde_json::json!({
        "core_electrons": [2, 0, 0],
        "alpha": {
            "energies_hartree": [-0.7, -0.5, -0.4, -0.3, 0.2, 0.4],
            "occupations": [2., 2., 2., 2., 0., 0.]
        },
        "beta": "restricted"
    }))
    .expect("orbital levels")
}

/// A minimal basis on water (O: s, p; H: s) and all six orbitals, as the
/// worker would send them.
fn worker_orbitals() -> qcdata::Orbitals {
    let s = |atom, exponent| Shell {
        atom,
        l: 0,
        pure: false,
        exponents_per_bohr2: vec![exponent],
        coefficients: vec![1.],
    };
    let basis = BasisSet {
        shells: vec![
            s(0, 1.2),
            Shell {
                atom: 0,
                l: 1,
                pure: false,
                exponents_per_bohr2: vec![0.9],
                coefficients: vec![1.],
            },
            s(1, 0.6),
            s(2, 0.6),
        ],
    };
    // Six orbitals over six AOs (s, px, py, pz on O; s on each H).
    let mut coefficients = vec![0.; 36];
    for (row, ao) in [(0, 0), (1, 4), (2, 1), (3, 3), (4, 2), (5, 5)] {
        coefficients[row * 6 + ao] = 1.;
    }
    // The LUMO: antibonding over both hydrogens.
    coefficients[4 * 6 + 4] = 0.7;
    coefficients[4 * 6 + 5] = -0.7;
    let levels = levels();
    qcdata::Orbitals {
        basis: Some(basis),
        alpha: OrbitalSet {
            rows: (0..6).collect(),
            coefficients,
            ..levels.alpha
        },
        ..levels
    }
}

/// Water with an energy result that keeps only levels, its Orbitals tab
/// open.
fn tab() -> App {
    let (mut app, _) = water(CalcTask::Energy);
    let mut computed = run::example_computed(-5.070176, 14.01);
    computed.orbitals = Some(levels());
    event(
        &mut app,
        Event::Done(Ok(Arc::new(Report {
            engine: run::example_engine(),
            energy: computed.energy,
            computed,
            converged: true,
            positions: START.to_vec(),
            steps: 0,
            energies: vec![-5.070176],
            path: vec![],
            evaluations: vec![],
        }))),
    );
    let _ = app.update(Message::Xtb(XtbAction::Palette(
        super::super::palette::Action::View(View::Orbitals),
    )));
    app
}

fn key(app: &App) -> String {
    science::result_key(&app.tab.doc.science.results[0].result)
}

/// The worker's answer, as a build with the engine gets it.
fn computed(app: &mut App) {
    let key = key(app);
    act(app, Action::Computed(key, Ok(Arc::new(worker_orbitals()))));
}

/// The worker's answer and the mesh of every figure on the canvas, built
/// here on the test thread.
fn answer(app: &mut App) {
    computed(app);
    let doc = app.display_document().clone();
    for model in &doc.models {
        let Some(figure) = &model.surface else {
            continue;
        };
        let orbitals = worker_orbitals();
        let (basis, row) = compute::row(
            &orbitals,
            figure.orbital.spin,
            figure.orbital.index as usize,
        )
        .unwrap();
        let points: Vec<[f32; 3]> = model
            .points(&doc)
            .unwrap()
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        let meshes = compute::meshes(basis, row, &points, figure.iso);
        let mesh_key = surface::draw::key(&doc, model).unwrap();
        act(app, Action::Built(mesh_key, meshes));
    }
}

fn has_surface(doc: &Document) -> bool {
    reshiki::scene::primitives(doc)
        .iter()
        .any(|p| matches!(p, Primitive::Surface { .. }))
}

#[test]
fn the_tab_previews_the_homo_without_changing_the_drawing() {
    let mut app = tab();
    computed(&mut app);
    let target = app.xtb_orbital_target().expect("an orbital target");
    assert_eq!(target.problem, None);
    let choice = app.xtb_orbital_choice(&target);
    assert_eq!((choice.column, choice.index), (0, 3), "the HOMO first");
    let preview = app.display_document();
    let figure = preview.models[0]
        .surface
        .as_ref()
        .expect("a previewed figure");
    assert_eq!(figure.orbital.index, 3);
    assert!(
        app.tab.doc.models[0].surface.is_none(),
        "a preview is no edit"
    );
    let frames = app.tab.history.frames();

    answer(&mut app);
    assert!(has_surface(app.display_document()), "the mesh is drawn");
    assert!(!has_surface(&app.tab.doc));

    // ↑ (with nothing selected) goes up the diagram, to the LUMO; HOMO
    // comes back.
    app.tab.selected.clear();
    assert!(app.xtb_orbitals_visible(), "visible");
    assert!(
        !app.xtb_orbital_keys(),
        "arrows belong to the drawing first"
    );
    act(&mut app, Action::Homo);
    assert!(
        app.xtb_orbital_keys(),
        "after a choice in the tab they choose orbitals"
    );
    let _ = app.update(Message::Shortcut(crate::app::shortcuts::Action::Nudge(
        0., -1.,
    )));
    let target = app.xtb_orbital_target().unwrap();
    assert_eq!(app.xtb_orbital_choice(&target).index, 4);
    assert_eq!(app.status, "LUMO · 5.44 eV");
    act(&mut app, Action::Homo);
    let target = app.xtb_orbital_target().unwrap();
    assert_eq!(app.xtb_orbital_choice(&target).index, 3);
    assert_eq!(
        app.tab.history.frames(),
        frames,
        "browsing adds no Undo step"
    );

    // Leaving the tab ends the preview.
    let _ = app.update(Message::Xtb(XtbAction::Palette(
        super::super::palette::Action::View(View::Summary),
    )));
    assert!(app.display_document().models[0].surface.is_none());
}

#[test]
fn place_keeps_the_orbital_in_the_drawing_as_one_undo_step() {
    let mut app = tab();
    let target = app.xtb_orbital_target().unwrap();
    let choice = app.xtb_orbital_choice(&target);
    assert!(
        !app.xtb_orbital_ready(&target, &choice),
        "Place waits for the coefficients"
    );
    answer(&mut app);
    let before = key(&app);
    let frames = app.tab.history.frames();
    act(&mut app, Action::Place);
    assert_eq!(
        app.tab.history.frames(),
        frames + 1,
        "Place is one Undo step"
    );
    let figure = app.tab.doc.models[0].surface.clone().expect("placed");
    assert_eq!(figure.orbital.index, 3);
    assert_eq!(figure.iso, ISO);
    // The stored result keeps that orbital's row, and nothing else.
    let stored = app.tab.doc.science.results[0]
        .result
        .orbitals
        .clone()
        .unwrap();
    assert_eq!(stored.alpha.rows, vec![3]);
    assert!(stored.basis.is_some());
    assert_eq!(key(&app), before, "pinning keeps the figure's result key");
    assert!(has_surface(&app.tab.doc));

    // A slider drag changes the figure at once and is one Undo step; a
    // preset is another.
    act(&mut app, Action::Iso(0.08));
    act(&mut app, Action::Iso(0.09));
    assert_eq!(app.tab.doc.models[0].surface.as_ref().unwrap().iso, 0.09);
    act(&mut app, Action::Settle);
    // Presets are steps of their own, even one after another.
    act(&mut app, Action::Preset(0.02));
    act(&mut app, Action::Preset(0.10));
    assert_eq!(app.tab.doc.models[0].surface.as_ref().unwrap().iso, 0.10);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].surface.as_ref().unwrap().iso, 0.02);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].surface.as_ref().unwrap().iso, 0.09);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc.models[0].surface.as_ref().unwrap().iso, ISO);

    act(&mut app, Action::Look(LookChoice::Mesh));
    let look = app.tab.doc.models[0].surface.as_ref().unwrap().look;
    assert_eq!((look.fill, look.mesh), (Fill::None, true));
    let _ = app.update(Message::Undo);

    // Undo removes the placed orbital and its row.
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.models[0].surface.is_none());
    let stored = app.tab.doc.science.results[0]
        .result
        .orbitals
        .clone()
        .unwrap();
    assert!(stored.alpha.rows.is_empty());
}

#[test]
fn a_changed_molecule_shows_no_orbitals() {
    let mut app = tab();
    let o = app.tab.doc.atoms[0].id;
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(o).unwrap().charge = 1;
    app.changed(before);
    let target = app.xtb_orbital_target().unwrap();
    assert!(
        target
            .problem
            .as_deref()
            .is_some_and(|p| p.contains("Recalculate to show orbitals")),
        "{:?}",
        target.problem
    );
    let _ = app.update(Message::Xtb(XtbAction::Palette(
        super::super::palette::Action::View(View::Orbitals),
    )));
    assert!(app.display_document().models[0].surface.is_none());
}

#[test]
fn levels_follow_the_spin_columns_of_an_open_shell() {
    let open = qcdata::Orbitals {
        beta: Beta::SameOrbitals {
            occupations: vec![1., 1., 1., 0., 0., 0.],
        },
        alpha: OrbitalSet {
            occupations: vec![1., 1., 1., 1., 0., 0.],
            ..levels().alpha
        },
        ..levels()
    };
    let columns = levels::columns(&open);
    assert_eq!(columns[0].homo(), Some(3));
    assert_eq!(columns[1].homo(), Some(2));
}

// Waiting needs a build that can recompute xTB orbitals. Builds without the
// engine (Windows) leave it out rather than ignore it: the real-renderer job
// runs ignored tests.
#[cfg(all(feature = "xtb", not(windows)))]
#[test]
fn a_reopened_placed_orbital_waits_for_its_new_level() {
    let mut app = tab();
    answer(&mut app);
    act(&mut app, Action::Place);
    // As after reopening: only the placed row is at hand.
    app.tab.xtb.orbitals.computed.clear();
    act(&mut app, Action::Lumo);
    assert_eq!(
        app.tab.doc.models[0]
            .surface
            .as_ref()
            .unwrap()
            .orbital
            .index,
        3
    );
    assert!(app.tab.xtb.orbitals.pending.is_some());
    let frames = app.tab.history.frames();
    answer(&mut app);
    let figure = app.tab.doc.models[0].surface.clone().unwrap();
    assert_eq!(figure.orbital.index, 4, "the waiting level is placed");
    assert_eq!(app.tab.history.frames(), frames + 1);
    let stored = app.tab.doc.science.results[0]
        .result
        .orbitals
        .clone()
        .unwrap();
    assert_eq!(stored.alpha.rows, vec![3, 4]);
}
