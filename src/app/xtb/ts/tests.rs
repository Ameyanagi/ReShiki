//! Exercise the normal app flow with fabricated worker events.
use super::*;
use crate::app::xtb::{palette, results, run, tests::model};
use reshiki::document::Document;
use reshiki::geometry::Point3;
use reshiki::qcdata::{Endpoints, Status, Trajectory};
use reshiki::science::Launch;

fn send(app: &mut App, action: XtbAction) {
    let _ = app.update(Message::Xtb(action));
}

fn start() -> (App, u64) {
    let (doc, arrow) = card::tests::claisen();
    let (mut app, _) = App::new();
    app.tab.doc = doc;
    app.tab.selected = vec![arrow];
    app.tab.busy = false;
    send(&mut app, XtbAction::Ts(Action::Card(card::Action::Find)));
    let plan = ts_plan::plan(&app.tab.doc, arrow).unwrap();
    assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, plan.start_atoms);
    assert!(app.tab.model_job.is_some());
    (app, arrow)
}

fn finish_model(app: &mut App) {
    let positions = (0..app.model_atoms().len())
        .map(|i| Point3 {
            x: i as f64 * 1.4,
            y: 0.,
            z: 0.,
        })
        .collect();
    let _ = app.finish_model(positions, vec![]);
    assert!(
        app.tab.xtb.run.as_ref().unwrap().launch.is_some(),
        "{}",
        app.status
    );
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
