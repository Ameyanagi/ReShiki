//! Exercise the normal app flow with fabricated worker events.
use super::*;
use crate::app::xtb::{palette, results, run, tests::model};
use reshiki::document::Document;
use reshiki::geometry::Point3;
use reshiki::qcdata::{Endpoints, Status, Trajectory, Vibrations};
use reshiki::science::Launch;

fn send(app: &mut App, action: XtbAction) {
    let _ = app.update(Message::Xtb(action));
}

fn reaction_app(doc: Document, arrow: u64) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = doc;
    app.tab.selected = vec![arrow];
    app.tab.busy = false;
    app
}

fn start() -> (App, u64) {
    let (doc, arrow) = card::tests::claisen();
    let mut app = reaction_app(doc, arrow);
    send(&mut app, XtbAction::Ts(Action::Card(card::Action::Find)));
    let plan = ts_plan::plan(&app.tab.doc, arrow).unwrap();
    assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, plan.start_atoms);
    assert!(app.tab.model_job.is_some());
    (app, arrow)
}

fn finish_model(app: &mut App) {
    let order = app.model_atoms();
    let mut positions: Vec<_> = (0..order.len())
        .map(|i| Point3 {
            x: i as f64 * 1.4,
            y: 0.,
            z: 0.,
        })
        .collect();
    let mut parents = vec![];
    for (index, id) in order.iter().enumerate() {
        let hydrogens = match app.tab.doc.atom(*id).unwrap().map_num {
            1 | 4 | 6 => 2,
            2 | 5 => 1,
            3 => 0,
            map => panic!("unexpected Claisen map number {map}"),
        };
        for hydrogen in 0..hydrogens {
            positions.push(Point3 {
                x: positions[index].x,
                y: if hydrogen == 0 { 1. } else { -1. },
                z: 0.,
            });
            parents.push(index);
        }
    }
    assert_eq!(
        parents.len(),
        8,
        "the Claisen model includes every hydrogen"
    );
    let _ = app.finish_model(positions, parents);
    assert!(
        app.tab.xtb.run.as_ref().unwrap().launch.is_some(),
        "{}",
        app.status
    );
    let run = app.tab.xtb.run.as_ref().unwrap();
    let launch = run.launch.as_ref().unwrap();
    assert_eq!(
        launch.frame.numbers.len(),
        run.ts.as_ref().unwrap().request.plan.atoms
    );
    assert_eq!(launch.frame.numbers.iter().filter(|&&z| z == 1).count(), 8);
}

fn example_report(launch: &Launch) -> Arc<job::Report> {
    let reactant = (*launch.frame).clone();
    let mut transition = reactant.clone();
    transition.positions_angstrom[0][2] += 0.5;
    let mut product = reactant.clone();
    product.positions_angstrom[0][2] += 1.;
    let mut result = run::example_computed(-4.9, 0.9)
        .to_calc_result(&transition, CalcTask::Energy, &run::example_engine(), None)
        .unwrap();
    result.task = CalcTask::TransitionState;
    result.status = Status::Converged;
    result.input_frame = Some(launch.frame.hash());
    result.endpoints = Some(Endpoints {
        reactant: reactant.hash(),
        product: product.hash(),
    });
    result.vibrations = Some(Vibrations {
        frequencies_cm1: vec![-387.],
        ir_intensities_km_mol: vec![12.],
        modes: vec![],
        reduced_masses_amu: vec![],
        masses_amu: vec![],
    });
    result.trajectory = Some(Trajectory {
        numbers: reactant.numbers.clone(),
        isotopes: reactant.isotopes.clone(),
        masses_amu: reactant.masses_amu.clone(),
        charge: reactant.charge,
        multiplicity: reactant.multiplicity,
        positions_angstrom: vec![],
        energies_hartree: vec![-5., -4.9, -5.02],
        labels: vec![],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    });
    let extras = Extras {
        image_positions: [&reactant, &transition, &product]
            .map(|frame| frame.positions_angstrom.clone())
            .to_vec(),
        connects: true,
        climbing: 1,
        engine_imaginary: Some(-387.),
        worker_frequencies: vec![-387.],
        irc_energies: [vec![], vec![]],
        forming: vec![],
        breaking: vec![],
    };
    Arc::new(job::Report {
        frames: vec![transition, reactant, product],
        result,
        extras,
        info: Default::default(),
    })
}

#[test]
fn search_progress_and_finish_keep_the_drawing_and_open_a_previewable_path() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let report = example_report(run.launch.as_ref().unwrap());
    let reactant = model(&app.tab.doc);
    let selected = report.frames[0].positions_angstrom[0].map(|x| x as f32);
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Stage(Stage::Neb)),
    );
    send(
        &mut app,
        XtbAction::TsEvent(
            serial,
            Event::Neb {
                iteration: 120,
                max_force: 0.012,
                climbing: Some(1),
            },
        ),
    );
    assert!(
        app.status
            .contains("✓ Endpoints · ✓ IDPP · NEB 120/300 · TS refine · IRC")
    );
    assert!(
        app.status.contains(
            "Climbing-image NEB · iteration 120 of at most 300 · max force 0.012 Eh/bohr"
        )
    );
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    assert!(!app.xtb_running());
    assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);
    assert_eq!(model(&app.tab.doc), reactant);
    let shown = app.xtb_shown().unwrap();
    assert!(!shown.stale);
    assert_eq!(
        results::View::available(&shown.stored.result),
        [
            results::View::Path,
            results::View::Summary,
            results::View::Vibrations,
            results::View::Log,
        ]
    );
    assert!(
        shown
            .method()
            .contains("GFN2-xTB · axtb 0.2.0 · IDPP → CI-NEB (3 images) → P-RFO → IRC · gas phase")
    );
    send(&mut app, XtbAction::Ts(Action::Image(1)));
    assert_eq!(app.xtb_step(), Some(1));
    assert_eq!(model(app.display_document())[0], selected);
    send(&mut app, XtbAction::Palette(palette::Action::UseStep));
    assert_eq!(
        model(&app.tab.doc),
        reactant,
        "TS previews cannot be applied"
    );
    send(&mut app, XtbAction::Palette(palette::Action::StepBack));
    assert_eq!(model(app.display_document()), reactant);

    let file = app.tab.doc.file_json().unwrap();
    let before = app.tab.doc.clone();
    let product = app.tab.doc.reactions[0].products[0].atoms[0];
    app.tab.doc.atom_mut(product).unwrap().map_num = 0;
    app.changed(before);
    assert!(app.xtb_shown().unwrap().stale);
    send(&mut app, XtbAction::Ts(Action::Image(1)));
    assert_eq!(app.xtb_step(), Some(1));
    assert!(app.xtb_step_preview().is_none());

    let (mut reopened, _) = App::new();
    reopened.tab.doc = Document::from_native_file(&file).unwrap();
    reopened.tab.busy = false;
    reopened.tab.selected = vec![arrow];
    send(&mut reopened, XtbAction::Palette(palette::Action::Show));
    assert_eq!(reopened.tab.xtb.palette.view, results::View::Path);
    assert!(reopened.xtb_shown().unwrap().ts.is_none());
    send(&mut reopened, XtbAction::Ts(Action::Image(1)));
    assert_eq!(reopened.xtb_step(), Some(1));
    assert!(reopened.xtb_step_preview().is_none());
    send(&mut reopened, XtbAction::Ts(Action::Rerun));
    assert_eq!(
        reopened
            .tab
            .xtb
            .run
            .as_ref()
            .unwrap()
            .ts
            .as_ref()
            .unwrap()
            .request
            .images,
        20
    );
}

#[test]
fn saved_ts_without_modes_uses_the_vibrations_recompute() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let report = example_report(run.launch.as_ref().unwrap());
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );

    let file = app.tab.doc.file_json().unwrap();
    let mut reopened = reaction_app(Document::from_native_file(&file).unwrap(), arrow);
    send(&mut reopened, XtbAction::Palette(palette::Action::Show));
    let shown = reopened.xtb_shown().unwrap();
    let result = Arc::clone(&shown.stored.result);
    let atoms = shown.atoms.to_vec();
    let frame = Arc::clone(shown.frame);
    assert!(reopened.xtb_modes(&result).is_none());

    let vibrations = XtbAction::Palette(palette::Action::View(results::View::Vibrations));
    send(&mut reopened, vibrations.clone());
    assert!(!reopened.tab.xtb.vibrations.recomputing(&result));

    // As for other light results, recomputation requires the model at the result geometry.
    reshiki::science::move_model(&mut reopened.tab.doc, &atoms, &frame).unwrap();
    send(&mut reopened, vibrations);
    assert!(reopened.tab.xtb.vibrations.recomputing(&result));
}

#[test]
fn reopened_light_and_full_paths_become_stale_after_an_opposite_side_map_edit() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let report = example_report(run.launch.as_ref().unwrap());
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    assert!(!app.xtb_shown().unwrap().stale);

    for keep_full in [false, true] {
        reshiki::science::set_keep_full(&mut app.tab.doc, 0, keep_full);
        let file = app.tab.doc.file_json().unwrap();
        let (mut reopened, _) = App::new();
        reopened.tab.doc = Document::from_native_file(&file).unwrap();
        reopened.tab.busy = false;
        reopened.tab.selected = vec![arrow];
        send(&mut reopened, XtbAction::Palette(palette::Action::Show));
        let shown = reopened.xtb_shown().unwrap();
        assert!(
            shown.ts.is_none(),
            "reload has no in-memory reaction session"
        );
        assert_eq!(shown.stored.keep_full, keep_full);
        assert!(!shown.stale, "the unchanged saved reaction is current");

        let before = reopened.tab.doc.clone();
        let product = reopened.tab.doc.reactions[0].products[0].atoms[0];
        assert!(
            !shown.atoms.contains(&Some(product)),
            "edit the opposite side"
        );
        reopened.tab.doc.atom_mut(product).unwrap().map_num = 0;
        reopened.changed(before);
        assert!(
            reopened.xtb_shown().unwrap().stale,
            "the reloaded result must see the changed product (keep_full={keep_full})"
        );
        send(&mut reopened, XtbAction::Ts(Action::Rerun));
        assert!(!reopened.xtb_running(), "a stale reaction cannot be rerun");
    }
}

#[test]
fn path_image_previews_a_hydrogen_nearest_its_new_parent() {
    let (doc, arrow) = card::tests::hydrogen_transfer();
    let mut app = reaction_app(doc, arrow);
    send(
        &mut app,
        XtbAction::Ts(Action::Card(card::Action::Reviewed(true))),
    );
    send(&mut app, XtbAction::Ts(Action::Card(card::Action::Find)));
    let order = app.model_atoms();
    let index = |map| {
        order
            .iter()
            .position(|id| app.tab.doc.atom(*id).unwrap().map_num == map)
            .unwrap()
    };
    let donor = index(2);
    let acceptor = index(3);
    let other_carbon = index(1);
    let mut positions: Vec<_> = order
        .iter()
        .enumerate()
        .map(|(i, _)| Point3 {
            x: i as f64 * 1.4,
            y: 0.,
            z: 0.,
        })
        .collect();
    let moved = Point3 {
        x: positions[acceptor].x,
        y: 0.9,
        z: 0.,
    };
    positions.extend([
        Point3 {
            x: positions[donor].x,
            y: 1.,
            z: 0.,
        },
        Point3 {
            x: positions[donor].x,
            y: -1.,
            z: 0.,
        },
        Point3 {
            x: positions[other_carbon].x,
            y: 1.,
            z: 0.,
        },
        Point3 {
            x: positions[other_carbon].x,
            y: -1.,
            z: 0.,
        },
        Point3 {
            x: positions[other_carbon].x,
            y: 0.,
            z: 1.,
        },
    ]);
    let _ = app.finish_model(
        positions,
        vec![donor, donor, other_carbon, other_carbon, other_carbon],
    );
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let launch = run.launch.as_ref().unwrap();
    let mut report = example_report(launch);
    let report_data = Arc::get_mut(&mut report).unwrap();
    let mut image = report_data.frames[2].clone();
    image.positions_angstrom[order.len()] = [moved.x, moved.y, moved.z];
    report_data.extras.image_positions[2].clone_from(&image.positions_angstrom);
    let before = app.tab.doc.models[0].hydrogens[0];
    assert_eq!(before.parent, donor as u64);
    let mut document = app.tab.doc.clone();
    assert!(matches!(
        reshiki::science::move_model(&mut document, &launch.atoms, &image),
        Err(ScienceError::ModelMismatch)
    ));

    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    send(&mut app, XtbAction::Ts(Action::Image(2)));
    let preview = app.xtb_step_preview().expect("the TS image should preview");
    assert_eq!(
        preview.models[0].hydrogens[0].xyz,
        [moved.x as f32, moved.y as f32, moved.z as f32]
    );
    assert_eq!(preview.models[0].hydrogens[0].parent, before.parent);
    assert_eq!(app.tab.doc.models[0].hydrogens[0], before);
}

#[test]
fn background_ts_completion_preserves_the_foreground_predict_page_and_dock() {
    let (mut app, _) = start();
    finish_model(&mut app);
    let first = app.tab.id;
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let report = example_report(run.launch.as_ref().unwrap());
    app.add_tab();
    let foreground = app.tab.id;
    app.xtb.page = crate::app::xtb::Page::Nmr;
    app.xtb.dock.inspector = true;
    app.xtb.dock.collapsed = true;
    let finished = Message::Xtb(XtbAction::TsEvent(serial, Event::Done(Ok(report))));
    let _ = app.update(Message::Tab(first, Box::new(finished)));
    assert_eq!(app.tab.id, foreground);
    assert_eq!(app.xtb.page, crate::app::xtb::Page::Nmr);
    assert!(app.xtb.dock.collapsed);
    assert!(app.tab.doc.science.results.is_empty());
    assert_eq!(
        app.in_tab(first, |app| app.tab.doc.science.results.len()),
        Some(1)
    );
}

#[test]
fn cancel_drops_generation_and_a_late_finish_cannot_attach() {
    let (mut app, _) = start();
    send(&mut app, XtbAction::Cancel);
    assert!(app.tab.model_job.is_none());
    assert!(!app.xtb_running());
    assert!(app.tab.doc.models.is_empty());
    send(&mut app, XtbAction::Ts(Action::Card(card::Action::Find)));
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let report = example_report(run.launch.as_ref().unwrap());
    let model_before = model(&app.tab.doc);
    send(&mut app, XtbAction::Cancel);
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    assert_eq!(app.status, "Calculation canceled");
    assert!(!app.xtb_running());
    assert!(app.tab.doc.science.results.is_empty());
    assert_eq!(model(&app.tab.doc), model_before);
}
