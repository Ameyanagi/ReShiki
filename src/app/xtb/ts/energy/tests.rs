use super::*;

#[cfg(not(target_arch = "wasm32"))]
fn result(energy: f64) -> (Frame, CalcResult) {
    let frame = Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: crate::app::xtb::tests::START.to_vec(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let result = crate::app::xtb::run::example_computed(energy, 1.)
        .to_calc_result(
            &frame,
            qcdata::Task::Optimize,
            &crate::app::xtb::run::example_engine(),
            None,
        )
        .unwrap();
    (frame, result)
}

fn species(product: bool, coefficient: u32) -> Species {
    Species {
        atoms: vec![1],
        coefficient,
        product,
        charge: 0,
        multiplicity: 1,
    }
}

#[test]
fn energy_task_choice_uses_the_calculate_fallback_until_a_choice_is_remembered() {
    let mut state = State::default();
    assert_eq!(state.task(CalcTask::OptFreq), CalcTask::OptFreq);
    assert_eq!(state.task(CalcTask::Optimize), CalcTask::Optimize);
    assert_eq!(state.task(CalcTask::Energy), CalcTask::Optimize);
    state.task = Some(CalcTask::Optimize);
    assert_eq!(state.task(CalcTask::OptFreq), CalcTask::Optimize);
    state.task = Some(CalcTask::OptFreq);
    assert_eq!(state.task(CalcTask::Energy), CalcTask::OptFreq);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn totals_use_products_minus_reactants_and_each_coefficient() {
    let (_, a) = result(-1.);
    let (_, b) = result(-0.75);
    let (_, c) = result(-1.4);
    let a_species = species(false, 2);
    let b_species = species(false, 1);
    let c_species = species(true, 2);
    let total = totals(
        [(&a_species, &a), (&b_species, &b), (&c_species, &c)],
        CalcTask::Optimize,
    )
    .unwrap();
    assert!((total.energy - (-0.05 * KCAL_PER_HARTREE)).abs() < 1e-10);
    assert_eq!(total.gibbs, None);
    assert!(total.line().starts_with("ΔE -31.38 kcal/mol"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn gibbs_includes_the_same_weighted_electronic_energies_and_thermal_corrections() {
    let (frame, mut a) = result(-1.);
    let (_, mut b) = result(-2.1);
    let thermo =
        crate::app::xtb::tests::optfreq(&frame.positions_angstrom, &frame.positions_angstrom)
            .computed
            .thermo
            .unwrap();
    a.thermo = Some(qcdata::Thermo {
        gibbs_correction_hartree: 0.02,
        ..thermo.clone()
    });
    b.thermo = Some(qcdata::Thermo {
        gibbs_correction_hartree: 0.03,
        ..thermo
    });
    let a_species = species(false, 2);
    let b_species = species(true, 1);
    let values = [(&a_species, &a), (&b_species, &b)];
    let total = totals(values, CalcTask::OptFreq).unwrap();
    assert!((total.energy - -0.1 * KCAL_PER_HARTREE).abs() < 1e-10);
    assert!((total.gibbs.unwrap() - -0.11 * KCAL_PER_HARTREE).abs() < 1e-10);
    assert!(total.line().contains("ΔG (298.15 K)"));
    b.thermo = None;
    assert!(totals([(&a_species, &a), (&b_species, &b)], CalcTask::OptFreq).is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn reuse_requires_matching_method_electronic_state_task_and_unconstrained_convergence() {
    let (frame, result) = result(-1.);
    let species = species(false, 1);
    assert!(matching(&result, &frame, &species, CalcTask::Optimize));
    assert!(!matching(&result, &frame, &species, CalcTask::OptFreq));
    assert!(!matching(
        &result,
        &Frame {
            charge: 1,
            ..frame.clone()
        },
        &species,
        CalcTask::Optimize
    ));
    assert!(!matching(
        &result,
        &Frame {
            multiplicity: 3,
            ..frame.clone()
        },
        &species,
        CalcTask::Optimize
    ));
    for change in 0..6 {
        let mut result = result.clone();
        let qcdata::Settings::Xtb(settings) = &mut result.settings else {
            unreachable!()
        };
        match change {
            0 => settings.method = qcdata::XtbMethod::GfnFf,
            1 => settings.fixed_atoms = vec![0],
            2 => settings.conventions = qcdata::XtbConventions::Xtb68,
            3 => settings.electronic_temperature_k = Some(1000.),
            4 => settings.convergence = Some(qcdata::Convergence::Tight),
            _ => {
                result.status = qcdata::Status::NotConverged {
                    reason: "step limit".into(),
                }
            }
        }
        assert!(!matching(&result, &frame, &species, CalcTask::Optimize));
    }
    let mut single_point = result;
    single_point.task = qcdata::Task::Energy;
    assert!(!matching(
        &single_point,
        &frame,
        &species,
        CalcTask::Optimize
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn optfreq_reuse_requires_thermochemistry_at_298_kelvin_and_one_atmosphere() {
    let (frame, _) = result(-1.);
    let report =
        crate::app::xtb::tests::optfreq(&frame.positions_angstrom, &frame.positions_angstrom);
    let mut result = report
        .computed
        .to_calc_result(
            &frame,
            qcdata::Task::Optimize,
            &crate::app::xtb::run::example_engine(),
            None,
        )
        .unwrap();
    let species = species(false, 1);
    assert!(matching(&result, &frame, &species, CalcTask::OptFreq));
    if let qcdata::Settings::Xtb(settings) = &mut result.settings {
        settings.convergence = Some(qcdata::Convergence::Tight);
    }
    assert!(matching(&result, &frame, &species, CalcTask::OptFreq));
    result.thermo.as_mut().unwrap().temperature_k = 350.;
    assert!(!matching(&result, &frame, &species, CalcTask::OptFreq));
    result.thermo.as_mut().unwrap().temperature_k = TEMPERATURE_K;
    result.thermo.as_mut().unwrap().pressure_pa = 1_000.;
    assert!(!matching(&result, &frame, &species, CalcTask::OptFreq));
    result.thermo.as_mut().unwrap().pressure_pa = 101_325.;
    result.vibrations = None;
    assert!(!matching(&result, &frame, &species, CalcTask::OptFreq));
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
mod execution {
    use super::*;
    use crate::app::xtb::tests::{MOVED, START, event, optimized, send};
    use reshiki::document::{Arrow, Point};
    use reshiki::geometry::Point3;
    use reshiki::reactions::{Participant, Reaction};

    fn reaction(app: &mut App, a: u64, b: u64) -> (u64, Vec<Species>) {
        let arrow = app.tab.doc.next_id();
        app.tab.doc.arrows.push(Arrow::new(
            arrow,
            Point::new(30., 0.),
            Point::new(70., 0.),
            Default::default(),
            Default::default(),
        ));
        let mut reaction = Reaction::new(arrow);
        reaction.reactants = vec![Participant {
            atoms: vec![a],
            coefficient: 1,
        }];
        reaction.products = vec![Participant {
            atoms: vec![b],
            coefficient: 1,
        }];
        app.tab.doc.reactions.push(reaction);
        let species = vec![
            Species {
                atoms: vec![a],
                ..species(false, 1)
            },
            Species {
                atoms: vec![b],
                ..species(true, 1)
            },
        ];
        app.tab.selected = vec![arrow];
        (arrow, species)
    }

    fn model(app: &mut App) {
        let _ = app.finish_model(
            START.iter().map(|&[x, y, z]| Point3 { x, y, z }).collect(),
            vec![0, 0],
        );
        assert!(
            app.tab.xtb.run.as_ref().unwrap().launch.is_some(),
            "{}",
            app.status
        );
    }

    #[test]
    fn every_species_uses_ordinary_optimization_storage_and_queue_advances() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        let _ = app.xtb_reaction_energy_begin(arrow, species.clone(), CalcTask::Optimize);
        assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, [a]);
        model(&mut app);
        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
        assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, [b]);
        model(&mut app);
        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 2, "{}", app.status);
        assert!(
            app.tab
                .doc
                .science
                .results
                .iter()
                .all(|stored| stored.result.task == qcdata::Task::Optimize)
        );
        assert!(app.tab.xtb.run.is_none());
        assert!(app.tab.xtb.energy_state.batch.is_none());
        assert!(
            app.status.contains("Reaction energy · ΔE +0.00 kcal/mol"),
            "{}",
            app.status
        );
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize);
        assert!(app.tab.xtb.run.is_none(), "both current results are reused");
        assert_eq!(app.tab.doc.science.results.len(), 2);
        assert_eq!(app.tab.xtb.energy_state.summaries[0].reused, 2);
    }

    #[test]
    fn a_current_result_is_reused_and_cancel_stops_remaining_species() {
        let (mut app, a) = crate::app::xtb::tests::water(CalcTask::Optimize);
        event(&mut app, optimized(&START, &MOVED));
        let original = app.tab.doc.science.results[0].result.clone();
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        assert!(reusable(&app.tab.doc, &species[0], CalcTask::Optimize).is_some());
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize);
        assert_eq!(app.tab.xtb.energy_state.batch.as_ref().unwrap().reused, 1);
        assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, [b]);
        send(&mut app, xtb::Action::Cancel);
        assert!(app.tab.xtb.run.is_none());
        assert!(app.tab.model_job.is_none());
        assert!(app.tab.xtb.energy_state.batch.is_none());
        assert_eq!(app.tab.doc.science.results.len(), 1);
        assert!(Arc::ptr_eq(
            &original,
            &app.tab.doc.science.results[0].result
        ));
        assert_eq!(app.status, "Reaction energy canceled");
    }

    #[test]
    fn stale_results_are_not_reused() {
        let (mut app, a) = crate::app::xtb::tests::water(CalcTask::Optimize);
        event(&mut app, optimized(&START, &MOVED));
        let species = Species {
            atoms: vec![a],
            ..species(false, 1)
        };
        assert!(reusable(&app.tab.doc, &species, CalcTask::Optimize).is_some());
        app.tab.doc.atom_mut(a).unwrap().isotope = 18;
        assert!(reusable(&app.tab.doc, &species, CalcTask::Optimize).is_none());
    }

    #[test]
    fn a_reaction_edit_stops_the_queue_after_keeping_the_finished_species_result() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize);
        model(&mut app);
        app.tab.doc.reactions[0].products[0].coefficient = 2;
        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 1);
        assert!(app.tab.xtb.run.is_none());
        assert!(app.tab.xtb.energy_state.batch.is_none());
        assert!(
            app.status.contains("the reaction changed"),
            "{}",
            app.status
        );
    }
}
