//! Exercise the normal app flow with fabricated worker events.
use super::*;
use crate::app::xtb::{
    palette, results, run,
    tests::{f32s, model},
};
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

fn select(app: &mut App, arrow: u64) {
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![arrow])));
    assert_eq!(app.xtb_selected_reaction(), Some(arrow));
}

fn name_reactant(doc: &mut Document, arrow: u64, name: &str) {
    let atoms = reshiki::reaction_mapping::reaction(doc, arrow)
        .unwrap()
        .reactants[0]
        .atoms
        .clone();
    let identity = reshiki::naming::selected_identity(doc, &atoms)
        .unwrap()
        .smiles;
    reshiki::molecule_names::show(
        doc,
        &atoms,
        name.into(),
        identity,
        Default::default(),
        false,
    )
    .unwrap();
}

fn append_reaction(doc: &mut Document, name: &str) -> u64 {
    let (mut source, arrow) = card::tests::claisen();
    name_reactant(&mut source, arrow, name);
    reshiki::editing::append_preserving_layout(
        doc,
        &source,
        reshiki::document::Point::new(0., 160.),
    );
    doc.reactions.last().unwrap().arrow
}

fn start() -> (App, u64) {
    let (doc, arrow) = card::tests::claisen();
    let mut app = reaction_app(doc, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
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
    if let Some(ts) = &run.ts {
        assert_eq!(launch.frame.numbers.len(), ts.request.plan.atoms);
    }
    assert_eq!(launch.frame.numbers.iter().filter(|&&z| z == 1).count(), 8);
}

#[test]
fn barrier_result_places_the_start_model_under_its_species() {
    let (mut doc, arrow) = card::tests::claisen();
    let product = doc.reactions[0].products[0].atoms[2];
    doc.atom_mut(product).unwrap().position.y = 75.;
    let agent = doc.add_atom("O", reshiki::document::Point::new(50., 100.));
    doc.reactions[0]
        .agents
        .push(reshiki::reactions::Participant {
            atoms: vec![agent],
            coefficient: 1,
        });
    let caption = reshiki::molecule_names::show(
        &mut doc,
        &[agent],
        "water".into(),
        "O".into(),
        Default::default(),
        false,
    )
    .unwrap();
    let reaction = &doc.reactions[0];
    let objects = [vec![arrow], vec![caption]]
        .into_iter()
        .chain(
            reshiki::reactions::Role::ALL
                .into_iter()
                .flat_map(|role| reaction.participants(role))
                .map(|participant| participant.atoms.clone()),
        )
        .collect::<Vec<_>>();
    let bounds = reshiki::scene::selections_bounds(&doc, &objects);
    let mut app = reaction_app(doc, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    finish_model(&mut app);
    assert!(app.tab.doc.models.is_empty());
    let start = app.xtb_live_ts().unwrap().request.plan.start_atoms[0];
    complete(&mut app);

    let doc = &app.tab.doc;
    let (lo, hi) = reshiki::models3d::frame(doc, &doc.models[0]).unwrap();
    let (a, b) = bounds[objects.iter().position(|ids| ids.contains(&start)).unwrap()].unwrap();
    assert!(((lo.x + hi.x - a.x - b.x) / 2.).abs() < 0.01);
    assert!(lo.y >= b.y + doc.drawing_style.bond_length_world - 0.01);
    for (a, b) in bounds.into_iter().map(Option::unwrap) {
        let overlaps = lo.x < b.x && hi.x > a.x && lo.y < b.y && hi.y > a.y;
        assert!(!overlaps, "frame {lo:?}–{hi:?} covers {a:?}–{b:?}");
    }
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.models.is_empty());
    assert!(app.tab.doc.science.results.is_empty());
}

#[test]
fn saved_path_model_preview_is_centered_under_the_arrow() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    complete(&mut app);
    let original = app.tab.doc.clone();
    let atoms = original.reactions[0].reactants[0].atoms.clone();
    let (species_lo, species_hi) = reshiki::scene::selection_bounds(
        &Document {
            models: vec![],
            ..original.clone()
        },
        &atoms,
    )
    .unwrap();
    send(&mut app, XtbAction::Ts(Action::Image(1)));

    let doc = app.display_document();
    let (a, b) = doc
        .arrows
        .iter()
        .find(|item| item.id == arrow)
        .unwrap()
        .bounds();
    let (lo, hi) = reshiki::models3d::frame(doc, &doc.models[0]).unwrap();
    assert!(((lo.x + hi.x - a.x - b.x) / 2.).abs() < 0.01);
    assert!(lo.y >= b.y + doc.drawing_style.bond_length_world - 0.01);
    let (species_model_lo, species_model_hi) =
        reshiki::models3d::frame(doc, &doc.models[1]).unwrap();
    assert!((species_model_lo.y - lo.y).abs() < 0.01);
    assert!(
        ((species_model_lo.x + species_model_hi.x - species_lo.x - species_hi.x) / 2.).abs() < 0.01
    );
    assert_eq!(app.tab.doc, original);
}

fn example_report(launch: &Launch) -> Arc<job::Report> {
    let reactant = (*launch.frame).clone();
    let mut transition = reactant.clone();
    transition.positions_angstrom[0][2] += 0.5;
    let mut product = reactant.clone();
    product.positions_angstrom[0][2] += 1.;
    let mut result = run::example_computed(-4.95777, 0.9)
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
        rmsd_to_best_angstrom: Vec::new(),
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
    result.run = Some(reshiki::qcdata::RunSummary {
        seconds: 2.,
        scf_iterations: vec![9, 4],
        finished: Some("2026-11-25T12:00:00Z".into()),
        search: None,
        transition_state: None,
    });
    let extras = Extras {
        endpoint_energies_hartree: [-5., -5.02],
        endpoint_thermo: None,
        neb_converged: true,
        candidate: None,
        reached_positions_angstrom: vec![],
        image_positions: [&reactant, &transition, &product]
            .map(|frame| frame.positions_angstrom.clone())
            .to_vec(),
        saddle_converged: true,
        num_imaginary: 1,
        diagnostics: vec![
            ("Method".into(), "IDPP → CI-NEB → P-RFO → IRC".into()),
            ("NEB".into(), "120 iterations · converged".into()),
        ],
        connects: true,
        climbing: 1,
        engine_imaginary: Some(-387.),
        worker_frequencies: vec![-387.],
        irc: None,
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

fn prepare_higher_order_candidate(report: &mut job::Report) {
    report.result.trajectory.as_mut().unwrap().energies_hartree =
        vec![-5., -4.99, -4.93, -4.96, -5.02];
    let images = &mut report.extras.image_positions;
    images.insert(1, images[0].clone());
    images.insert(3, images[2].clone());
    let positions = job::bohr(&report.frames[0].positions_angstrom);
    report.extras.candidate = Some(reshiki_optimize::ts::SaddleCandidate {
        hessian: vec![0.; positions.len() * positions.len()],
        positions,
    });
    report.extras.connects = false;
    report.extras.num_imaginary = 2;
    report.extras.worker_frequencies = vec![-387., -100.];
    let vibrations = report.result.vibrations.as_mut().unwrap();
    vibrations.frequencies_cm1 = report.extras.worker_frequencies.clone();
    vibrations.ir_intensities_km_mol = vec![12., 3.];
    report.result.status = Status::NotConverged {
        reason: "expected one imaginary frequency below -50 cm-1, found 2".into(),
    };
}

fn complete(app: &mut App) {
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let report = example_report(run.launch.as_ref().unwrap());
    send(app, XtbAction::TsEvent(serial, Event::Done(Ok(report))));
}

#[test]
fn transition_state_status_follows_energy_units_and_clears_on_molecule_selection() {
    use crate::appearance::EnergyUnit;

    let (mut app, _) = start();
    finish_model(&mut app);
    complete(&mut app);
    assert_eq!(
        app.status,
        "Transition state · GFN2-xTB · ΔE‡ 26.5 kcal/mol"
    );
    let _ = app.update(Message::Settings(
        crate::app::settings_page::Action::EnergyUnit(EnergyUnit::KjPerMol),
    ));
    assert_eq!(app.status, "Transition state · GFN2-xTB · ΔE‡ 110.9 kJ/mol");
    let atom = app.tab.doc.reactions[0].reactants[0].atoms[0];
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![atom])));
    assert!(!app.status.contains("Transition state"), "{}", app.status);
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn narrow_reaction_card_shows_endpoint_energies_in_the_selected_unit() {
    use crate::appearance::EnergyUnit;

    let (mut app, arrow) = start();
    finish_model(&mut app);
    complete(&mut app);
    let ui = card::tests::ViewHarness::new().await.at_width(330.);
    for (unit, barrier, reaction) in [
        (EnergyUnit::KcalPerMol, "26.5", "−12.6"),
        (EnergyUnit::KjPerMol, "110.9", "−52.5"),
        (EnergyUnit::Ev, "1.15", "−0.54"),
    ] {
        app.appearance.energy_unit = unit;
        let card = ui.inspect(app.xtb_reaction_card(arrow));
        let displayed = |value: &str| {
            card.texts
                .iter()
                .position(|text| text == value)
                .unwrap_or_else(|| panic!("missing {value:?} in {:?}", card.texts))
        };
        let recalculate = displayed("Recalculate");
        for value in ["GFN2-xTB Barrier ΔE‡", barrier, reaction, unit.label()] {
            assert!(displayed(value) < recalculate);
        }
        assert!(card.semantics.duplicate_ids.is_empty());
    }
}

#[test]
fn selecting_reactions_switches_the_palette_and_restores_its_path_preview() {
    let (mut app, a) = start();
    finish_model(&mut app);
    complete(&mut app);
    let a_result = Arc::clone(&app.xtb_shown().unwrap().stored.result);
    let mut candidate = app.tab.doc.clone();
    let b = append_reaction(&mut candidate, "Second reactant");
    let c = append_reaction(&mut candidate, "Third reactant");
    app.try_replace_document(candidate, vec![a]).unwrap();

    select(&mut app, b);
    assert!(app.xtb_bottom_palette().is_none());
    send(&mut app, XtbAction::Ts(Action::Calculate(b)));
    finish_model(&mut app);
    complete(&mut app);
    let b_result = Arc::clone(&app.xtb_shown().unwrap().stored.result);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(results::View::Log)),
    );

    select(&mut app, a);
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &a_result
    ));
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);
    send(&mut app, XtbAction::Ts(Action::Image(1)));
    assert!(app.xtb_step_preview().is_some());
    select(&mut app, b);
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &b_result
    ));
    assert_eq!(app.tab.xtb.palette.view, results::View::Log);
    assert!(app.xtb_step_preview().is_none());
    select(&mut app, c);
    assert!(app.xtb_bottom_palette().is_none());
    select(&mut app, a);
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);
    assert_eq!(app.xtb_step(), Some(1));
    assert!(app.xtb_step_preview().is_some());
}

#[test]
fn changing_a_fallback_molecule_tab_preserves_the_reaction_path_preview() {
    use super::super::tests::{MOVED, START, event, optimized, water};

    let (mut app, arrow) = start();
    finish_model(&mut app);
    complete(&mut app);
    let (mut molecule, _) = water(super::super::CalcTask::Optimize);
    event(&mut molecule, optimized(&START, &MOVED));
    let mut doc = app.tab.doc.clone();
    reshiki::editing::append_preserving_layout(
        &mut doc,
        &molecule.tab.doc,
        reshiki::document::Point::new(0., 200.),
    );
    let oxygen = doc.atoms.last().unwrap().id;
    app.try_replace_document(doc, vec![arrow]).unwrap();

    send(&mut app, XtbAction::Ts(Action::Image(1)));
    assert!(app.xtb_step_preview().is_some());
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![oxygen])));
    assert_eq!(
        app.xtb_shown().unwrap().stored.result.task,
        CalcTask::Optimize
    );
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(results::View::Summary)),
    );
    assert_eq!(app.tab.xtb.palette.view, results::View::Summary);
    assert!(app.xtb_step_preview().is_none());

    select(&mut app, arrow);
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);
    assert_eq!(app.xtb_step(), Some(1));
    assert!(app.xtb_step_preview().is_some());
}

#[test]
fn saved_arrow_with_an_unrelated_attachment_stays_unbound() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    complete(&mut app);
    let mut doc = Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap();
    let other = append_reaction(&mut doc, "Unrelated reactant");
    let result = Arc::make_mut(&mut doc.science.results[0].result);
    result
        .run
        .as_mut()
        .unwrap()
        .transition_state
        .as_mut()
        .unwrap()
        .arrow = Some(other);
    let mut reopened = reaction_app(doc, other);
    send(&mut reopened, XtbAction::Palette(palette::Action::Show));
    assert!(matches!(
        reopened.xtb_ts_card_result(other),
        card::ArrowResult::Missing
    ));
    assert!(reopened.xtb_shown().is_none());
    assert!(matches!(
        reopened.xtb_ts_card_result(arrow),
        card::ArrowResult::Missing
    ));
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn worker_and_storage_failures_show_before_the_previous_reaction_result() {
    let ui = card::tests::ViewHarness::new().await;
    for storage_failure in [false, true] {
        let (mut app, arrow) = start();
        finish_model(&mut app);
        complete(&mut app);
        let previous = Arc::clone(&app.xtb_shown().unwrap().stored.result);
        let mut candidate = app.tab.doc.clone();
        let other = append_reaction(&mut candidate, "Unrelated reactant");
        app.try_replace_document(candidate, vec![arrow]).unwrap();
        send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
        let run = app.tab.xtb.run.as_ref().unwrap();
        let serial = run.serial;
        if storage_failure {
            let science = &mut app.tab.doc.science;
            let available = reshiki::science::MAX_SCIENCE_RECORDS
                - science.frames.len()
                - science.attachments.len();
            science
                .results
                .resize(available, science.results[0].clone());
            complete(&mut app);
        } else {
            send(
                &mut app,
                XtbAction::TsEvent(
                    serial,
                    Event::Done(Err(state::Failure::Crashed {
                        summary: "worker stopped".into(),
                        details: None,
                    })),
                ),
            );
        }
        let Some(super::super::Outcome::Failed(failed)) = &app.tab.xtb.outcome else {
            panic!("the attempt should have failed");
        };
        assert_eq!(failed.ts().unwrap().arrow, arrow);
        let (failure, _) = failed.explain(app.xtb_can_retry(failed));
        if storage_failure {
            assert!(failure.contains("would exceed"), "{failure}");
        }
        assert!(Arc::ptr_eq(
            &app.xtb_shown().unwrap().stored.result,
            &previous
        ));
        let title = format!("xTB results · {}", app.xtb_reaction_label(arrow).title());
        let palette = ui.inspect(app.xtb_bottom_palette().unwrap()).texts;
        let position = |text: &str| palette.iter().position(|item| item == text).unwrap();
        let earlier = palette
            .iter()
            .position(|text| text.starts_with("Previous result ("))
            .unwrap();
        assert!(position(&title) < position(&failure));
        assert!(position(&failure) < earlier);
        assert!(palette[earlier].contains("Nov 2026"));
        app.xtb.page = super::super::Page::Xtb;
        let card = ui.inspect(app.predict_inspector()).texts;
        assert!(
            card.iter()
                .position(|text| text == "No reliable transition state found.")
                .unwrap()
                < card
                    .iter()
                    .position(|text| text == &palette[earlier])
                    .unwrap()
        );
        select(&mut app, other);
        assert!(app.xtb_bottom_palette().is_none());
        assert!(!ui.inspect(app.predict_inspector()).texts.contains(&failure));
        select(&mut app, arrow);
        assert!(Arc::ptr_eq(
            &app.xtb_shown().unwrap().stored.result,
            &previous
        ));
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn deselecting_keeps_the_failed_rerun_on_the_shown_reaction() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    complete(&mut app);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    let serial = app.tab.xtb.run.as_ref().unwrap().serial;
    send(
        &mut app,
        XtbAction::TsEvent(
            serial,
            Event::Done(Err(state::Failure::Crashed {
                summary: "worker stopped".into(),
                details: None,
            })),
        ),
    );

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![])));
    assert!(app.tab.selected.is_empty());
    assert_eq!(app.xtb_shown().unwrap().arrow, Some(arrow));

    let ui = card::tests::ViewHarness::new().await;
    let palette = ui.inspect(app.xtb_bottom_palette().unwrap()).texts;
    assert!(
        palette
            .iter()
            .any(|text| text == "The calculation stopped unexpectedly. Your drawing is safe.")
    );
    assert!(
        palette
            .iter()
            .any(|text| text.starts_with("Previous result ("))
    );
}

#[test]
fn an_unrelated_molecule_failure_does_not_interrupt_the_shown_ts_vibration() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    let data = Arc::get_mut(&mut report).unwrap();
    let modes = &mut data.result.vibrations.as_mut().unwrap().modes;
    *modes = vec![0.; data.frames[0].numbers.len() * 3];
    modes[0] = 1.;
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    let mut doc = app.tab.doc.clone();
    let other = doc.add_atom("O", reshiki::document::Point::new(0., 200.));
    app.try_replace_document(doc, vec![arrow]).unwrap();
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(results::View::Vibrations)),
    );
    assert!(app.xtb_vibration_preview().is_some());

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![other])));
    app.xtb.settings.iterations = "invalid".into();
    send(&mut app, XtbAction::Run);
    assert_eq!(app.xtb_failed().unwrap().atoms, vec![other]);
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![])));

    assert_eq!(app.xtb_shown().unwrap().arrow, Some(arrow));
    assert!(app.xtb_failed().is_none());
    assert!(app.xtb_vibration_preview().is_some());
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn reopened_legacy_result_is_labelled_not_verified() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    complete(&mut app);
    let mut doc = Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap();
    let result = Arc::make_mut(&mut doc.science.results[0].result);
    result
        .run
        .as_mut()
        .unwrap()
        .transition_state
        .as_mut()
        .unwrap()
        .context = None;
    let mut reopened = reaction_app(doc, arrow);
    send(&mut reopened, XtbAction::Palette(palette::Action::Show));
    assert!(reopened.xtb_shown().unwrap().stale);
    let ui = card::tests::ViewHarness::new().await;
    assert!(
        ui.inspect(reopened.xtb_bottom_palette().unwrap())
            .texts
            .iter()
            .any(|text| text.starts_with("Reaction freshness not verified"))
    );
}

fn reopened_reaction_card() -> (App, u64) {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    complete(&mut app);
    let doc = Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap();
    let mut reopened = reaction_app(doc, arrow);
    reopened.inspector_open = true;
    reopened.inspector_tab = crate::app::InspectorTab::Predict;
    reopened.xtb.page = super::super::Page::Xtb;
    assert_eq!(reopened.xtb_reaction_bond_marks().len(), 2);
    (reopened, arrow)
}

#[test]
fn reopened_reaction_hides_saved_bond_changes_without_a_complete_drawn_map() {
    let (mut reopened, arrow) = reopened_reaction_card();
    let product = reopened.tab.doc.reactions[0].products[0].atoms[0];
    reopened.tab.doc.atom_mut(product).unwrap().map_num = 0;
    assert!(review::scheme_plan(&reopened.tab.doc, arrow).is_err());
    assert!(reopened.xtb_reaction_bond_marks().is_empty());
}

#[test]
fn reopened_reaction_hides_saved_bond_changes_after_claisen_map_renumbering() {
    let (mut reopened, arrow) = reopened_reaction_card();
    let doc = &mut reopened.tab.doc;
    let reactants = &doc.reactions[0].reactants[0].atoms;
    let (c1, c2) = (reactants[0], reactants[1]);
    doc.atom_mut(c1).unwrap().map_num = 0;
    doc.atom_mut(c2).unwrap().map_num = 1;
    let search = saved_search(&doc.science.results[0].result).unwrap();
    assert_eq!(
        search.context.as_ref().unwrap().reactant_chemistry,
        endpoint_chemistry(doc, &doc.reactions[0], Role::Reactant),
        "the chemistry fingerprint cannot validate saved atom map numbers"
    );
    assert!(review::scheme_plan(doc, arrow).is_err());
    assert!(reopened.xtb_reaction_bond_marks().is_empty());
}

#[test]
fn reopened_reaction_hides_saved_bond_changes_when_the_map_has_no_changes() {
    let (mut reopened, arrow) = reopened_reaction_card();
    let doc = &mut reopened.tab.doc;
    let reaction = &doc.reactions[0];
    let reactants = reaction.reactants[0].atoms.clone();
    let products = reaction.products[0].atoms.clone();
    let bonds: Vec<_> = doc
        .bonds
        .iter()
        .filter_map(|bond| {
            let a = reactants.iter().position(|&id| id == bond.a)?;
            let b = reactants.iter().position(|&id| id == bond.b)?;
            Some((products[a], products[b], bond.order))
        })
        .collect();
    doc.bonds
        .retain(|bond| !products.contains(&bond.a) || !products.contains(&bond.b));
    for (a, b, order) in bonds {
        doc.add_bond(a, b, order, "plain");
    }
    assert!(matches!(
        review::scheme_plan(doc, arrow),
        Err(ts_plan::TsPlanError::NoChange)
    ));
    assert!(reopened.xtb_reaction_bond_marks().is_empty());
}

#[test]
fn reopened_reaction_uses_current_bond_changes_after_valid_map_renumbering() {
    let (mut reopened, arrow) = reopened_reaction_card();
    let expected = reopened.xtb_reaction_bond_marks();
    for atom in &mut reopened.tab.doc.atoms {
        if atom.map_num == 6 {
            atom.map_num = 7;
        }
    }
    assert!(review::scheme_plan(&reopened.tab.doc, arrow).is_ok());
    assert_eq!(reopened.xtb_reaction_bond_marks(), expected);
}

#[test]
fn stale_result_keeps_its_saved_title_and_export_name() {
    let (mut doc, arrow) = card::tests::claisen();
    name_reactant(&mut doc, arrow, "allyl vinyl ether");
    let mut app = reaction_app(doc, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    finish_model(&mut app);
    complete(&mut app);
    let mut doc = Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap();
    let saved_title = saved_search(&doc.science.results[0].result)
        .unwrap()
        .context
        .as_ref()
        .unwrap()
        .title
        .clone();
    assert_eq!(saved_title, "allyl vinyl ether → C5H8O");
    name_reactant(&mut doc, arrow, "Renamed reactant");
    let product = doc.reactions[0].products[0].atoms[0];
    doc.atom_mut(product).unwrap().map_num = 0;
    let mut reopened = reaction_app(doc, arrow);
    send(&mut reopened, XtbAction::Palette(palette::Action::Show));
    let shown = reopened.xtb_shown().unwrap();
    assert!(shown.stale);
    assert_ne!(reopened.xtb_reaction_label(arrow).title(), saved_title);
    assert_eq!(
        reopened.xtb_saved_reaction_title(&shown.stored.result, shown.arrow),
        saved_title
    );
    send(&mut reopened, XtbAction::Ts(Action::Export));
    let output = reopened.qc_output().unwrap();
    assert_eq!(output.main.name, "allyl-vinyl-ether-ts.gjf");
    assert!(output.main.text.contains(&saved_title));
}

#[test]
fn stepper_keeps_stage_times_across_interpolation_and_frequency_events() {
    let (mut app, _) = start();
    let live = app.tab.xtb.run.as_mut().unwrap().ts.as_mut().unwrap();
    let start = live.stage_started;
    let ms = Duration::from_millis;
    let states = |live: &Live, now| {
        live.steps(start + ms(now))
            .map(|(_, state)| state)
            .collect::<Vec<_>>()
    };
    use StepState::{Current, Done, Todo};

    assert_eq!(states(live, 350), [Current(ms(350)), Todo, Todo, Todo]);
    live.request.free_energy = true;
    live.advance(Stage::Endpoints, start + ms(700));
    live.advance(Stage::Idpp, start + ms(800));
    live.advance(Stage::Neb, start + ms(950));
    assert_eq!(
        states(live, 3100),
        [Done(Some(ms(800))), Current(ms(2300)), Todo, Todo, Todo]
    );
    live.advance(Stage::Saddle, start + ms(3100));
    live.advance(Stage::Saddle, start + ms(3700));
    live.advance(Stage::Irc, start + ms(3900));
    live.advance(Stage::Thermochemistry, start + ms(4400));
    assert_eq!(
        states(live, 4700),
        [
            Done(Some(ms(800))),
            Done(Some(ms(2300))),
            Done(Some(ms(800))),
            Done(Some(ms(500))),
            Current(ms(300))
        ]
    );
}

#[test]
fn candidate_retry_times_refinement_and_marks_an_untraced_irc_skipped() {
    let (mut app, _) = start();
    let run = app.tab.xtb.run.as_mut().unwrap();
    let serial = run.serial;
    let mut request = run.ts.as_ref().unwrap().request.clone();
    request.retry = Some(Retry::RefineHighest);
    request.free_energy = true;
    run.ts = Some(Live::new(request));
    let live = run.ts.as_mut().unwrap();
    let start = live.stage_started;
    use StepState::{Current, Done, Skipped, Todo};
    let states = |live: &Live, millis| {
        live.steps(start + Duration::from_millis(millis))
            .map(|(_, state)| state)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        states(live, 50),
        [
            Done(None),
            Done(None),
            Current(Duration::from_millis(50)),
            Todo,
            Todo
        ]
    );
    app.xtb_ts_event(serial, Event::IrcSkipped);
    let live = app.tab.xtb.run.as_mut().unwrap().ts.as_mut().unwrap();
    live.advance(Stage::Thermochemistry, start + Duration::from_millis(70));
    assert_eq!(
        states(live, 100),
        [
            Done(None),
            Done(None),
            Done(Some(Duration::from_millis(70))),
            Skipped,
            Current(Duration::from_millis(30))
        ]
    );
}

fn molecule_summary() -> (App, u64, Arc<CalcResult>) {
    use crate::app::xtb::tests::{START, event, water};
    let (mut app, molecule) = water(state::Task::Energy);
    event(
        &mut app,
        run::Event::Done(Ok(Arc::new(run::Report {
            engine: run::example_engine(),
            computed: run::example_computed(-5.07, 0.1),
            energy: -5.07,
            converged: true,
            positions: START.to_vec(),
            steps: 0,
            optimizers: vec![],
            energies: vec![-5.07],
            path: vec![],
            evaluations: vec![],
        }))),
    );
    assert_eq!(app.tab.xtb.palette.view, results::View::Summary);
    let result = Arc::clone(&app.xtb_shown().unwrap().stored.result);
    (app, molecule, result)
}

#[test]
fn a_search_retains_an_unrelated_result_and_molecule_selection_leaves_the_live_path() {
    let (mut app, molecule, result) = molecule_summary();
    let arrow = append_reaction(&mut app.tab.doc, "Allyl vinyl ether");
    select(&mut app, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    assert!(app.xtb_live_ts().is_some());
    assert!(app.xtb_palette_live_ts().is_none());
    assert_eq!(app.tab.xtb.palette.view, results::View::Summary);
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &result
    ));
    finish_model(&mut app);

    send(
        &mut app,
        XtbAction::Ts(Action::Card(card::Action::ShowPath)),
    );
    assert!(app.xtb_palette_live_ts().is_some());
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);
    assert!(app.xtb_shown().is_none());

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![molecule])));
    assert!(
        app.xtb_live_ts().is_some(),
        "selection keeps the search running"
    );
    assert!(app.xtb_palette_live_ts().is_none());
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &result
    ));
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(results::View::Log)),
    );
    assert_eq!(app.tab.xtb.palette.view, results::View::Log);
    complete(&mut app);
    assert_eq!(app.tab.xtb.palette.view, results::View::Log);
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &result
    ));
    assert!(matches!(
        app.xtb_ts_card_result(arrow),
        card::ArrowResult::Found(_)
    ));
}

#[test]
fn a_search_completion_keeps_an_unrelated_dock_when_the_live_path_was_not_chosen() {
    let (mut app, _, result) = molecule_summary();
    let arrow = append_reaction(&mut app.tab.doc, "Allyl vinyl ether");
    select(&mut app, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    finish_model(&mut app);
    app.tab.xtb.palette.collapsed = true;
    complete(&mut app);
    assert!(app.tab.xtb.palette.collapsed);
    assert_eq!(app.tab.xtb.palette.view, results::View::Summary);
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &result
    ));
    assert!(matches!(
        app.xtb_ts_card_result(arrow),
        card::ArrowResult::Found(_)
    ));
}

#[test]
fn starting_a_different_reaction_keeps_the_saved_path_point_in_the_dock() {
    let (mut app, _) = start();
    finish_model(&mut app);
    complete(&mut app);
    let result = Arc::clone(&app.xtb_shown().unwrap().stored.result);
    let other = append_reaction(&mut app.tab.doc, "Other reaction");
    send(&mut app, XtbAction::Ts(Action::Image(1)));
    send(&mut app, XtbAction::Ts(Action::Calculate(other)));
    assert_eq!(app.xtb_live_ts().unwrap().request.arrow, other);
    assert!(app.xtb_palette_live_ts().is_none());
    assert_eq!(app.xtb_step(), Some(1));
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &result
    ));
    finish_model(&mut app);
    complete(&mut app);
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &result
    ));
}

#[test]
fn a_search_opens_its_live_path_for_a_closed_empty_or_matching_dock() {
    let (mut app, _, _) = molecule_summary();
    send(&mut app, XtbAction::Palette(palette::Action::Close));
    let arrow = append_reaction(&mut app.tab.doc, "Allyl vinyl ether");
    select(&mut app, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    assert!(app.tab.xtb.palette.open);
    assert!(
        app.xtb_palette_live_ts().is_some(),
        "the closed dock opens live"
    );

    let (mut app, arrow) = start();
    assert!(
        app.xtb_palette_live_ts().is_some(),
        "the empty dock opens live"
    );
    finish_model(&mut app);
    complete(&mut app);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(results::View::Log)),
    );
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    assert!(
        app.xtb_palette_live_ts().is_some(),
        "this reaction's dock opens live"
    );
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn running_reaction_keeps_molecule_summary_until_show_live_path_is_chosen() {
    let ui = card::tests::ViewHarness::new().await;
    let (mut app, _, result) = molecule_summary();
    let arrow = append_reaction(&mut app.tab.doc, "Allyl vinyl ether");
    select(&mut app, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &result
    ));
    assert!(
        ui.inspect(app.xtb_bottom_palette().unwrap())
            .semantics
            .nodes
            .iter()
            .all(|node| node.id != "xtb-ts-path-profile")
    );
    let show_live = ui
        .activate(app.xtb_reaction_card(arrow), "xtb-ts-show-live-path")
        .expect("the stepper offers the live path");
    let _ = app.update(show_live);
    let inspect = |app: &App| {
        let snapshot = ui.inspect(app.xtb_bottom_palette().unwrap());
        assert!(
            snapshot
                .texts
                .iter()
                .any(|text| text.contains("Allyl vinyl ether"))
        );
        assert!(
            snapshot
                .semantics
                .nodes
                .iter()
                .any(|node| node.id == "xtb-results-path" && node.checked == Some(true))
        );
        snapshot
    };
    let snapshot = inspect(&app);
    assert!(snapshot.texts.iter().any(|text| text == "Finding a path…"));
    assert!(app.xtb_shown().is_none());
    finish_model(&mut app);
    let serial = app.tab.xtb.run.as_ref().unwrap().serial;
    for (iteration, energies) in [(0, vec![-5., -4.93, -5.01]), (1, vec![-5., -4.92, -5.02])] {
        send(
            &mut app,
            XtbAction::TsEvent(
                serial,
                Event::Neb {
                    iteration,
                    energies: energies.clone(),
                },
            ),
        );
        assert_eq!(app.xtb_live_ts().unwrap().energies, energies);
    }
    let snapshot = inspect(&app);
    assert!(snapshot.semantics.nodes.iter().any(|node| {
        node.id == "xtb-ts-path-profile"
            && node
                .name
                .starts_with("Live reaction path energy profile · 3 images")
            && node.name.contains("ΔE (kcal/mol)")
    }));
    complete(&mut app);
    let snapshot = inspect(&app);
    assert!(
        snapshot
            .semantics
            .nodes
            .iter()
            .any(|node| node.id == "xtb-ts-path-profile" && node.name.contains("TS‡ 26.5"))
    );
    assert_eq!(
        path_energies(&app.xtb_shown().unwrap().stored.result),
        [-5., -4.9, -5.02]
    );
}

#[test]
fn search_progress_and_finish_keep_the_drawing_and_open_a_previewable_path() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    Arc::get_mut(&mut report).unwrap().extras.diagnostics.push((
        "IRC +mode".into(),
        "reactant · downhill trace not converged · 10 points".into(),
    ));
    let reactant = f32s(&run.launch.as_ref().unwrap().frame.positions_angstrom);
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
                energies: vec![-5., -4.96, -5.02],
            },
        ),
    );
    assert_eq!(app.status, "GFN2-xTB · Finding a path · iteration 120");
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    assert!(!app.xtb_running());
    assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);
    assert!(app.status.contains("ΔE‡ 26.5 kcal/mol"), "{}", app.status);
    assert!(!app.status.contains("62.8"), "{}", app.status);
    assert_eq!(model(&app.tab.doc), reactant);
    let shown = app.xtb_shown().unwrap();
    assert!(!shown.stale);
    assert_eq!(
        view::irc_path_detail(&shown.stored.result, shown.ts, shown.stale).as_deref(),
        Some("The path stopped after 10 steps on the reactant side.")
    );
    assert_eq!(
        results::View::available(&shown.stored.result),
        [
            results::View::Path,
            results::View::Summary,
            results::View::Vibrations,
            results::View::Log,
        ]
    );
    assert!(shown.method().contains("GFN2-xTB · axtb 0.2.0 · gas phase"));
    assert!(!shown.method().contains("CI-NEB"));
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
    let shown = reopened.xtb_shown().unwrap();
    assert!(shown.ts.is_none());
    let presentation = view::presentation(&shown.stored.result, None);
    assert!(presentation.reliable());
    assert_eq!(
        view::irc_path_detail(&shown.stored.result, None, shown.stale).as_deref(),
        Some("The path stopped after 10 steps on the reactant side.")
    );
    let search = saved_search(&shown.stored.result).unwrap();
    assert_eq!(search.arrow, Some(arrow));
    assert!(search.context.is_some());
    assert_eq!(
        search.mapping.formed[0].each_ref().map(|atom| atom.number),
        [1, 6]
    );
    assert!(
        view::search_log(
            &shown.stored.result,
            None,
            crate::appearance::EnergyUnit::default()
        )
        .contains("NEB: 120 iterations · converged")
    );
    assert!(
        view::search_log(
            &shown.stored.result,
            None,
            crate::appearance::EnergyUnit::default()
        )
        .contains("Worker frequencies (cm⁻¹): -387.0")
    );
    send(&mut reopened, XtbAction::Ts(Action::Image(1)));
    assert_eq!(reopened.xtb_step(), Some(1));
    assert!(reopened.xtb_step_preview().is_none());
    send(
        &mut reopened,
        XtbAction::Ts(Action::Retry(arrow, Retry::FinerPath)),
    );
    assert!(
        reopened.ts_review_open(),
        "reruns always require a fresh review"
    );
    assert!(!reopened.xtb_running());
    send(
        &mut reopened,
        XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
    );
    assert!(!reopened.ts_review_open());
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

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn failed_irc_reports_failure_and_opens_the_saved_candidate() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    let failed = Arc::get_mut(&mut report).unwrap();
    failed.extras.connects = false;
    failed.result.status = Status::NotConverged {
        reason: "IRC failed: endpoint minimization reached its step limit".into(),
    };
    let energy = failed.result.energy_hartree;
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    assert_eq!(app.status, "GFN2-xTB · No reliable transition state found.");

    let ui = card::tests::ViewHarness::new().await;
    let texts = ui.inspect(app.xtb_reaction_card(arrow)).texts;
    assert!(texts.iter().any(|text| {
        text == "The reaction-path check could not finish: the endpoint optimization reached its step limit."
    }));
    assert!(texts.iter().any(|text| text == "Show what was found"));
    // Only the Calculate tick names the barrier; no barrier value is shown.
    assert!(
        texts
            .iter()
            .all(|text| !text.contains("ΔE‡") || text == "Barrier ΔE‡ (transition state)")
    );
    let action = ui
        .activate(app.xtb_reaction_card(arrow), "xtb-ts-card-path")
        .unwrap();
    let _ = app.update(action);
    let shown = app.xtb_shown().unwrap();
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);
    assert_eq!(shown.stored.result.energy_hartree, energy);
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn failure_card_primary_label_and_message_follow_the_reason() {
    let ui = card::tests::ViewHarness::new().await.at_width(330.);
    for (reason, modes, label) in [
        (view::IRC_RETURNS_TO_REACTANT, 1, "Check the atom map"),
        (
            "expected one imaginary frequency below -50 cm-1, found 2",
            2,
            "Refine from the highest point",
        ),
        (
            "IRC reaches the reactant and a different minimum",
            1,
            "Show where it leads",
        ),
        (
            "saddle refinement did not converge",
            1,
            "Try again with a finer path",
        ),
        (
            "IRC failed: endpoint step limit",
            1,
            "Try again with a finer path",
        ),
    ] {
        let (mut app, arrow) = start();
        finish_model(&mut app);
        let run = app.tab.xtb.run.as_ref().unwrap();
        let serial = run.serial;
        let mut report = example_report(run.launch.as_ref().unwrap());
        let failed = Arc::get_mut(&mut report).unwrap();
        if modes >= 2 {
            prepare_higher_order_candidate(failed);
        }
        if label == "Show where it leads" {
            failed.extras.reached_positions_angstrom = failed.frames[2].positions_angstrom.clone();
        }
        failed.extras.connects = false;
        failed.extras.num_imaginary = modes;
        failed.result.status = Status::NotConverged {
            reason: reason.into(),
        };
        send(
            &mut app,
            XtbAction::TsEvent(serial, Event::Done(Ok(report))),
        );
        let card = || app.xtb_reaction_card(arrow);
        let snapshot = ui.inspect(card());
        assert!(snapshot.texts.iter().any(|text| text == label), "{reason}");
        assert!(
            snapshot
                .texts
                .iter()
                .any(|text| text == "Show what was found"),
            "{reason}"
        );
        let action = ui.activate(card(), "xtb-ts-rerun").unwrap();
        let target = match (label, action) {
            ("Check the atom map", Message::Xtb(XtbAction::Ts(Action::ReviewMap(id))))
            | (
                "Refine from the highest point",
                Message::Xtb(XtbAction::Ts(Action::Retry(id, Retry::RefineHighest))),
            )
            | ("Show where it leads", Message::Xtb(XtbAction::Ts(Action::ShowWhere(id))))
            | (
                "Try again with a finer path",
                Message::Xtb(XtbAction::Ts(Action::Retry(id, Retry::FinerPath))),
            ) => id,
            (_, action) => panic!("{label} dispatched {action:?}"),
        };
        assert_eq!(target, arrow);
        assert!(matches!(
            ui.activate(card(), "xtb-ts-card-path"),
            Some(Message::Xtb(XtbAction::Ts(Action::Card(
                card::Action::ShowPath
            ))))
        ));
    }
}

#[test]
fn show_where_previews_the_reached_geometry_even_after_reopening() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    let failed = Arc::get_mut(&mut report).unwrap();
    failed.extras.connects = false;
    failed.result.status = Status::NotConverged {
        reason: "IRC reaches the reactant and a different minimum".into(),
    };
    let mut reached = failed.frames[2].positions_angstrom.clone();
    reached[0][2] += 0.25;
    failed.extras.reached_positions_angstrom = reached.clone();
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    let file = app.tab.doc.file_json().unwrap();
    let mut reopened = reaction_app(Document::from_native_file(&file).unwrap(), arrow);
    for app in [&mut app, &mut reopened] {
        let original = model(&app.tab.doc);
        send(app, XtbAction::Ts(Action::ShowWhere(arrow)));
        assert_eq!(app.tab.xtb.palette.view, results::View::Path);
        assert!(app.xtb_reached_endpoint_selected());
        assert_eq!(
            model(app.display_document())[0],
            reached[0].map(|x| x as f32)
        );
        assert_eq!(model(&app.tab.doc), original);
        send(app, XtbAction::Palette(palette::Action::StepBack));
        assert_eq!(model(app.display_document()), original);
    }
}

#[test]
fn show_where_without_an_available_endpoint_preserves_the_current_selection() {
    for stale in [false, true] {
        let (mut app, arrow) = start();
        finish_model(&mut app);
        let run = app.tab.xtb.run.as_ref().unwrap();
        let serial = run.serial;
        let mut report = example_report(run.launch.as_ref().unwrap());
        let failed = Arc::get_mut(&mut report).unwrap();
        if stale {
            failed.extras.reached_positions_angstrom = failed.frames[2].positions_angstrom.clone();
        }
        failed.extras.connects = false;
        failed.result.status = Status::NotConverged {
            reason: "IRC reaches the reactant and a different minimum".into(),
        };
        send(
            &mut app,
            XtbAction::TsEvent(serial, Event::Done(Ok(report))),
        );
        let file = app.tab.doc.file_json().unwrap();
        let mut reopened = reaction_app(Document::from_native_file(&file).unwrap(), arrow);
        for current in [&mut app, &mut reopened] {
            if stale {
                let before = current.tab.doc.clone();
                let product = current.tab.doc.reactions[0].products[0].atoms[0];
                current.tab.doc.atom_mut(product).unwrap().map_num = 0;
                current.changed(before);
            }
            assert!(current.xtb_show_reaction_path(arrow));
            send(current, XtbAction::Ts(Action::Image(1)));
            current.tab.xtb.palette.view = results::View::Log;
            current.tab.xtb.palette.collapsed = true;
            let selected = current.tab.selected.clone();
            let displayed = model(current.display_document());
            let result = Arc::clone(&current.xtb_shown().unwrap().stored.result);
            assert_eq!(view::has_reached_endpoint(&result), stale);
            assert_eq!(current.xtb_shown().unwrap().stale, stale);
            send(current, XtbAction::Ts(Action::ShowWhere(arrow)));
            assert_eq!(current.tab.selected, selected);
            assert_eq!(current.xtb_step(), Some(1));
            assert!(!current.xtb_reached_endpoint_selected());
            assert_eq!(current.tab.xtb.palette.view, results::View::Log);
            assert!(current.tab.xtb.palette.collapsed);
            assert_eq!(model(current.display_document()), displayed);
            assert!(Arc::ptr_eq(
                &current.xtb_shown().unwrap().stored.result,
                &result
            ));
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn unavailable_endpoint_disables_show_where_on_the_card_and_path() {
    let ui = card::tests::ViewHarness::new().await;
    for stale in [false, true] {
        let (mut app, arrow) = start();
        finish_model(&mut app);
        let run = app.tab.xtb.run.as_ref().unwrap();
        let serial = run.serial;
        let mut report = example_report(run.launch.as_ref().unwrap());
        let failed = Arc::get_mut(&mut report).unwrap();
        if stale {
            failed.extras.reached_positions_angstrom = failed.frames[2].positions_angstrom.clone();
        }
        failed.extras.connects = false;
        failed.result.status = Status::NotConverged {
            reason: "IRC reaches the reactant and a different minimum".into(),
        };
        send(
            &mut app,
            XtbAction::TsEvent(serial, Event::Done(Ok(report))),
        );
        let file = app.tab.doc.file_json().unwrap();
        let mut reopened = reaction_app(Document::from_native_file(&file).unwrap(), arrow);
        for current in [&mut app, &mut reopened] {
            if stale {
                let before = current.tab.doc.clone();
                let product = current.tab.doc.reactions[0].products[0].atoms[0];
                current.tab.doc.atom_mut(product).unwrap().map_num = 0;
                current.changed(before);
            }
            let snapshot = ui.inspect(current.xtb_reaction_card(arrow));
            let command = snapshot
                .semantics
                .nodes
                .iter()
                .find(|node| node.id == "xtb-ts-rerun")
                .unwrap();
            assert_eq!(command.name, "Show where it leads");
            assert!(!command.enabled);
            assert!(
                ui.activate(current.xtb_reaction_card(arrow), "xtb-ts-rerun")
                    .is_none()
            );
            assert!(current.xtb_show_reaction_path(arrow));
            let path = || current.xtb_bottom_palette().expect("open Path palette");
            let snapshot = ui.inspect(path());
            let command = snapshot
                .semantics
                .nodes
                .iter()
                .find(|node| node.id == "xtb-ts-rerun-20")
                .unwrap();
            assert_eq!(command.name, "Show where it leads");
            assert!(!command.enabled);
            assert!(ui.activate(path(), "xtb-ts-rerun-20").is_none());
        }
    }
}

#[test]
fn live_and_reopened_frequency_disagreement_keep_the_specific_reason_and_finer_recovery() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    let failed = Arc::get_mut(&mut report).unwrap();
    failed.extras.connects = false;
    failed.extras.worker_frequencies = vec![-387., -100.];
    let vibrations = failed.result.vibrations.as_mut().unwrap();
    vibrations.frequencies_cm1 = failed.extras.worker_frequencies.clone();
    vibrations.ir_intensities_km_mol = vec![12., 3.];
    failed.result.status = Status::NotConverged {
        reason: "engine/worker imaginary frequency counts disagree: engine 1, worker 2".into(),
    };
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    let file = app.tab.doc.file_json().unwrap();
    let mut reopened = reaction_app(Document::from_native_file(&file).unwrap(), arrow);
    for current in [&mut app, &mut reopened] {
        let card::ArrowResult::Found(result) = current.xtb_ts_card_result(arrow) else {
            panic!("missing result");
        };
        let session = current.tab.xtb.ts_sessions.get(result).map(AsRef::as_ref);
        let presentation = view::presentation(result, session);
        assert_eq!(presentation.refined_kcal, None);
        assert_eq!(presentation.recovery, view::Recovery::FinerPath);
        assert_eq!(
            presentation.reason.as_deref(),
            Some("The two frequency checks disagree (engine 1, worker 2).")
        );
        send(
            current,
            XtbAction::Ts(Action::Retry(arrow, Retry::RefineHighest)),
        );
        send(
            current,
            XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
        );
        assert_eq!(
            current.xtb_live_ts().unwrap().request.retry,
            Some(Retry::FinerPath)
        );
    }
}

#[test]
fn missing_or_incomplete_session_candidate_uses_a_finer_path() {
    for missing in ["candidate", "band", "positions", "hessian"] {
        let (mut app, arrow) = start();
        finish_model(&mut app);
        let run = app.tab.xtb.run.as_ref().unwrap();
        let serial = run.serial;
        let mut report = example_report(run.launch.as_ref().unwrap());
        let failed = Arc::get_mut(&mut report).unwrap();
        prepare_higher_order_candidate(failed);
        match missing {
            "candidate" => failed.extras.candidate = None,
            "band" => failed.extras.image_positions.clear(),
            "positions" => {
                failed.extras.candidate.as_mut().unwrap().positions.pop();
            }
            "hessian" => {
                failed.extras.candidate.as_mut().unwrap().hessian.pop();
            }
            _ => unreachable!(),
        }
        send(
            &mut app,
            XtbAction::TsEvent(serial, Event::Done(Ok(report))),
        );
        let card::ArrowResult::Found(result) = app.xtb_ts_card_result(arrow) else {
            panic!("missing result");
        };
        let session = app.tab.xtb.ts_sessions.get(result).map(AsRef::as_ref);
        assert_eq!(
            view::presentation(result, session).recovery,
            view::Recovery::FinerPath,
            "{missing}"
        );
        assert!(!app.xtb_ts_refinement_available(arrow), "{missing}");
        send(
            &mut app,
            XtbAction::Ts(Action::Retry(arrow, Retry::RefineHighest)),
        );
        send(
            &mut app,
            XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
        );
        assert_eq!(
            app.xtb_live_ts().unwrap().request.retry,
            Some(Retry::FinerPath),
            "{missing}"
        );
        assert!(app.xtb_live_ts().unwrap().energies.is_empty(), "{missing}");
    }
}

#[test]
fn changing_the_model_geometry_makes_the_saved_candidate_unavailable() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    prepare_higher_order_candidate(Arc::get_mut(&mut report).unwrap());
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    assert!(app.xtb_ts_refinement_available(arrow));
    app.tab.doc.models[0].atoms[0].xyz[2] += 0.25;
    assert!(!app.xtb_ts_refinement_available(arrow));
    send(
        &mut app,
        XtbAction::Ts(Action::Retry(arrow, Retry::RefineHighest)),
    );
    send(
        &mut app,
        XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
    );
    assert_eq!(
        app.xtb_live_ts().unwrap().request.retry,
        Some(Retry::FinerPath)
    );
}

#[test]
fn refine_highest_resumes_current_band_and_reopened_result_uses_a_finer_path() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    let failed = Arc::get_mut(&mut report).unwrap();
    prepare_higher_order_candidate(failed);
    let energies = failed
        .result
        .trajectory
        .as_ref()
        .unwrap()
        .energies_hartree
        .clone();
    let candidate = failed.extras.candidate.clone().unwrap();
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    let file = app.tab.doc.file_json().unwrap();
    let mut reopened = reaction_app(Document::from_native_file(&file).unwrap(), arrow);
    for (current, expected) in [
        (&app, view::Recovery::RefineHighest),
        (&reopened, view::Recovery::FinerPath),
    ] {
        let card::ArrowResult::Found(result) = current.xtb_ts_card_result(arrow) else {
            panic!("missing saved result");
        };
        let session = current.tab.xtb.ts_sessions.get(result).map(AsRef::as_ref);
        assert_eq!(view::presentation(result, session).recovery, expected);
    }
    for app in [&mut app, &mut reopened] {
        send(
            app,
            XtbAction::Ts(Action::Retry(arrow, Retry::RefineHighest)),
        );
        assert!(app.ts_review_open());
        send(
            app,
            XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
        );
    }
    assert_eq!(app.xtb_live_ts().unwrap().energies, energies);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let Some(job::SearchPath::Saved {
        candidate: resumed, ..
    }) = app.xtb_ts_retry_path(
        &run.ts.as_ref().unwrap().request,
        run.launch.as_ref().unwrap(),
    )
    else {
        panic!("current session should retain its failed candidate");
    };
    assert_eq!(resumed, candidate);
    let run = reopened.tab.xtb.run.as_ref().unwrap();
    assert_eq!(
        run.ts.as_ref().unwrap().request.retry,
        Some(Retry::FinerPath)
    );
    assert!(matches!(
        reopened.xtb_ts_retry_path(
            &run.ts.as_ref().unwrap().request,
            run.launch.as_ref().unwrap()
        ),
        Some(job::SearchPath::Finer(20))
    ));
}

#[test]
fn every_rerun_requires_review_even_after_an_earlier_review_was_accepted() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let report = example_report(run.launch.as_ref().unwrap());
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );

    for _ in 0..2 {
        send(
            &mut app,
            XtbAction::Ts(Action::Retry(arrow, Retry::FinerPath)),
        );
        assert!(app.ts_review_open());
        assert!(!app.xtb_running());
        send(
            &mut app,
            XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
        );
        assert!(!app.ts_review_open());
        let request = &app
            .tab
            .xtb
            .run
            .as_ref()
            .unwrap()
            .ts
            .as_ref()
            .unwrap()
            .request;
        assert_eq!(request.arrow, arrow);
        assert_eq!(request.images, 20);
        send(&mut app, XtbAction::Cancel);
    }
}

#[test]
fn reopened_endpoint_charge_makes_the_result_stale_and_requires_review() {
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
    assert_eq!(reopened.xtb_shown().unwrap().frame.charge, 0);

    let before = reopened.tab.doc.clone();
    let product = reopened.tab.doc.reactions[0].products[0]
        .atoms
        .iter()
        .copied()
        .find(|&id| reopened.tab.doc.atom(id).unwrap().element == "O")
        .unwrap();
    let atom = reopened.tab.doc.atom_mut(product).unwrap();
    atom.charge = 2;
    atom.no_implicit = true;
    reopened.changed(before);
    assert!(ts_plan::plan_for_review(&reopened.tab.doc, arrow).is_ok());
    assert!(ts_plan::plan(&reopened.tab.doc, arrow).is_err());
    assert!(reopened.xtb_shown().unwrap().stale);

    send(
        &mut reopened,
        XtbAction::Ts(Action::Retry(arrow, Retry::FinerPath)),
    );
    assert!(reopened.ts_review_open());
    assert!(!reopened.xtb_running());
    send(
        &mut reopened,
        XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
    );
    assert!(
        reopened.ts_review_open(),
        "the charge imbalance blocks Accept"
    );
    assert!(!reopened.xtb_running());
    send(
        &mut reopened,
        XtbAction::Ts(Action::ReviewSheet(review::Action::Charge(0, "2".into()))),
    );
    send(
        &mut reopened,
        XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
    );
    assert!(!reopened.ts_review_open());
    let run = reopened.tab.xtb.run.as_ref().unwrap();
    assert_eq!(run.config.charge, 2, "use the newly reviewed state");
    let request = &run.ts.as_ref().unwrap().request;
    assert_eq!(request.arrow, arrow);
    assert_eq!(request.images, 20);
}

#[test]
fn animate_dispatch_opens_the_imaginary_mode_and_previews_the_saddle_without_applying_it() {
    let (mut app, _) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    let report_data = Arc::get_mut(&mut report).unwrap();
    let axes = report_data.frames[0].numbers.len() * 3;
    let vibrations = report_data.result.vibrations.as_mut().unwrap();
    vibrations.frequencies_cm1.push(1200.);
    vibrations.ir_intensities_km_mol.push(5.);
    vibrations.modes = vec![0.; axes * 2];
    vibrations.modes[0] = 1.;
    vibrations.modes[axes + 1] = 1.;
    let saddle = report_data.frames[0].positions_angstrom.clone();
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    let result = Arc::clone(&app.xtb_shown().unwrap().stored.result);
    let before = app.tab.doc.clone();
    assert_ne!(
        model(&before),
        saddle
            .iter()
            .map(|p| p.map(|x| x as f32))
            .collect::<Vec<_>>()
    );

    for (inspector, action) in [
        (true, XtbAction::Ts(Action::Card(card::Action::Animate))),
        (
            false,
            XtbAction::Vibrations(super::super::vibrations::Action::AnimateImaginary),
        ),
    ] {
        send(&mut app, XtbAction::Ts(Action::Image(2)));
        assert_eq!(app.xtb_step(), Some(2));
        app.tab.xtb.vibrations.mode = 1;
        app.tab.xtb.vibrations.playing = false;
        app.tab.xtb.palette.open = false;
        app.tab.xtb.palette.collapsed = true;
        app.xtb.dock.inspector = inspector;
        app.inspector_open = false;
        app.inspector_tab = crate::app::InspectorTab::Reactions;
        send(&mut app, action);

        assert!(app.tab.xtb.palette.open);
        assert!(!app.tab.xtb.palette.collapsed);
        assert_eq!(app.tab.xtb.palette.view, results::View::Vibrations);
        assert!(Arc::ptr_eq(
            &app.xtb_shown().unwrap().stored.result,
            &result
        ));
        assert_eq!(app.tab.xtb.vibrations.mode, 0);
        assert!(app.tab.xtb.vibrations.playing);
        assert_eq!(app.xtb_step(), None, "Animate ends the path preview");
        if inspector {
            assert!(app.inspector_open);
            assert_eq!(app.inspector_tab, crate::app::InspectorTab::Predict);
            assert_eq!(app.xtb.page, super::super::Page::Xtb);
        }
        assert!(app.xtb_vibration_preview().is_some(), "{}", app.status);
        let equilibrium: Vec<_> = saddle.iter().map(|p| p.map(|x| x as f32)).collect();
        assert_eq!(model(app.display_document()), equilibrium);
        for _ in 0..2 {
            send(
                &mut app,
                XtbAction::Vibrations(super::super::vibrations::Action::Tick),
            );
        }
        assert_ne!(model(app.display_document()), equilibrium);
        assert_eq!(app.tab.doc, before, "Animate keeps the accepted drawing");

        send(
            &mut app,
            XtbAction::Vibrations(super::super::vibrations::Action::PlaceArrows),
        );
        assert_eq!(app.tab.doc, before, "arrows cannot apply the TS saddle");
        send(
            &mut app,
            XtbAction::Palette(palette::Action::View(results::View::Path)),
        );
        assert!(app.xtb_vibration_preview().is_none());
        assert_eq!(model(app.display_document()), model(&before));
    }
}

#[test]
fn saved_ts_without_modes_uses_the_vibrations_recompute_at_its_saved_input_geometry() {
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
    assert!(reopened.xtb_modes(&result).is_none());
    let before = reopened.tab.doc.clone();

    let vibrations = XtbAction::Palette(palette::Action::View(results::View::Vibrations));
    send(&mut reopened, vibrations);
    assert!(reopened.tab.xtb.vibrations.recomputing(&result));

    send(
        &mut reopened,
        XtbAction::Palette(palette::Action::View(results::View::Path)),
    );
    assert!(!reopened.tab.xtb.vibrations.recomputing(&result));
    reopened.tab.xtb.vibrations.mode = 4;
    reopened.tab.xtb.vibrations.playing = false;
    send(
        &mut reopened,
        XtbAction::Ts(Action::Card(card::Action::Animate)),
    );
    assert_eq!(reopened.tab.xtb.palette.view, results::View::Vibrations);
    assert_eq!(reopened.tab.xtb.vibrations.mode, 0);
    assert!(reopened.tab.xtb.vibrations.playing);
    assert!(reopened.tab.xtb.vibrations.recomputing(&result));
    assert_eq!(reopened.tab.doc, before);

    let saddle = reopened
        .xtb_shown()
        .unwrap()
        .frame
        .positions_angstrom
        .clone();
    let mut report = super::super::tests::optfreq(&saddle, &saddle);
    report.computed.vibrations.as_mut().unwrap().frequencies_cm1 = vec![-387.];
    report.computed.normal_modes = vec![0.; saddle.len() * 3];
    report.computed.normal_modes[0] = 1.;
    let epoch = reopened.tab.file_epoch;
    // The first request above was canceled by leaving Vibrations. Animate
    // scheduled the second request, whose completion supplies these modes.
    send(
        &mut reopened,
        XtbAction::Vibrations(super::super::vibrations::Action::Recomputed(
            epoch,
            2,
            Ok(Arc::new(report)),
        )),
    );
    assert!(!reopened.tab.xtb.vibrations.recomputing(&result));
    assert!(reopened.xtb_modes(&result).is_some());
    assert!(reopened.xtb_vibration_preview().is_some());
    let equilibrium = saddle
        .iter()
        .map(|p| p.map(|x| x as f32))
        .collect::<Vec<_>>();
    assert_eq!(model(reopened.display_document()), equilibrium);
    for _ in 0..2 {
        send(
            &mut reopened,
            XtbAction::Vibrations(super::super::vibrations::Action::Tick),
        );
    }
    assert_ne!(model(reopened.display_document()), equilibrium);
    assert_eq!(
        reopened.tab.doc, before,
        "recomputed animation keeps the drawing"
    );
}

#[test]
fn reopened_ts_animate_does_not_guess_between_arrows_sharing_the_starting_molecule() {
    let (mut app, arrow) = start();
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = example_report(run.launch.as_ref().unwrap());
    // Earlier files had no originating arrow recorded in the saved result.
    Arc::make_mut(&mut report).result.run = None;
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );

    let other = app.tab.doc.next_id();
    let mut other_arrow = app.tab.doc.arrows[0].clone();
    other_arrow.id = other;
    app.tab.doc.arrows.push(other_arrow);
    let mut reaction = app.tab.doc.reactions[0].clone();
    reaction.arrow = other;
    app.tab.doc.reactions.push(reaction);
    app.tab.doc.validate().unwrap();
    let file = app.tab.doc.file_json().unwrap();
    let shown = app.xtb_shown().unwrap();
    assert_eq!(matching_plans(&app.tab.doc, shown.atoms).count(), 2);
    assert!(unique_matching_plan(&app.tab.doc, shown.atoms).is_none());

    // The in-memory session still knows which arrow owns its result.
    send(&mut app, XtbAction::Ts(Action::Card(card::Action::Animate)));
    assert_eq!(app.tab.xtb.palette.view, results::View::Vibrations);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(results::View::Path)),
    );
    app.tab.xtb.palette.open = false;
    app.tab.selected = vec![other];
    send(&mut app, XtbAction::Ts(Action::Card(card::Action::Animate)));
    assert!(!app.tab.xtb.palette.open);
    assert_eq!(app.tab.xtb.palette.view, results::View::Path);

    let mut reopened = reaction_app(Document::from_native_file(&file).unwrap(), arrow);
    reopened.tab.selected = vec![reopened.tab.doc.reactions[0].reactants[0].atoms[0]];
    send(&mut reopened, XtbAction::Palette(palette::Action::Show));
    assert!(reopened.xtb_shown().unwrap().ts.is_none());
    assert!(reopened.xtb_shown().unwrap().arrow.is_none());
    assert!(reopened.xtb_shown().unwrap().stale);
    for candidate in [arrow, other] {
        reopened.tab.selected = vec![candidate];
        assert!(matches!(
            reopened.xtb_ts_card_result(candidate),
            card::ArrowResult::Ambiguous
        ));
        reopened.tab.xtb.palette.open = false;
        send(
            &mut reopened,
            XtbAction::Ts(Action::Card(card::Action::Animate)),
        );
        assert!(!reopened.tab.xtb.palette.open);
        assert_eq!(reopened.tab.xtb.palette.view, results::View::Path);
    }

    let before = reopened.tab.doc.clone();
    reopened
        .tab
        .doc
        .reactions
        .retain(|reaction| reaction.arrow != other);
    reopened.tab.doc.arrows.retain(|arrow| arrow.id != other);
    reopened.changed(before);
    reopened.tab.selected = vec![arrow];
    assert!(matches!(
        reopened.xtb_ts_card_result(arrow),
        card::ArrowResult::Found(_)
    ));
    send(
        &mut reopened,
        XtbAction::Ts(Action::Card(card::Action::Animate)),
    );
    assert!(
        reopened.tab.xtb.palette.open,
        "one matching arrow can show Animate"
    );
    assert_eq!(reopened.tab.xtb.palette.view, results::View::Vibrations);
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
        let card::ArrowResult::Found(result) = reopened.xtb_ts_card_result(arrow) else {
            panic!("the edited map must not orphan the saved result");
        };
        let search = saved_search(result).unwrap();
        assert_eq!(
            search.mapping.formed[0].each_ref().map(|atom| atom.number),
            [1, 6]
        );
        send(
            &mut reopened,
            XtbAction::Ts(Action::Retry(arrow, Retry::FinerPath)),
        );
        assert!(!reopened.xtb_running(), "a stale reaction cannot be rerun");
    }
}

#[test]
fn path_previews_update_imported_interleaved_hydrogens_in_frame_order() {
    let (mut doc, launch) = interleaved_imported_model();
    let original = doc.models[0].clone();
    let mut image = (*launch.frame).clone();
    for (i, xyz) in image.positions_angstrom.iter_mut().enumerate() {
        xyz[2] += (i + 1) as f64 * 0.1;
    }

    preview_image(&mut doc, &launch.atoms, &launch.frame, &image).unwrap();

    let model = &doc.models[0];
    assert_eq!(model.frame_positions(), f32s(&image.positions_angstrom));
    assert_eq!(model.frame_sources(), launch.atoms);
    assert_eq!(model.frame_atom_order, original.frame_atom_order);
    assert_eq!(
        model.hydrogens.iter().map(|h| h.parent).collect::<Vec<_>>(),
        original
            .hydrogens
            .iter()
            .map(|h| h.parent)
            .collect::<Vec<_>>(),
        "intermediate images retain each imported hydrogen's parent"
    );
    assert_eq!(
        model.atoms[0].xyz,
        image.positions_angstrom[1].map(|x| x as f32)
    );
    assert_eq!(
        model.atoms[1].xyz,
        image.positions_angstrom[2].map(|x| x as f32)
    );
    assert_eq!(
        model.hydrogens[0].xyz,
        image.positions_angstrom[0].map(|x| x as f32)
    );
}

fn interleaved_imported_model() -> (Document, Launch) {
    let frame = Frame {
        numbers: vec![1, 8, 6, 1, 1, 1],
        positions_angstrom: vec![
            [-0.96, 0., 0.],
            [0., 0., 0.],
            [1.42, 0., 0.],
            [1.8, 1., 0.],
            [1.8, -0.5, 0.866],
            [1.8, -0.5, -0.866],
        ],
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let (mut doc, _) = crate::app::frames_import::drawing(&frame).unwrap();
    crate::app::frames_import::fold_calculation_hydrogens(&mut doc, &frame);
    assert_eq!(doc.models[0].frame_atom_order, [2, 0, 1, 3, 4, 5]);
    assert_eq!(doc.models[0].hydrogens.len(), 4);
    let launch = reshiki::science::launch(&doc, doc.atoms[0].id, 0, 1).unwrap();
    assert_eq!(launch.frame.numbers, frame.numbers);
    assert_eq!(
        launch.atoms,
        [
            None,
            Some(doc.atoms[0].id),
            Some(doc.atoms[1].id),
            None,
            None,
            None
        ]
    );
    (doc, launch)
}

#[test]
fn endpoint_previews_align_imported_sources_elements_positions_and_bonds_in_frame_order() {
    let (mut doc, launch) = interleaved_imported_model();
    let mut image = (*launch.frame).clone();
    image.positions_angstrom[3] = [0., 0.95, 0.];

    for disconnected in [false, true] {
        if disconnected {
            image.positions_angstrom[3] = [100.; 3];
        }
        preview_endpoint_image(&mut doc, &launch.atoms, &launch.frame, &image).unwrap();

        let model = &doc.models[0];
        assert!(model.hydrogens.is_empty());
        assert!(model.frame_atom_order.is_empty());
        assert_eq!(model.frame_sources(), launch.atoms);
        assert_eq!(model.frame_positions(), f32s(&image.positions_angstrom));
        assert_eq!(
            model.points(&doc).unwrap(),
            ["H", "O", "C", "H", "H", "H"]
                .into_iter()
                .zip(f32s(&image.positions_angstrom))
                .collect::<Vec<_>>()
        );
        let geometry = model.generated.as_ref().unwrap();
        assert_eq!(geometry.sources, launch.atoms);
        let expected_bonds: Vec<_> =
            reshiki_optimize::ts::bonds(&image.numbers, &job::bohr(&image.positions_angstrom))
                .unwrap()
                .into_iter()
                .map(|(a, b)| [a, b])
                .collect();
        assert_eq!(geometry.bonds, expected_bonds);
        assert_eq!(geometry.bonds.contains(&[1, 3]), !disconnected);
        assert!(!geometry.bonds.contains(&[2, 3]));
        assert!(!model.is_stale(&doc));
    }
}

#[test]
fn endpoint_previews_perceive_transferred_and_disconnected_hydrogens_without_editing_the_model() {
    let (doc, arrow) = card::tests::hydrogen_transfer();
    let mut app = reaction_app(doc, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    assert!(app.ts_review_visible());
    send(
        &mut app,
        XtbAction::Ts(Action::ReviewSheet(review::Action::Accept)),
    );
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
    let mut image = (*launch.frame).clone();
    image.positions_angstrom[order.len()] = [moved.x, moved.y, moved.z];
    report_data.extras.image_positions[2].clone_from(&image.positions_angstrom);
    report_data.extras.reached_positions_angstrom = image.positions_angstrom.clone();
    let mut document = app.tab.doc.clone();
    assert!(run.generated.as_ref().unwrap().apply(&mut document));
    let before = document.models[0].hydrogens[0];
    assert_eq!(before.parent, donor as u64);
    assert!(matches!(
        reshiki::science::move_model(&mut document, &launch.atoms, &image),
        Err(ScienceError::ModelMismatch)
    ));
    preview_image(&mut document, &launch.atoms, &launch.frame, &image).unwrap();
    assert_eq!(
        document.models[0].hydrogens[0].parent, donor as u64,
        "intermediate NEB images keep their original topology"
    );
    let expected_bonds: Vec<_> =
        reshiki_optimize::ts::bonds(&image.numbers, &job::bohr(&image.positions_angstrom))
            .unwrap()
            .into_iter()
            .map(|(a, b)| [a, b])
            .collect();

    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    let accepted_model = app.tab.doc.models[0].clone();
    send(&mut app, XtbAction::Ts(Action::Image(2)));
    let preview = app
        .xtb_step_preview()
        .expect("the NEB image should preview");
    assert_eq!(preview.models[0].hydrogens[0].parent, donor as u64);

    send(&mut app, XtbAction::Ts(Action::ReachedEndpoint));
    let preview = app
        .xtb_step_preview()
        .expect("the reached endpoint should preview");
    assert_eq!(
        preview.models[0].atoms[order.len()].xyz,
        [moved.x as f32, moved.y as f32, moved.z as f32]
    );
    let geometry = preview.models[0].generated.as_ref().unwrap();
    assert_eq!(
        geometry.bonds, expected_bonds,
        "the engine's exact covalent graph"
    );
    assert!(geometry.bonds.contains(&[acceptor, order.len()]));
    assert!(!geometry.bonds.contains(&[donor, order.len()]));
    assert!(
        preview.models[0].hydrogens.is_empty(),
        "no forced H-parent sticks"
    );
    assert!(geometry.sources[order.len()..].iter().all(Option::is_none));

    assert!(!preview.models[0].is_stale(preview));

    let shown = app.xtb_shown().unwrap();
    let mut disconnected = image.clone();
    disconnected.positions_angstrom[order.len()] = [100., 100., 100.];
    let mut preview = app.tab.doc.clone();
    preview_endpoint_image(&mut preview, shown.atoms, shown.input, &disconnected).unwrap();
    let geometry = preview.models[0].generated.as_ref().unwrap();
    assert!(
        geometry
            .bonds
            .iter()
            .all(|pair| !pair.contains(&order.len()))
    );
    assert_eq!(preview.models[0].atoms[order.len()].xyz, [100.; 3]);
    assert_eq!(app.tab.doc.models[0].hydrogens[0], before);
    assert_eq!(app.tab.doc.models[0], accepted_model);
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
    app.tab.xtb.palette.collapsed = true;
    let finished = Message::Xtb(XtbAction::TsEvent(serial, Event::Done(Ok(report))));
    let _ = app.update(Message::Tab(first, Box::new(finished)));
    assert_eq!(app.tab.id, foreground);
    assert_eq!(app.xtb.page, crate::app::xtb::Page::Nmr);
    assert!(app.tab.xtb.palette.collapsed);
    assert!(app.tab.doc.science.results.is_empty());
    assert_eq!(
        app.in_tab(first, |app| app.tab.doc.science.results.len()),
        Some(1)
    );
}

#[test]
fn cancel_drops_generation_and_a_late_finish_cannot_attach() {
    let (mut app, arrow) = start();
    send(&mut app, XtbAction::Cancel);
    assert!(app.tab.model_job.is_none());
    assert!(!app.xtb_running());
    assert!(app.tab.doc.models.is_empty());
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    finish_model(&mut app);
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let report = example_report(run.launch.as_ref().unwrap());
    assert!(app.tab.doc.models.is_empty());
    send(&mut app, XtbAction::Cancel);
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(report))),
    );
    assert_eq!(app.status, "Calculation canceled");
    assert!(!app.xtb_running());
    assert!(app.tab.doc.science.results.is_empty());
    assert!(app.tab.doc.models.is_empty());
}

mod irc_profile;
mod placed_profile;
mod refine;
pub(in crate::app) use irc_profile::finished as finished_irc;
mod tab_playback;
