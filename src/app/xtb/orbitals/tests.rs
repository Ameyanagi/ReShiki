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
use reshiki_xtb_worker::protocol::Method;

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

fn worker_rows(rows: &[u32]) -> qcdata::Orbitals {
    let mut orbitals = worker_orbitals();
    let coefficients = rows
        .iter()
        .flat_map(|&index| {
            compute::row(&orbitals, qcdata::Spin::Alpha, index as usize)
                .unwrap()
                .1
                .iter()
                .copied()
        })
        .collect();
    orbitals.alpha.rows = rows.to_vec();
    orbitals.alpha.coefficients = coefficients;
    orbitals
}

/// A wider synthetic spectrum puts both range limits within reach.
fn wide_orbitals() -> qcdata::Orbitals {
    let mut orbitals = worker_orbitals();
    orbitals.basis = Some(BasisSet {
        shells: (0..20)
            .map(|i| Shell {
                atom: 0,
                l: 0,
                pure: false,
                exponents_per_bohr2: vec![0.5 + f64::from(i) * 0.1],
                coefficients: vec![1.],
            })
            .collect(),
    });
    orbitals.alpha.energies_hartree = (0..20).map(|i| f64::from(i) * 0.1 - 1.).collect();
    orbitals.alpha.occupations = (0..20).map(|i| if i < 10 { 2. } else { 0. }).collect();
    orbitals.alpha.rows = (0..20).collect();
    orbitals.alpha.coefficients = (0..400)
        .map(|i| if i / 20 == i % 20 { 1. } else { 0. })
        .collect();
    orbitals
}

/// Water with an energy result that keeps only levels, its Orbitals tab
/// open.
fn tab() -> App {
    tab_with_orbitals(levels())
}

fn tab_with_orbitals(orbitals: qcdata::Orbitals) -> App {
    let (mut app, _) = water(CalcTask::Energy);
    finish_energy(&mut app, Method::Gfn2, -5.070176, orbitals);
    let _ = app.update(Message::Xtb(XtbAction::Palette(
        super::super::palette::Action::View(View::Orbitals),
    )));
    app
}

fn finish_energy(app: &mut App, method: Method, energy: f64, orbitals: qcdata::Orbitals) {
    let mut computed = run::example_computed(energy, 14.01);
    computed.method = method;
    computed.orbitals = Some(orbitals);
    event(
        app,
        Event::Done(Ok(Arc::new(Report {
            engine: run::example_engine(),
            energy: computed.energy,
            computed,
            converged: true,
            positions: START.to_vec(),
            steps: 0,
            optimizers: vec![],
            energies: vec![energy],
            path: vec![],
            evaluations: vec![],
        }))),
    );
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
fn the_orbital_preview_follows_the_selected_molecule() {
    let mut app = tab();
    let before = app.tab.doc.clone();
    // Appending a copy gives the second molecule its own attachment but shares the result Arc.
    let copied = reshiki::editing::append(
        &mut app.tab.doc,
        &before,
        reshiki::document::Point::new(200., 0.),
    );
    app.changed(before);
    let [first, second] = app.tab.doc.science.results.as_slice() else {
        panic!("one result per molecule");
    };
    assert!(Arc::ptr_eq(&first.result, &second.result));
    assert_ne!(first.attachment, second.attachment);

    answer(&mut app);
    let preview = app.xtb_orbital_preview().expect("first molecule's preview");
    assert!(preview.models[0].surface.is_some());
    assert!(preview.models[1].surface.is_none());
    assert!(has_surface(preview));

    app.tab.selected = copied.clone();
    assert_eq!(app.xtb_shown_entry().map(|(index, _)| index), Some(1));
    assert!(app.xtb_orbital_preview().is_none());

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(copied)));
    let preview = app
        .xtb_orbital_preview()
        .expect("second molecule's preview");
    assert!(preview.models[0].surface.is_none());
    assert!(preview.models[1].surface.is_some());
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
fn a_failed_run_leaves_no_orbital_surface_behind() {
    let mut app = tab();
    computed(&mut app);
    assert!(app.display_document().models[0].surface.is_some());
    let _ = app.update(Message::Xtb(XtbAction::Run));
    event(
        &mut app,
        Event::Done(Err(super::super::state::Failure::Scf(
            "not converged".into(),
        ))),
    );
    assert!(app.xtb_failed().is_some());
    assert!(
        app.display_document().models[0].surface.is_none(),
        "the failure card hides the tabs, so no surface may stay on the canvas"
    );
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

#[test]
fn the_frontier_figure_places_homo_and_lumo_under_the_molecule() {
    let mut app = tab();
    answer(&mut app);
    let frames = app.tab.history.frames();
    act(&mut app, Action::PlaceFrontier);
    assert_eq!(app.tab.history.frames(), frames + 1, "one Undo step");
    let model = &app.tab.doc.models[0];
    let captions: Vec<_> = model.panels.iter().map(|p| p.caption.as_str()).collect();
    assert_eq!(captions, ["HOMO (−8.16 eV)", "LUMO (5.44 eV)"]);
    assert_eq!(
        model
            .panels
            .iter()
            .map(|p| p.figure.orbital.index)
            .collect::<Vec<_>>(),
        [3, 4]
    );
    // Both rows stay with the result, so the figure reopens unchanged.
    let stored = app.tab.doc.science.results[0]
        .result
        .orbitals
        .clone()
        .unwrap();
    assert_eq!(stored.alpha.rows, vec![3, 4]);
    // The captions are drawn, under the model's frame.
    let (_, hi) = reshiki::models3d::frame(&app.tab.doc, model).unwrap();
    let texts: Vec<_> = reshiki::scene::primitives(&app.tab.doc)
        .into_iter()
        .filter_map(|p| match p {
            Primitive::Text { text, position, .. } if text.ends_with("eV)") => Some(position),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 2);
    assert!(texts.iter().all(|p| p.y > hi.y));
    assert!(texts[0].x < texts[1].x, "side by side");
    let target = app.xtb_orbital_target().unwrap();
    assert!(target.frontier);
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.models[0].panels.is_empty());
}

#[test]
fn placing_an_mo_diagram_selects_it_with_both_frontier_rows_in_one_edit() {
    use reshiki::models3d::Style;

    let mut app = tab();
    computed(&mut app);
    act(&mut app, Action::Preset(0.07));
    act(&mut app, Action::Look(LookChoice::Mesh));
    app.tab.doc.models[0].style = Style::Wireframe;
    let before = app.tab.doc.clone();
    let atoms = before.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
    let (lo, hi) = reshiki::scene::selection_bounds(&before, &atoms).unwrap();
    let frames = app.tab.history.frames();
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    assert!(!app.error, "{}", app.status);
    let [graphic] = app.tab.doc.graphics.as_slice() else {
        panic!("one graphic")
    };
    assert_eq!(app.tab.selected, [graphic.id]);
    assert_eq!(
        graphic.origin.x,
        hi.x + before.drawing_style.bond_length_world
    );
    assert_eq!(graphic.origin.y, lo.y);
    let mo = graphic.mo.as_deref().unwrap();
    assert!(mo.linked);
    assert_eq!(mo.result, key(&app));
    assert_eq!(
        (graphic.axis_x.x, graphic.axis_y.y),
        mo.natural_size(&before.drawing_style)
    );
    assert_eq!(
        mo.pictures
            .iter()
            .map(|p| (p.column, p.figure.orbital.index))
            .collect::<Vec<_>>(),
        [(0, 3), (0, 4)]
    );
    assert!(
        mo.pictures
            .iter()
            .all(|p| p.figure.iso == 0.07 && p.figure.look.mesh)
    );
    assert_eq!(
        graphic
            .mo_picture_models(&app.tab.doc)
            .map(|model| model.style)
            .collect::<Vec<_>>(),
        [Style::BallStick; 2]
    );
    let stored = app.tab.doc.science.results[0]
        .result
        .orbitals
        .as_ref()
        .unwrap();
    assert_eq!(stored.alpha.rows, [3, 4]);
    assert_eq!(app.tab.history.frames(), frames + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn mo_diagram_placement_clears_the_model_panels_and_other_ink() {
    use reshiki::document::Point;
    use reshiki::graphics::{BracketSides, Graphic, GraphicKind};

    let mut app = tab();
    computed(&mut app);
    act(&mut app, Action::PlaceFrontier);
    let doc = &mut app.tab.doc;
    let atoms = doc.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
    let (lo, hi) = reshiki::scene::selection_bounds(doc, &atoms).unwrap();
    let gap = doc.drawing_style.bond_length_world;
    let start = Point::new(hi.x + gap, lo.y);
    let obstacle = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        obstacle,
        GraphicKind::Rectangle,
        start,
        Point::new(start.x + gap, hi.y + 2. * gap),
        Default::default(),
        BracketSides::Both,
        false,
    ));
    let (_, bottom) = reshiki::scene::selection_bounds(doc, &doc.all_ids()).unwrap();
    app.set_selection(vec![obstacle]);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: true,
        },
    );
    assert_eq!(app.status, "Run a calculation with orbitals first");
    assert_eq!(app.tab.doc.graphics.len(), 1);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    assert_eq!(
        app.tab.doc.graphics[1].origin,
        Point::new(lo.x, bottom.y + gap)
    );
}

#[test]
fn mo_diagram_picture_toggles_from_the_tab_and_canvas_without_editing_the_main_surface() {
    let mut app = tab();
    computed(&mut app);
    act(&mut app, Action::Place);
    let main = app.tab.doc.models[0].surface.clone();
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let id = app.tab.selected[0];
    let frames = app.tab.history.frames();
    act(&mut app, Action::ToggleDiagramPicture(id, 0, 3));
    assert_eq!(
        app.tab.doc.graphics[0].mo.as_ref().unwrap().pictures.len(),
        1
    );
    let _ = app.update(Message::Canvas(crate::canvas::Edit::MoLevelClick(id, 0, 2)));
    assert_eq!(
        app.tab.doc.graphics[0].mo.as_ref().unwrap().pictures.len(),
        2
    );
    assert_eq!(app.tab.doc.models[0].surface, main);
    let target = app.xtb_orbital_target().unwrap();
    assert_eq!(app.xtb_orbital_choice(&target).index, 2);
    assert_eq!(app.tab.xtb.palette.view, View::Orbitals);
    assert_eq!(app.tab.history.frames(), frames + 2);
    let _ = app.update(Message::Undo);
    assert_eq!(
        app.tab.doc.graphics[0].mo.as_ref().unwrap().pictures.len(),
        1
    );
}

#[test]
fn orbitals_controls_follow_the_selected_matching_mo_diagram() {
    let mut app = tab();
    computed(&mut app);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let before = app.tab.doc.clone();
    let first = before.graphics[0].clone();
    let mut second = first.clone();
    second.id = before.next_id();
    let id = second.id;
    app.tab.doc.graphics.push(second);
    app.changed(before);
    app.set_selection(vec![]);
    assert_eq!(app.xtb_orbital_target().unwrap().diagram, Some(first.id));
    app.set_selection(vec![id]);
    assert_eq!(app.xtb_orbital_target().unwrap().diagram, Some(id));

    let _ = app.update(Message::Canvas(crate::canvas::Edit::MoLevelClick(id, 0, 3)));
    let target = app.xtb_orbital_target().unwrap();
    assert_eq!(target.diagram, Some(id));
    assert_eq!(app.xtb_orbital_choice(&target).index, 3);
    assert_eq!(
        app.tab.doc.graphics[1].mo.as_ref().unwrap().pictures.len(),
        1
    );
    act(
        &mut app,
        Action::EditDiagram(target.diagram.unwrap(), DiagramEdit::Remove),
    );
    assert_eq!(app.tab.doc.graphics, [first]);
}

#[cfg(all(feature = "xtb", not(windows)))]
#[test]
fn place_mo_diagram_command_opens_matching_orbitals_and_requests_frontier_rows() {
    let mut app = tab();
    app.tab.xtb.palette.view = View::Summary;
    app.tab.xtb.palette.collapsed = true;
    app.tab.xtb.orbitals.computing = None;
    app.set_selection(vec![app.tab.doc.atoms[0].id]);
    let command = Action::PlaceDiagram {
        for_selection: true,
    };

    act(&mut app, command.clone());
    assert!(app.xtb_orbitals_visible());
    assert!(app.tab.xtb.orbitals.computing.is_some());
    assert!(app.tab.doc.graphics.is_empty());
    assert_eq!(app.status, "The orbitals are loading; try again");

    computed(&mut app);
    act(&mut app, command);
    assert_eq!(app.tab.doc.graphics.len(), 1);
    assert_eq!(app.tab.selected, [app.tab.doc.graphics[0].id]);
}

#[test]
fn mo_diagram_places_and_selects_beta_when_alpha_levels_are_missing() {
    let mut partial = worker_orbitals();
    let mut beta = partial.alpha.clone();
    beta.occupations = vec![1., 1., 1., 1., 0., 0.];
    partial.beta = Beta::Unrestricted(beta);
    partial.alpha.energies_hartree.clear();
    partial.alpha.occupations.clear();
    partial.alpha.rows.clear();
    partial.alpha.coefficients.clear();
    let (mut app, _) = water(CalcTask::Energy);
    finish_energy(&mut app, Method::Gfn2, -5.07, partial.clone());
    let key = key(&app);
    act(&mut app, Action::Computed(key, Ok(Arc::new(partial))));
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    assert!(!app.error, "{}", app.status);
    let graphic = &app.tab.doc.graphics[0];
    let id = graphic.id;
    let mo = graphic.mo.as_deref().unwrap();
    assert!(mo.columns[0].energies_ev.is_empty());
    assert_eq!(mo.columns[1].label, "β");
    assert_eq!(mo.pictures.len(), 2);
    assert!(mo.pictures.iter().all(|picture| picture.column == 1));

    let _ = app.update(Message::Canvas(crate::canvas::Edit::MoLevelClick(id, 1, 4)));
    let target = app.xtb_orbital_target().unwrap();
    let choice = app.xtb_orbital_choice(&target);
    assert_eq!((choice.column, choice.index), (0, 4));
    let mo = app.tab.doc.graphics[0].mo.as_deref().unwrap();
    assert_eq!(mo.pictures.len(), 1);
}

#[cfg(all(feature = "xtb", not(windows)))]
#[test]
fn an_empty_mo_diagram_waits_for_multiple_requested_rows_and_pins_each_on_arrival() {
    let mut app = tab();
    computed(&mut app);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let id = app.tab.selected[0];
    act(
        &mut app,
        Action::EditDiagram(id, DiagramEdit::RemovePictures),
    );
    app.tab.xtb.orbitals.computed.clear();
    let frames = app.tab.history.frames();
    act(&mut app, Action::ToggleDiagramPicture(id, 0, 1));
    act(&mut app, Action::ToggleDiagramPicture(id, 0, 2));
    act(&mut app, Action::ToggleDiagramPicture(id, 0, 0));
    act(&mut app, Action::ToggleDiagramPicture(id, 0, 0));
    assert_eq!(app.tab.history.frames(), frames);
    assert_eq!(app.tab.xtb.orbitals.pending_pictures.len(), 2);
    assert!(app.tab.xtb.orbitals.computing.is_some());
    let key = key(&app);
    act(
        &mut app,
        Action::Computed(key.clone(), Ok(Arc::new(worker_rows(&[1])))),
    );
    let mo = app.tab.doc.graphics[0].mo.as_ref().unwrap();
    assert_eq!(
        mo.pictures
            .iter()
            .map(|p| p.figure.orbital.index)
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(app.tab.history.frames(), frames + 1);
    assert_eq!(app.tab.xtb.orbitals.pending_pictures.len(), 1);
    assert_eq!(
        app.tab.xtb.orbitals.pending_pictures[0]
            .picture
            .figure
            .orbital
            .index,
        2
    );
    assert_eq!(
        app.tab
            .xtb
            .orbitals
            .computing
            .as_ref()
            .map(|c| c.key.as_str()),
        Some(key.as_str()),
        "a partial reply starts a follow-up request for the remaining picture"
    );
    act(
        &mut app,
        Action::Computed(key, Ok(Arc::new(worker_rows(&[2])))),
    );
    let mo = app.tab.doc.graphics[0].mo.as_ref().unwrap();
    assert_eq!(
        mo.pictures
            .iter()
            .map(|p| p.figure.orbital.index)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(app.tab.history.frames(), frames + 2);
    assert!(app.tab.xtb.orbitals.pending_pictures.is_empty());
    assert_eq!(
        app.tab.doc.science.results[0]
            .result
            .orbitals
            .as_ref()
            .unwrap()
            .alpha
            .rows,
        [1, 2, 3, 4]
    );
}

fn tab_with_mo_diagram() -> App {
    let mut app = tab();
    computed(&mut app);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    app
}

fn rerun_diagram(app: &mut App) -> qcdata::Orbitals {
    let atom = app.tab.doc.atoms[0].id;
    app.set_selection(vec![atom]);
    app.xtb.settings.method = qcdata::XtbMethod::GXtb;
    let _ = app.update(Message::Xtb(XtbAction::Run));
    let mut new_levels = levels();
    new_levels.alpha.energies_hartree[4] += 0.05;
    finish_energy(app, Method::Gxtb, -5.07, new_levels.clone());
    assert!(!app.error, "{}", app.status);
    new_levels
}

fn reply_with_rerun_rows(app: &mut App, levels: &qcdata::Orbitals, rows: &[u32]) {
    let key = science::result_key(&app.tab.doc.science.results.last().unwrap().result);
    let mut reply = worker_rows(rows);
    reply
        .alpha
        .energies_hartree
        .clone_from(&levels.alpha.energies_hartree);
    act(app, Action::Computed(key, Ok(Arc::new(reply))));
}

fn rerun_rows(doc: &Document) -> &[u32] {
    &doc.science
        .results
        .last()
        .unwrap()
        .result
        .orbitals
        .as_ref()
        .unwrap()
        .alpha
        .rows
}

#[test]
fn rerunning_relinks_and_pins_linked_pictures_in_one_undo_step() {
    let mut app = tab_with_mo_diagram();
    let before_copy = app.tab.doc.clone();
    let mut unlinked = before_copy.graphics[0].clone();
    unlinked.id = before_copy.next_id();
    unlinked.mo.as_mut().unwrap().linked = false;
    app.tab.doc.graphics.push(unlinked.clone());
    app.changed(before_copy);
    let before = app.tab.doc.clone();
    let frames = app.tab.history.frames();
    let old_key = key(&app);
    let new_levels = rerun_diagram(&mut app);
    let linked = app.tab.doc.graphics[0].mo.as_ref().unwrap();
    assert_ne!(linked.result, old_key);
    assert_eq!(
        linked.result,
        science::result_key(&app.tab.doc.science.results.last().unwrap().result)
    );
    assert_eq!(linked.columns, levels::columns(&new_levels));
    assert!(
        linked
            .pictures
            .iter()
            .all(|p| p.figure.orbital.result == linked.result && p.figure.snapshot.is_none())
    );
    assert_eq!(app.tab.doc.graphics[1], unlinked);
    assert_eq!(app.tab.history.frames(), frames + 1);
    assert!(rerun_rows(&app.tab.doc).is_empty());
    let status = app.status.clone();
    reply_with_rerun_rows(&mut app, &new_levels, &[0, 3]);
    assert_eq!(rerun_rows(&app.tab.doc), [3]);
    reply_with_rerun_rows(&mut app, &new_levels, &[1, 4]);
    assert_eq!(rerun_rows(&app.tab.doc), [3, 4]);
    assert_eq!(app.status, status);
    assert_eq!(app.tab.history.frames(), frames + 1);
    let after = app.tab.doc.clone();
    let reopened = Document::from_json(&after.file_json().unwrap()).unwrap();
    assert_eq!(rerun_rows(&reopened), [3, 4]);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}

#[test]
fn rerun_picture_rows_are_a_separate_undo_step_after_an_edit() {
    let mut app = tab_with_mo_diagram();
    let new_levels = rerun_diagram(&mut app);
    let relinked = app.tab.doc.clone();
    let id = relinked.graphics[0].id;
    act(&mut app, Action::EditDiagram(id, DiagramEdit::Below(3)));
    let edited = app.tab.doc.clone();
    let frames = app.tab.history.frames();
    let status = "Edited the MO diagram range";
    app.status = status.into();
    reply_with_rerun_rows(&mut app, &new_levels, &[3, 4]);
    assert_eq!(rerun_rows(&app.tab.doc), [3, 4]);
    assert_eq!(app.status, status);
    assert_eq!(app.tab.history.frames(), frames + 1);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, edited);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, relinked);
}

#[test]
fn rerun_picture_rows_are_skipped_for_unlinked_diagrams_and_stale_results() {
    for unlink in [true, false] {
        let mut app = tab_with_mo_diagram();
        let new_levels = rerun_diagram(&mut app);
        if unlink {
            let id = app.tab.doc.graphics[0].id;
            act(&mut app, Action::EditDiagram(id, DiagramEdit::Unlink));
        } else {
            let before = app.tab.doc.clone();
            app.tab.doc.atoms[0].charge = 1;
            app.changed(before);
            let doc = &app.tab.doc;
            let stored = doc.science.results.last().unwrap();
            let attachment = doc.science.attachment(stored.attachment).unwrap();
            assert_eq!(attachment.freshness(doc), science::Freshness::Stale);
        }
        let before = app.tab.doc.clone();
        let frames = app.tab.history.frames();
        reply_with_rerun_rows(&mut app, &new_levels, &[3, 4]);
        assert_eq!(app.tab.doc, before, "unlink = {unlink}");
        assert_eq!(app.tab.history.frames(), frames);
    }
}

#[test]
fn a_stale_rerun_keeps_the_linked_mo_diagram_and_its_snapshots() {
    let mut app = tab();
    computed(&mut app);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let small = app.tab.doc.graphics[0]
        .mo_picture_models(&app.tab.doc)
        .next()
        .unwrap();
    let view = surface::draw::view_signature(&app.tab.doc, &small);
    let shapes =
        surface::snapshot::encode(&Primitive::Surface { groups: vec![] }, Default::default())
            .unwrap();
    let before = app.tab.doc.clone();
    app.tab.doc.graphics[0].mo.as_mut().unwrap().pictures[0]
        .figure
        .snapshot = Some(surface::Snapshot {
        view,
        shapes,
        reach: 1.,
    });
    app.changed(before);
    let diagram = app.tab.doc.graphics[0].clone();
    let old_key = key(&app);
    let atom = app.tab.doc.atoms[0].id;
    app.set_selection(vec![atom]);
    app.xtb.settings.method = qcdata::XtbMethod::GXtb;
    let _ = app.update(Message::Xtb(XtbAction::Run));
    assert!(app.xtb_running());

    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(atom).unwrap().charge = 1;
    app.changed(before);
    let mut new_levels = levels();
    new_levels.alpha.energies_hartree[4] += 0.05;
    finish_energy(&mut app, Method::Gxtb, -5.07, new_levels);

    assert!(!app.error, "{}", app.status);
    let stored = app.tab.doc.science.results.last().unwrap();
    assert_ne!(science::result_key(&stored.result), old_key);
    assert_eq!(
        app.tab
            .doc
            .science
            .attachment(stored.attachment)
            .unwrap()
            .freshness(&app.tab.doc),
        science::Freshness::Stale,
        "the rerun result is retained as stale"
    );
    assert_eq!(app.tab.doc.graphics[0], diagram);
}

#[test]
fn adding_an_mo_picture_extends_either_range_and_resizes_in_one_undo_step() {
    use reshiki::document::Point;

    for (index, edit, range) in [
        (1, DiagramEdit::Below(8), (8, 2)),
        (18, DiagramEdit::Above(8), (2, 8)),
    ] {
        let orbitals = wide_orbitals();
        let mut app = tab_with_orbitals(orbitals.clone());
        let key = key(&app);
        act(&mut app, Action::Computed(key, Ok(Arc::new(orbitals))));
        act(
            &mut app,
            Action::PlaceDiagram {
                for_selection: false,
            },
        );
        let id = app.tab.selected[0];
        let before = app.tab.doc.clone();
        app.tab.doc.graphics[0].axis_x = Point::new(200., 80.);
        app.tab.doc.graphics[0].axis_y = Point::new(-50., 130.);
        app.changed(before);
        let before = app.tab.doc.clone();
        let frames = app.tab.history.frames();
        act(&mut app, Action::EditDiagram(id, edit));
        let stepped = app.tab.doc.graphics[0].clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);

        act(&mut app, Action::ToggleDiagramPicture(id, 0, index));
        assert!(!app.error, "{}", app.status);
        let g = &app.tab.doc.graphics[0];
        let mo = g.mo.as_ref().unwrap();
        assert_eq!((mo.below, mo.above), range);
        assert!(super::diagram::has_picture(mo, 0, index));
        assert_eq!(app.tab.history.frames(), frames + 1);
        assert_eq!(g.origin, stepped.origin);
        let (w, h) = mo.natural_size(&app.tab.doc.drawing_style);
        let (step_w, step_h) = stepped
            .mo
            .as_ref()
            .unwrap()
            .natural_size(&app.tab.doc.drawing_style);
        for (actual, expected) in [
            (g.axis_x.x / w, stepped.axis_x.x / step_w),
            (g.axis_x.y / w, stepped.axis_x.y / step_w),
            (g.axis_y.x / h, stepped.axis_y.x / step_h),
            (g.axis_y.y / h, stepped.axis_y.y / step_h),
        ] {
            assert!(
                (actual - expected).abs() < 1e-5,
                "picture addition keeps the same axis scale as the range stepper"
            );
        }
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.tab.doc, before,
            "one Undo removes the picture and range edit"
        );
    }
}

#[test]
fn an_mo_picture_beyond_either_range_limit_does_not_change_the_drawing() {
    let orbitals = wide_orbitals();
    let mut app = tab_with_orbitals(orbitals.clone());
    let key = key(&app);
    act(&mut app, Action::Computed(key, Ok(Arc::new(orbitals))));
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let id = app.tab.selected[0];
    let before = app.tab.doc.clone();
    let frames = app.tab.history.frames();
    for index in [0, 19] {
        act(&mut app, Action::ToggleDiagramPicture(id, 0, index));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.history.frames(), frames);
        assert!(app.tab.xtb.orbitals.pending_pictures.is_empty());
        assert_eq!(app.status, "This orbital is out of the diagram's range");
        assert!(app.status.chars().count() <= 60);
    }
}

#[test]
fn an_mo_picture_requires_its_spin_level_in_the_resolved_result() {
    for column in 0..=1 {
        let mut orbitals = worker_rows(&[3, 4]);
        orbitals.alpha.occupations = vec![1., 1., 1., 1., 0., 0.];
        orbitals.beta = Beta::Unrestricted(orbitals.alpha.clone());
        let mut app = tab_with_orbitals(orbitals.clone());
        let result_key = key(&app);
        act(
            &mut app,
            Action::Computed(result_key.clone(), Ok(Arc::new(orbitals))),
        );
        act(
            &mut app,
            Action::PlaceDiagram {
                for_selection: false,
            },
        );
        let id = app.tab.selected[0];
        // A native diagram can retain more levels than its referenced result.
        // Keep the opposite spin's level so that the gate must check this spin.
        let result = Arc::make_mut(&mut app.tab.doc.science.results[0].result);
        let orbitals = result.orbitals.as_mut().unwrap();
        let set = match (&mut orbitals.beta, column) {
            (Beta::Unrestricted(beta), 1) => beta,
            _ => &mut orbitals.alpha,
        };
        set.energies_hartree.pop();
        set.occupations.pop();
        assert_eq!(key(&app), result_key);
        assert!(
            app.tab.doc.graphics[0]
                .mo
                .as_ref()
                .unwrap()
                .range_for_picture(column, 5)
                .is_some()
        );
        let before = app.tab.doc.clone();
        let frames = app.tab.history.frames();

        act(&mut app, Action::ToggleDiagramPicture(id, column, 5));

        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.history.frames(), frames);
        assert!(app.tab.xtb.orbitals.pending_pictures.is_empty());
        assert!(app.tab.xtb.orbitals.computing.is_none());
        assert!(!app.error);
        assert_eq!(app.status, "This orbital is not in the result");
        assert!(app.status.chars().count() <= 60);
    }
}

#[cfg(all(feature = "xtb", not(windows)))]
#[test]
fn a_pending_mo_picture_retires_when_its_result_loses_the_requested_spin_level() {
    for column in 0..=1 {
        let mut orbitals = worker_rows(&[3, 4]);
        orbitals.alpha.occupations = vec![1., 1., 1., 1., 0., 0.];
        orbitals.beta = Beta::Unrestricted(orbitals.alpha.clone());
        let mut app = tab_with_orbitals(orbitals.clone());
        let result_key = key(&app);
        act(
            &mut app,
            Action::Computed(result_key.clone(), Ok(Arc::new(orbitals.clone()))),
        );
        act(
            &mut app,
            Action::PlaceDiagram {
                for_selection: false,
            },
        );
        let id = app.tab.selected[0];
        act(&mut app, Action::ToggleDiagramPicture(id, column, 5));
        assert_eq!(app.tab.xtb.orbitals.pending_pictures.len(), 1);
        assert!(app.tab.xtb.orbitals.computing.is_some());

        // The result identity and model geometry remain valid, but the resolved
        // spectrum no longer holds the row when its worker reply arrives.
        let result = Arc::make_mut(&mut app.tab.doc.science.results[0].result);
        for spectrum in [result.orbitals.as_mut().unwrap(), &mut orbitals] {
            let set = match (&mut spectrum.beta, column) {
                (Beta::Unrestricted(beta), 1) => beta,
                _ => &mut spectrum.alpha,
            };
            set.energies_hartree.pop();
            set.occupations.pop();
        }
        assert_eq!(key(&app), result_key);
        let before = app.tab.doc.clone();
        let frames = app.tab.history.frames();

        act(
            &mut app,
            Action::Computed(result_key, Ok(Arc::new(orbitals))),
        );

        assert_eq!(app.tab.doc, before, "the late row is not pinned or placed");
        assert_eq!(app.tab.history.frames(), frames);
        assert!(app.tab.xtb.orbitals.pending_pictures.is_empty());
        assert!(app.tab.xtb.orbitals.computing.is_none());
        assert!(!app.error);
        assert_eq!(app.status, "This orbital is not in the result");
        assert!(app.status.chars().count() <= 60);
    }
}

#[test]
fn an_mo_picture_requires_a_current_attachment_and_matching_model_geometry() {
    for stale_attachment in [false, true] {
        let mut app = tab();
        computed(&mut app);
        act(
            &mut app,
            Action::PlaceDiagram {
                for_selection: false,
            },
        );
        let id = app.tab.selected[0];
        let before = app.tab.doc.clone();
        if stale_attachment {
            app.tab.doc.atoms[0].charge = 1;
        } else {
            app.tab.doc.models[0].atoms[0].xyz[2] += 0.01;
        }
        app.changed(before);
        let small = app.tab.doc.graphics[0]
            .mo_picture_models(&app.tab.doc)
            .next()
            .unwrap();
        assert!(!surface::draw::figure_ready(&app.tab.doc, &small));
        app.tab.xtb.orbitals.computed.clear();
        let before = app.tab.doc.clone();
        let frames = app.tab.history.frames();
        act(&mut app, Action::ToggleDiagramPicture(id, 0, 1));
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.history.frames(), frames);
        assert!(app.tab.xtb.orbitals.pending_pictures.is_empty());
        assert_eq!(app.status, "The orbital no longer matches this molecule");
        assert!(app.status.chars().count() <= 60);
    }
}

#[test]
fn adding_an_mo_picture_uses_the_renderers_geometry_tolerance() {
    let mut app = tab();
    computed(&mut app);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let id = app.tab.selected[0];
    let before = app.tab.doc.clone();
    app.tab.doc.models[0].atoms[0].xyz[2] += 0.0005;
    app.changed(before);
    let small = app.tab.doc.graphics[0]
        .mo_picture_models(&app.tab.doc)
        .next()
        .unwrap();
    assert!(surface::draw::figure_ready(&app.tab.doc, &small));
    let frames = app.tab.history.frames();
    act(&mut app, Action::ToggleDiagramPicture(id, 0, 1));
    assert!(!app.error, "{}", app.status);
    assert!(super::diagram::has_picture(
        app.tab.doc.graphics[0].mo.as_ref().unwrap(),
        0,
        1
    ));
    assert_eq!(app.tab.history.frames(), frames + 1);
}

#[cfg(all(feature = "xtb", not(windows)))]
#[test]
fn a_pending_mo_picture_retires_when_its_source_geometry_changes() {
    let mut app = tab();
    computed(&mut app);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let id = app.tab.selected[0];
    app.tab.xtb.orbitals.computed.clear();
    act(&mut app, Action::ToggleDiagramPicture(id, 0, 1));
    assert_eq!(app.tab.xtb.orbitals.pending_pictures.len(), 1);
    assert!(app.tab.xtb.orbitals.computing.is_some());

    let before = app.tab.doc.clone();
    app.tab.doc.models[0].atoms[0].xyz[2] += 0.01;
    app.changed(before);
    let stored = &app.tab.doc.science.results[0];
    assert_eq!(
        app.tab
            .doc
            .science
            .attachment(stored.attachment)
            .unwrap()
            .freshness(&app.tab.doc),
        science::Freshness::Current,
        "the attachment still matches; only the accepted model geometry changed"
    );
    let small = app.tab.doc.graphics[0]
        .mo_picture_models(&app.tab.doc)
        .next()
        .unwrap();
    assert!(!surface::draw::figure_ready(&app.tab.doc, &small));
    let before = app.tab.doc.clone();
    let frames = app.tab.history.frames();
    computed(&mut app);
    assert_eq!(app.tab.doc, before, "the late row is not pinned or placed");
    assert_eq!(app.tab.history.frames(), frames);
    assert!(app.tab.xtb.orbitals.pending_pictures.is_empty());
    assert_eq!(app.status, "The orbital no longer matches this molecule");
    assert!(app.status.chars().count() <= 60);
}

#[test]
fn mo_diagram_range_and_unlink_are_undoable_and_keep_the_frame_scale() {
    let mut app = tab();
    computed(&mut app);
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let id = app.tab.selected[0];
    let before = app.tab.doc.clone();
    let frame = &before.graphics[0];
    let (w, h) = frame
        .mo
        .as_ref()
        .unwrap()
        .natural_size(&before.drawing_style);
    let frames = app.tab.history.frames();
    act(&mut app, Action::EditDiagram(id, DiagramEdit::Below(0)));
    act(&mut app, Action::EditDiagram(id, DiagramEdit::Above(8)));
    act(&mut app, Action::EditDiagram(id, DiagramEdit::Unlink));
    let g = &app.tab.doc.graphics[0];
    let mo = g.mo.as_ref().unwrap();
    assert_eq!((mo.below, mo.above, mo.linked), (0, 8, false));
    let (new_w, new_h) = mo.natural_size(&before.drawing_style);
    assert_eq!(g.origin, frame.origin);
    assert_eq!(
        (g.axis_x.x / new_w, g.axis_y.y / new_h),
        (frame.axis_x.x / w, frame.axis_y.y / h)
    );
    assert_eq!(app.tab.history.frames(), frames + 3);
    for _ in 0..3 {
        let _ = app.update(Message::Undo);
    }
    assert_eq!(app.tab.doc, before);
}

#[test]
fn native_clipboard_paste_remaps_the_molecule_and_delete_removes_the_mo_diagram() {
    for cross_theme in [false, true] {
        native_mo_paste_and_delete(cross_theme);
    }
}

fn native_mo_paste_and_delete(cross_theme: bool) {
    let mut app = tab();
    computed(&mut app);
    act(&mut app, Action::Preset(0.061_234_5));
    act(
        &mut app,
        Action::PlaceDiagram {
            for_selection: false,
        },
    );
    let g = &app.tab.doc.graphics[0];
    let small = g.mo_picture_models(&app.tab.doc).next().unwrap();
    let view = surface::draw::view_signature(&app.tab.doc, &small);
    let shapes =
        surface::snapshot::encode(&Primitive::Surface { groups: vec![] }, Default::default())
            .unwrap();
    app.tab.doc.graphics[0].mo.as_mut().unwrap().pictures[0]
        .figure
        .snapshot = Some(surface::Snapshot {
        view,
        shapes,
        reach: 1.,
    });
    let doc = &app.tab.doc;
    let part = reshiki::clipboard::selection_or_drawing(doc, &doc.all_ids());
    let part = Document::from_json(&part.file_json().unwrap()).unwrap();
    let original = doc.graphics[0].mo.as_ref().unwrap();
    let original_atom = original.molecule;
    let pictures = original.pictures.clone();
    if cross_theme {
        app.tab.doc.canvas_theme = app.tab.doc.canvas_theme.toggled();
    }
    app.clipboard_read(
        app.tab.file_epoch,
        app.tab.revision,
        Ok(reshiki::clipboard::PasteOutcome::native(part)),
    );
    assert!(!app.error, "{}", app.status);
    let pasted = &app.tab.doc.graphics[1];
    let id = pasted.id;
    let mo = pasted.mo.as_ref().unwrap();
    assert_eq!(mo.pictures[1], pictures[1]);
    assert_eq!(mo.pictures[0].figure.orbital, pictures[0].figure.orbital);
    let saved = mo.pictures[0].figure.snapshot.as_ref().unwrap();
    let source = pictures[0].figure.snapshot.as_ref().unwrap();
    assert_eq!(saved.shapes, source.shapes);
    let small = pasted.mo_picture_models(&app.tab.doc).next().unwrap();
    assert_eq!(
        saved.view == surface::draw::view_signature(&app.tab.doc, &small),
        !cross_theme,
        "cross_theme = {cross_theme}"
    );
    assert_ne!(mo.molecule, original_atom);
    assert!(app.tab.selected.contains(&mo.molecule.unwrap()));
    assert!(app.tab.selected.contains(&id));
    app.set_selection(vec![id]);
    let _ = app.update(Message::Delete);
    assert_eq!(app.tab.doc.graphics.len(), 1);
    assert_eq!(app.tab.doc.models.len(), 2);
}
