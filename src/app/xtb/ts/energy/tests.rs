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
    let mut result = crate::app::xtb::run::example_computed(energy, 1.)
        .to_calc_result(
            &frame,
            qcdata::Task::Optimize,
            &crate::app::xtb::run::example_engine(),
            None,
        )
        .unwrap();
    if let qcdata::Settings::Xtb(settings) = &mut result.settings {
        settings.convergence = Some(qcdata::Convergence::Normal);
    }
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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn scheme_reuse_accepts_tight_electronic_minima_but_checks_state_method_and_freshness() {
    let mut doc = Document::default();
    let atom = doc.add_atom("O", reshiki::document::Point::default());
    let (frame, mut result) = result(-1.);
    result.input_frame = Some(frame.hash());
    if let qcdata::Settings::Xtb(settings) = &mut result.settings {
        settings.convergence = Some(qcdata::Convergence::Tight);
    }
    let launch = science::Launch {
        frame: Arc::new(frame),
        atoms: vec![Some(atom), None, None],
        baseline: science::baseline(&doc, &[atom]),
    };
    science::attach(&mut doc, &launch, Vec::new(), result).unwrap();
    let method = qcdata::XtbMethod::Gfn2Xtb;
    assert!(scheme_reusable(&doc, &[atom], 0, 1, method, false).is_some());
    assert!(scheme_reusable(&doc, &[atom], 0, 1, method, true).is_none());
    assert!(scheme_reusable(&doc, &[atom], 1, 1, method, false).is_none());
    assert!(scheme_reusable(&doc, &[atom], 0, 3, method, false).is_none());
    assert!(scheme_reusable(&doc, &[atom], 0, 1, qcdata::XtbMethod::GXtb, false).is_none());
    doc.atom_mut(atom).unwrap().isotope = 18;
    assert!(scheme_reusable(&doc, &[atom], 0, 1, method, false).is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_frequency_request_finds_older_eligible_thermo_before_a_newer_electronic_result() {
    let mut doc = Document::default();
    let atom = doc.add_atom("O", reshiki::document::Point::default());
    let (frame, mut electronic) = result(-0.9);
    let input = frame.hash();
    electronic.input_frame = Some(input);
    let report =
        crate::app::xtb::tests::optfreq(&frame.positions_angstrom, &frame.positions_angstrom);
    let mut thermo = report
        .computed
        .to_calc_result(
            &frame,
            qcdata::Task::Optimize,
            &crate::app::xtb::run::example_engine(),
            None,
        )
        .unwrap();
    thermo.input_frame = Some(input);
    if let qcdata::Settings::Xtb(settings) = &mut thermo.settings {
        settings.convergence = Some(qcdata::Convergence::Tight);
    }
    let launch = science::Launch {
        frame: Arc::new(frame),
        atoms: vec![Some(atom), None, None],
        baseline: science::baseline(&doc, &[atom]),
    };
    science::attach(&mut doc, &launch, Vec::new(), thermo).unwrap();
    science::attach(&mut doc, &launch, Vec::new(), electronic).unwrap();
    let method = qcdata::XtbMethod::Gfn2Xtb;
    let (_, electronic, _) = scheme_reusable(&doc, &[atom], 0, 1, method, false).unwrap();
    assert_eq!(electronic.energy_hartree, Some(-0.9));
    assert!(electronic.thermo.is_none());
    let (_, thermo, _) = scheme_reusable(&doc, &[atom], 0, 1, method, true).unwrap();
    assert!(thermo.thermo.is_some());
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
        qcdata::XtbMethod::Gfn2Xtb,
    )
    .unwrap();
    assert!((total.energy - (-0.05 * KCAL_PER_HARTREE)).abs() < 1e-10);
    assert_eq!(total.gibbs, None);
    assert_eq!(total.method, qcdata::XtbMethod::Gfn2Xtb);
    assert_eq!(
        total.line(EnergyUnit::default()),
        "Reaction energy · ΔE −31.4 kcal/mol · GFN2-xTB"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn selected_method_is_required_for_reuse_and_all_total_terms() {
    let (frame, gfn2) = result(-1.);
    let (_, mut gxtb) = result(-0.9);
    let qcdata::Settings::Xtb(settings) = &mut gxtb.settings else {
        unreachable!()
    };
    settings.method = qcdata::XtbMethod::GXtb;
    let reactant = species(false, 1);
    let product = species(true, 1);
    assert!(!matching(
        &gfn2,
        &frame,
        &reactant,
        CalcTask::Optimize,
        qcdata::XtbMethod::GXtb
    ));
    assert!(!matching(
        &gxtb,
        &frame,
        &product,
        CalcTask::Optimize,
        qcdata::XtbMethod::Gfn2Xtb
    ));
    assert!(matching(
        &gxtb,
        &frame,
        &product,
        CalcTask::Optimize,
        qcdata::XtbMethod::GXtb
    ));
    assert!(
        totals(
            [(&reactant, &gfn2), (&product, &gxtb)],
            CalcTask::Optimize,
            qcdata::XtbMethod::GXtb
        )
        .is_none()
    );
    let total = totals(
        [(&reactant, &gxtb), (&product, &gxtb)],
        CalcTask::Optimize,
        qcdata::XtbMethod::GXtb,
    )
    .unwrap();
    assert_eq!(total.method, qcdata::XtbMethod::GXtb);
    assert!(total.line(EnergyUnit::default()).contains("g-xTB"));
    assert_eq!(total.energy, 0.);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn failed_or_unconverged_terms_never_produce_reaction_totals() {
    let (frame, mut failed) = result(-1.);
    let (_, product) = result(-0.9);
    let reactant_species = species(false, 1);
    let product_species = species(true, 1);
    for status in [
        qcdata::Status::Failed {
            reason: "SCF failure".into(),
        },
        qcdata::Status::NotConverged {
            reason: "step limit".into(),
        },
    ] {
        failed.status = status;
        assert!(!matching(
            &failed,
            &frame,
            &reactant_species,
            CalcTask::Optimize,
            qcdata::XtbMethod::Gfn2Xtb
        ));
        assert!(
            totals(
                [(&reactant_species, &failed), (&product_species, &product)],
                CalcTask::Optimize,
                qcdata::XtbMethod::Gfn2Xtb
            )
            .is_none()
        );
    }
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
    let total = totals(values, CalcTask::OptFreq, qcdata::XtbMethod::Gfn2Xtb).unwrap();
    assert!((total.energy - -0.1 * KCAL_PER_HARTREE).abs() < 1e-10);
    assert!((total.gibbs.unwrap() - -0.11 * KCAL_PER_HARTREE).abs() < 1e-10);
    b.thermo = None;
    assert!(
        totals(
            [(&a_species, &a), (&b_species, &b)],
            CalcTask::OptFreq,
            qcdata::XtbMethod::Gfn2Xtb
        )
        .is_none()
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn reuse_requires_matching_method_electronic_state_task_and_unconstrained_convergence() {
    let (frame, result) = result(-1.);
    let species = species(false, 1);
    assert!(matching(
        &result,
        &frame,
        &species,
        CalcTask::Optimize,
        qcdata::XtbMethod::Gfn2Xtb
    ));
    assert!(!matching(
        &result,
        &frame,
        &species,
        CalcTask::OptFreq,
        qcdata::XtbMethod::Gfn2Xtb
    ));
    assert!(!matching(
        &result,
        &Frame {
            charge: 1,
            ..frame.clone()
        },
        &species,
        CalcTask::Optimize,
        qcdata::XtbMethod::Gfn2Xtb
    ));
    assert!(!matching(
        &result,
        &Frame {
            multiplicity: 3,
            ..frame.clone()
        },
        &species,
        CalcTask::Optimize,
        qcdata::XtbMethod::Gfn2Xtb
    ));
    for change in 0..7 {
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
            5 => settings.convergence = None,
            _ => {
                result.status = qcdata::Status::NotConverged {
                    reason: "step limit".into(),
                }
            }
        }
        assert!(!matching(
            &result,
            &frame,
            &species,
            CalcTask::Optimize,
            qcdata::XtbMethod::Gfn2Xtb
        ));
    }
    let mut single_point = result;
    single_point.task = qcdata::Task::Energy;
    assert!(!matching(
        &single_point,
        &frame,
        &species,
        CalcTask::Optimize,
        qcdata::XtbMethod::Gfn2Xtb
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn optfreq_reuse_requires_all_thermochemistry_settings_and_known_tight_convergence() {
    let (mut frame, _) = result(-1.);
    // The engine detects C2v at its automatic tolerance even when the
    // isotope-aware geometry counter sees a slightly distorted structure.
    frame.positions_angstrom[1][0] += 0.01;
    assert_eq!(
        qcdata::vibrational::rotational_symmetry_number(
            &[8000, 1000, 1000],
            &frame.positions_angstrom,
            &[15.999, 1.008, 1.008],
        ),
        1
    );
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
    assert!(!matching(
        &result,
        &frame,
        &species,
        CalcTask::OptFreq,
        qcdata::XtbMethod::Gfn2Xtb
    ));
    if let qcdata::Settings::Xtb(settings) = &mut result.settings {
        settings.convergence = Some(qcdata::Convergence::Tight);
    }
    assert!(matching(
        &result,
        &frame,
        &species,
        CalcTask::OptFreq,
        qcdata::XtbMethod::Gfn2Xtb
    ));
    for change in 0..6 {
        let mut changed = result.clone();
        let thermo = changed.thermo.as_mut().unwrap();
        match change {
            0 => thermo.temperature_k = 350.,
            1 => thermo.pressure_pa = 1_000.,
            2 => thermo.frequency_scale = 0.95,
            3 => thermo.rotor_cutoff_cm1 = 100.,
            4 => thermo.imaginary_cutoff_cm1 = 10.,
            _ => thermo.symmetry_number = 1,
        }
        assert!(
            !matching(
                &changed,
                &frame,
                &species,
                CalcTask::OptFreq,
                qcdata::XtbMethod::Gfn2Xtb
            ),
            "thermochemistry override {change} must not be reused"
        );
    }
    let mut unlabelled = result.clone();
    unlabelled.thermo.as_mut().unwrap().point_group = None;
    assert!(!matching(
        &unlabelled,
        &frame,
        &species,
        CalcTask::OptFreq,
        qcdata::XtbMethod::Gfn2Xtb
    ));

    let mut isotopologue = result.clone();
    let mut labelled_frame = frame.clone();
    labelled_frame.isotopes = vec![0, 2, 0];
    isotopologue.vibrations.as_mut().unwrap().masses_amu =
        xtb::isotope_masses(&labelled_frame).unwrap();
    assert!(!matching(
        &isotopologue,
        &labelled_frame,
        &species,
        CalcTask::OptFreq,
        qcdata::XtbMethod::Gfn2Xtb
    ));
    isotopologue.thermo.as_mut().unwrap().symmetry_number = 1;
    assert!(matching(
        &isotopologue,
        &labelled_frame,
        &species,
        CalcTask::OptFreq,
        qcdata::XtbMethod::Gfn2Xtb
    ));

    result.vibrations = None;
    assert!(!matching(
        &result,
        &frame,
        &species,
        CalcTask::OptFreq,
        qcdata::XtbMethod::Gfn2Xtb
    ));
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
mod execution {
    use super::*;
    use crate::app::xtb::tests::{MOVED, START, event, optimized, send};
    use reshiki::document::{Arrow, Point};
    use reshiki::geometry::Point3;
    use reshiki::reactions::{Participant, Reaction};

    fn optimized_method(method: qcdata::XtbMethod) -> crate::app::xtb::run::Event {
        let mut answer = optimized(&START, &MOVED);
        let crate::app::xtb::run::Event::Done(Ok(report)) = &mut answer else {
            unreachable!()
        };
        Arc::get_mut(report).unwrap().computed.method =
            reshiki_xtb_jobs::run::worker_method(method);
        answer
    }

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

    fn saved_model(app: &mut App, atom: u64, offset: Point) {
        use reshiki::models3d::{Display, ForceField, Hydrogen, Model3d, Placed};

        let mut model = Model3d::new(
            &app.tab.doc,
            vec![Placed {
                id: atom,
                xyz: START[0].map(|v| v as f32),
            }],
            START[1..]
                .iter()
                .map(|xyz| Hydrogen {
                    parent: atom,
                    xyz: xyz.map(|v| v as f32),
                })
                .collect(),
            ForceField::default(),
        );
        model.display = Display::Both;
        model.offset = offset;
        app.tab.doc.models.push(model);
        reshiki::models3d::require_version(&mut app.tab.doc);
    }

    fn summary_totals(app: &App) -> Option<Totals> {
        let summary = app.tab.xtb.energy_state.summaries.first()?;
        input_totals(
            &app.tab.doc,
            &summary.species,
            &summary.inputs,
            summary.task,
            summary.method,
        )
    }

    #[test]
    fn every_species_uses_ordinary_optimization_storage_and_queue_advances() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 50.));
        let (arrow, species) = reaction(&mut app, a, b);
        let bounds = reshiki::scene::selections_bounds(&app.tab.doc, &[vec![a], vec![b]]);
        let _ = app.xtb_reaction_energy_begin(arrow, species.clone(), CalcTask::Optimize, None);
        assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, [a]);
        model(&mut app);
        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
        assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, [b]);
        model(&mut app);
        assert_eq!(app.tab.doc.models.len(), 1, "B is staged until its result");
        event(&mut app, optimized(&START, &MOVED));
        let doc = &app.tab.doc;
        let frames: Vec<_> = doc
            .models
            .iter()
            .map(|model| reshiki::models3d::frame(doc, model).unwrap())
            .collect();
        let [(a_lo, a_hi), (b_lo, b_hi)] = frames[..] else {
            panic!("one model per species");
        };
        for ((lo, hi), (species_lo, species_hi)) in frames.iter().zip(bounds.into_iter().flatten())
        {
            assert!(((lo.x + hi.x - species_lo.x - species_hi.x) / 2.).abs() < 0.01);
            assert!(lo.y >= species_hi.y + doc.drawing_style.bond_length_world - 0.01);
        }
        assert!((a_lo.y - b_lo.y).abs() < 0.01);
        assert!(a_hi.x <= b_lo.x + 0.001 && b_lo.x < b_hi.x && a_lo.y < a_hi.y);
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
            app.status.contains("Reaction energy · ΔE 0.0 kcal/mol"),
            "{}",
            app.status
        );
        let _ = app.xtb_reaction_energy_begin(arrow, species.clone(), CalcTask::Optimize, None);
        assert!(app.tab.xtb.run.is_none(), "both current results are reused");
        assert_eq!(app.tab.doc.science.results.len(), 2);
        assert_eq!(app.tab.xtb.energy_state.summaries[0].reused, 2);
        let before = app.tab.doc.clone();
        send(
            &mut app,
            xtb::Action::Ts(xtb::ts::Action::PlaceEnergyProfile(Some(arrow))),
        );
        let diagram = app
            .tab
            .doc
            .graphics
            .last()
            .unwrap()
            .energy_profile
            .as_ref()
            .unwrap();
        use reshiki::graphics::EnergyProfileStateKind::{Product, Reactant};
        assert_eq!(
            [diagram.states[0].kind, diagram.states[1].kind],
            [Reactant, Product]
        );
        assert_eq!(diagram.states.len(), 2);
        assert!(diagram.states.iter().all(|s| s.geometry.is_some()));
        assert!(diagram.steps[0].transition.is_none() && diagram.steps[0].irc.is_empty());
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);

        send(
            &mut app,
            xtb::Action::Ts(xtb::ts::Action::PlaceEnergyProfile(Some(arrow))),
        );
        app.xtb.settings.barrier_method = qcdata::XtbMethod::GXtb;
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        event(&mut app, optimized_method(qcdata::XtbMethod::GXtb));
        let before_last = app.tab.doc.clone();
        event(&mut app, optimized_method(qcdata::XtbMethod::GXtb));
        let diagram = app
            .tab
            .doc
            .graphics
            .last()
            .unwrap()
            .energy_profile
            .as_ref()
            .unwrap();
        assert_eq!(diagram.method, "g-xTB");
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before_last);
    }

    #[test]
    fn queued_method_is_captured_and_survives_a_settings_change() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        app.xtb.settings.barrier_method = qcdata::XtbMethod::GXtb;
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        assert_eq!(
            app.tab.xtb.run.as_ref().unwrap().config.method,
            qcdata::XtbMethod::GXtb
        );
        app.xtb.settings.barrier_method = qcdata::XtbMethod::Gfn2Xtb;
        model(&mut app);
        event(&mut app, optimized_method(qcdata::XtbMethod::GXtb));
        assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, [b]);
        assert_eq!(
            app.tab.xtb.run.as_ref().unwrap().config.method,
            qcdata::XtbMethod::GXtb
        );
        model(&mut app);
        event(&mut app, optimized_method(qcdata::XtbMethod::GXtb));
        assert!(app.tab.xtb.energy_state.batch.is_none(), "{}", app.status);
        assert_eq!(
            summary_totals(&app).unwrap().method,
            qcdata::XtbMethod::GXtb
        );
        assert!(app.tab.doc.science.results.iter().all(|stored|
            matches!(&stored.result.settings, qcdata::Settings::Xtb(settings) if settings.method == qcdata::XtbMethod::GXtb)
        ));
    }

    #[test]
    fn a_gfn2_cached_result_is_not_reused_for_a_gxtb_queue() {
        let (mut app, a) = crate::app::xtb::tests::water(CalcTask::Optimize);
        event(&mut app, optimized(&START, &MOVED));
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        assert!(
            reusable(
                &app.tab.doc,
                &species[0],
                CalcTask::Optimize,
                qcdata::XtbMethod::Gfn2Xtb
            )
            .is_some()
        );
        assert!(
            reusable(
                &app.tab.doc,
                &species[0],
                CalcTask::Optimize,
                qcdata::XtbMethod::GXtb
            )
            .is_none()
        );
        app.xtb.settings.barrier_method = qcdata::XtbMethod::GXtb;
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        assert_eq!(app.tab.xtb.energy_state.batch.as_ref().unwrap().reused, 0);
        assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, [a]);
        assert_eq!(
            app.tab.xtb.run.as_ref().unwrap().config.method,
            qcdata::XtbMethod::GXtb
        );
        send(&mut app, xtb::Action::Cancel);
    }

    #[test]
    fn a_failed_target_method_optimization_stops_without_a_summary() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        app.xtb.settings.barrier_method = qcdata::XtbMethod::GXtb;
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        model(&mut app);
        let mut answer = optimized_method(qcdata::XtbMethod::GXtb);
        let crate::app::xtb::run::Event::Done(Ok(report)) = &mut answer else {
            unreachable!()
        };
        Arc::get_mut(report).unwrap().converged = false;
        event(&mut app, answer);
        assert!(app.tab.xtb.run.is_none());
        assert!(app.tab.xtb.energy_state.batch.is_none());
        assert!(app.xtb_energy_result(arrow, None).0.is_none());
        assert!(app.tab.xtb.energy_state.summaries.is_empty());
        assert!(app.status.contains("did not converge"), "{}", app.status);
    }

    #[test]
    fn optimization_relocates_existing_models_in_its_own_undo_step() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 50.));
        let (arrow, species) = reaction(&mut app, a, b);
        use reshiki::models3d::{ForceField, Hydrogen, Model3d, Placed, frame};

        for (id, offset) in [(a, Point::new(70., 40.)), (b, Point::new(100., 0.))] {
            let mut model = Model3d::new(
                &app.tab.doc,
                vec![Placed {
                    id,
                    xyz: START[0].map(|v| v as f32),
                }],
                START[1..]
                    .iter()
                    .map(|xyz| Hydrogen {
                        parent: id,
                        xyz: xyz.map(|v| v as f32),
                    })
                    .collect(),
                ForceField::default(),
            );
            model.offset = offset;
            app.tab.doc.models.push(model);
        }
        reshiki::models3d::require_version(&mut app.tab.doc);
        let original = app.tab.doc.models.clone();

        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        assert!(app.tab.model_job.is_none(), "{}", app.status);
        assert!(!app.tab.history.can_undo());
        event(
            &mut app,
            crate::app::xtb::run::Event::Steps {
                first: 0,
                energies: vec![-5.06],
                positions: MOVED.to_vec(),
            },
        );
        assert_eq!(app.tab.doc.models, original);
        let preview = app.display_document();
        assert_eq!(preview.models[0].atoms[0].xyz, MOVED[0].map(|v| v as f32));
        assert_eq!(preview.models[1], original[1]);
        assert!(!app.tab.history.can_undo(), "live steps create no history");
        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
        let laid_out = app.tab.doc.models.clone();
        assert_ne!(laid_out[0].offset, original[0].offset);
        assert_ne!(laid_out[1].offset, original[1].offset);
        assert!(
            (frame(&app.tab.doc, &laid_out[0]).unwrap().0.y
                - frame(&app.tab.doc, &laid_out[1]).unwrap().0.y)
                .abs()
                < 0.01
        );
        let _ = app.update(Message::Undo);
        assert!(app.tab.doc.science.results.is_empty());
        assert_eq!(app.tab.doc.models, original);
        assert!(!app.tab.history.can_undo());

        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc.science.results.len(), 1);
        assert_eq!(app.tab.doc.models, laid_out);
    }

    #[test]
    fn a_missing_model_and_existing_row_moves_commit_with_the_result() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 50.));
        let (arrow, species) = reaction(&mut app, a, b);
        saved_model(&mut app, b, Point::new(70., 40.));
        let original = app.tab.doc.clone();

        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        assert!(app.tab.model_job.is_some(), "{}", app.status);
        model(&mut app);
        assert_eq!(app.tab.doc, original, "generation leaves the drawing alone");
        assert!(
            !app.tab.history.can_undo(),
            "generation is not a separate edit"
        );

        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
        assert_eq!(app.tab.doc.models.len(), 2);
        assert_ne!(app.tab.doc.models[0].offset, original.models[0].offset);
        let frames: Vec<_> = app
            .tab
            .doc
            .models
            .iter()
            .map(|model| reshiki::models3d::frame(&app.tab.doc, model).unwrap())
            .collect();
        assert!((frames[0].0.y - frames[1].0.y).abs() < 0.01);
        let committed = app.tab.doc.clone();

        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, original, "one Undo removes A and restores B");
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, committed);
    }

    #[test]
    fn a_stale_model_replacement_and_existing_row_moves_commit_with_the_result() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 50.));
        let (arrow, species) = reaction(&mut app, a, b);
        saved_model(&mut app, a, Point::new(70., 40.));
        saved_model(&mut app, b, Point::new(100., 0.));
        app.tab.doc.atom_mut(a).unwrap().isotope = 18;
        assert!(app.tab.doc.models[0].is_stale(&app.tab.doc));
        let original = app.tab.doc.clone();

        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        assert!(app.tab.model_job.is_some(), "{}", app.status);
        model(&mut app);
        assert_eq!(app.tab.doc, original, "regeneration is staged in memory");
        assert!(!app.tab.history.can_undo());

        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 1, "{}", app.status);
        assert_eq!(app.tab.doc.models.len(), 2);
        assert!(!app.tab.doc.models[0].is_stale(&app.tab.doc));
        assert_ne!(app.tab.doc.models[0].offset, original.models[0].offset);
        assert_ne!(app.tab.doc.models[1].offset, original.models[1].offset);
        let committed = app.tab.doc.clone();

        let _ = app.update(Message::Undo);
        assert_eq!(
            app.tab.doc, original,
            "one Undo restores the stale model and B"
        );
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, committed);
    }

    #[test]
    fn an_unrelated_annotation_allows_result_placement_and_survives_its_undo() {
        let (mut app, a) = crate::app::xtb::tests::water(CalcTask::Optimize);
        event(&mut app, optimized(&START, &MOVED));
        let reused = Arc::clone(&app.tab.doc.science.results[0].result);
        let b = app.tab.doc.add_atom("O", Point::new(100., 50.));
        let (arrow, species) = reaction(&mut app, a, b);
        app.tab.doc.models[0].offset = Point::new(70., 40.);
        saved_model(&mut app, b, Point::new(100., 0.));
        let reviewed = snapshot(&app.tab.doc, arrow, &species).unwrap();

        let _ = app.xtb_reaction_energy_begin(arrow, species.clone(), CalcTask::Optimize, None);
        assert_eq!(app.tab.xtb.energy_state.batch.as_ref().unwrap().reused, 1);
        assert_eq!(app.tab.xtb.run.as_ref().unwrap().atoms, [b]);
        let started_revision = app.tab.revision;
        let before_edit = app.tab.doc.clone();
        let note = app.tab.doc.next_id();
        app.tab.doc.annotations.push(reshiki::document::Annotation {
            id: note,
            position: Point::new(1000., 1000.),
            text: "Independent note".into(),
            format: Default::default(),
            angle: 0.,
        });
        app.changed(before_edit);
        assert_ne!(app.tab.revision, started_revision);
        assert_eq!(snapshot(&app.tab.doc, arrow, &species).unwrap(), reviewed);
        let before_result = app.tab.doc.clone();

        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 2, "{}", app.status);
        assert!(app.tab.xtb.run.is_none());
        assert!(app.tab.xtb.energy_state.batch.is_none(), "{}", app.status);
        assert!(summary_totals(&app).is_some());
        assert!(Arc::ptr_eq(&app.tab.doc.science.results[0].result, &reused));
        assert_eq!(
            app.tab.doc.models[1].atoms[0].xyz,
            MOVED[0].map(|v| v as f32)
        );
        assert_ne!(app.tab.doc.models[0].offset, before_result.models[0].offset);
        assert_ne!(app.tab.doc.models[1].offset, before_result.models[1].offset);

        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before_result);
        assert!(
            app.tab
                .doc
                .annotations
                .iter()
                .any(|annotation| annotation.id == note)
        );
        let _ = app.update(Message::Undo);
        assert!(
            !app.tab
                .doc
                .annotations
                .iter()
                .any(|annotation| annotation.id == note)
        );
    }

    #[test]
    fn a_current_result_is_reused_and_cancel_stops_remaining_species() {
        let (mut app, a) = crate::app::xtb::tests::water(CalcTask::Optimize);
        event(&mut app, optimized(&START, &MOVED));
        let original = app.tab.doc.science.results[0].result.clone();
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        assert!(
            reusable(
                &app.tab.doc,
                &species[0],
                CalcTask::Optimize,
                qcdata::XtbMethod::Gfn2Xtb
            )
            .is_some()
        );
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
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
        assert!(
            reusable(
                &app.tab.doc,
                &species,
                CalcTask::Optimize,
                qcdata::XtbMethod::Gfn2Xtb
            )
            .is_some()
        );
        app.tab.doc.atom_mut(a).unwrap().isotope = 18;
        assert!(
            reusable(
                &app.tab.doc,
                &species,
                CalcTask::Optimize,
                qcdata::XtbMethod::Gfn2Xtb
            )
            .is_none()
        );
    }

    #[test]
    fn a_queued_species_geometry_stereo_edit_stops_before_its_launch() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let water = app.tab.doc.add_atom("O", Point::default());
        let alkene = [
            Point::new(100., 0.),
            Point::new(140., 0.),
            Point::new(80., -28.),
            Point::new(160., -28.),
        ]
        .map(|position| app.tab.doc.add_atom("C", position));
        app.tab.doc.add_bond(alkene[0], alkene[1], 2, "plain");
        app.tab.doc.add_bond(alkene[0], alkene[2], 1, "plain");
        app.tab.doc.add_bond(alkene[1], alkene[3], 1, "plain");
        let (arrow, mut species) = reaction(&mut app, water, alkene[0]);
        species[1].atoms = alkene.to_vec();
        app.tab.doc.reactions[0].products[0].atoms = alkene.to_vec();
        let _ = app.xtb_reaction_energy_begin(arrow, species.clone(), CalcTask::Optimize, None);
        model(&mut app);
        let reviewed = snapshot(&app.tab.doc, arrow, &species).unwrap();
        // Stored stereo fields stay unchanged; E/Z is perceived from drawing geometry.
        let stereo: Vec<_> = app
            .tab
            .doc
            .bonds
            .iter()
            .map(|bond| bond.stereo.clone())
            .collect();
        app.tab.doc.atom_mut(alkene[3]).unwrap().position.y = 28.;
        assert_eq!(
            app.tab
                .doc
                .bonds
                .iter()
                .map(|bond| bond.stereo.clone())
                .collect::<Vec<_>>(),
            stereo
        );
        assert_ne!(snapshot(&app.tab.doc, arrow, &species).unwrap(), reviewed);
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

    #[test]
    fn completed_energy_keeps_its_exact_results_and_expires_if_an_input_is_stale_or_removed() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        let _ = app.xtb_reaction_energy_begin(arrow, species.clone(), CalcTask::Optimize, None);
        model(&mut app);
        event(&mut app, optimized(&START, &MOVED));
        model(&mut app);
        event(&mut app, optimized(&START, &MOVED));
        let _ = app.xtb_reaction_energy_begin(arrow, species.clone(), CalcTask::Optimize, None);
        assert_eq!(app.tab.xtb.energy_state.summaries[0].reused, 2);
        let original_total = summary_totals(&app).unwrap();
        assert!(
            app.status
                .contains(&original_total.line(EnergyUnit::default())),
            "{}",
            app.status
        );
        let original = Arc::clone(&app.tab.xtb.energy_state.summaries[0].inputs[1].result);

        // An ordinary optimization starts from a different conformer with the same settings.
        app.tab.doc.models[1].atoms[0].xyz[2] += 0.1;
        app.tab.selected = vec![b];
        app.xtb.settings.task = CalcTask::Optimize;
        send(&mut app, xtb::Action::Run);
        let start = app
            .tab
            .xtb
            .run
            .as_ref()
            .unwrap()
            .launch
            .as_ref()
            .unwrap()
            .frame
            .positions_angstrom
            .clone();
        let mut end = MOVED;
        end[1][2] += 0.05;
        let mut answer = optimized(&start, &end);
        let crate::app::xtb::run::Event::Done(Ok(report)) = &mut answer else {
            unreachable!()
        };
        let report = Arc::get_mut(report).unwrap();
        report.energy -= 0.1;
        report.computed.energy = report.energy;
        *report.energies.last_mut().unwrap() = report.energy;
        event(&mut app, answer);
        assert_eq!(app.tab.doc.science.results.len(), 3, "{}", app.status);
        assert!(Arc::ptr_eq(
            &reusable(
                &app.tab.doc,
                &species[1],
                CalcTask::Optimize,
                qcdata::XtbMethod::Gfn2Xtb
            )
            .unwrap()
            .result,
            &app.tab.doc.science.results[2].result,
        ));
        assert_eq!(summary_totals(&app), Some(original_total));
        assert_eq!(app.tab.xtb.energy_state.summaries[0].reused, 2);
        assert!(Arc::ptr_eq(
            &app.tab.xtb.energy_state.summaries[0].inputs[1].result,
            &original,
        ));

        app.tab.doc.atom_mut(b).unwrap().isotope = 18;
        assert!(summary_totals(&app).is_none());
        app.tab.doc.atom_mut(b).unwrap().isotope = 0;
        assert!(summary_totals(&app).is_some());
        app.tab
            .doc
            .science
            .results
            .retain(|stored| !Arc::ptr_eq(&stored.result, &original));
        assert!(
            reusable(
                &app.tab.doc,
                &species[1],
                CalcTask::Optimize,
                qcdata::XtbMethod::Gfn2Xtb
            )
            .is_some()
        );
        assert!(
            summary_totals(&app).is_none(),
            "a newer result cannot replace a missing input"
        );
    }

    #[tokio::test]
    #[ignore = "Opt-in renderer check: needs a graphics adapter"]
    async fn completed_energy_follows_light_copies_and_keeps_its_card_summary() {
        use crate::app::prompt::{Answer, Prompt};

        let ui = crate::app::xtb::ts::card::tests::ViewHarness::new().await;
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        model(&mut app);
        app.tab.xtb.run.as_mut().unwrap().keep_full = true;
        event(&mut app, optimized(&START, &MOVED));
        model(&mut app);
        app.tab.xtb.run.as_mut().unwrap().keep_full = true;
        let mut answer = optimized(&START, &MOVED);
        let crate::app::xtb::run::Event::Done(Ok(report)) = &mut answer else {
            unreachable!()
        };
        let report = Arc::get_mut(report).unwrap();
        report.energy -= 0.1;
        report.computed.energy = report.energy;
        *report.energies.last_mut().unwrap() = report.energy;
        event(&mut app, answer);
        assert!(app.tab.xtb.energy_state.batch.is_none());
        let expected = summary_totals(&app).unwrap();
        assert!((expected.energy + 0.1 * KCAL_PER_HARTREE).abs() < 1e-10);
        let shown = EnergyUnit::default().number(expected.energy);
        assert!(
            ui.inspect(app.xtb_reaction_card(arrow))
                .texts
                .contains(&shown)
        );
        let full: Vec<_> = app
            .tab
            .doc
            .science
            .results
            .iter()
            .map(|stored| Arc::clone(&stored.result))
            .collect();
        assert!(
            full.iter()
                .all(|result| !result.gradient_hartree_per_bohr.is_empty())
        );
        let prompt = Prompt::LargeResults {
            tab: app.tab.id,
            epoch: app.tab.file_epoch,
            bytes: app.tab.doc.science.file_bytes(),
            lighter: Some((2, 0)),
            then: Box::new(Message::Cancel),
        };
        let _ = app.xtb_size_answer(prompt, Answer::Save);
        for (index, stored) in app.tab.doc.science.results.iter().enumerate() {
            assert!(!stored.keep_full);
            assert!(!Arc::ptr_eq(&stored.result, &full[index]));
            assert!(stored.result.gradient_hartree_per_bohr.is_empty());
            assert!(Arc::ptr_eq(
                &app.tab.xtb.energy_state.summaries[0].inputs[index].result,
                &stored.result,
            ));
        }
        assert_eq!(summary_totals(&app), Some(expected));
        assert!(
            ui.inspect(app.xtb_reaction_card(arrow))
                .texts
                .contains(&shown)
        );
        for action in [Message::Undo, Message::Redo] {
            let _ = app.update(action);
            assert_eq!(
                EnergyUnit::default().number(summary_totals(&app).unwrap().energy),
                shown
            );
            assert!(
                ui.inspect(app.xtb_reaction_card(arrow))
                    .texts
                    .contains(&shown)
            );
        }
    }

    #[test]
    fn a_reaction_edit_stops_the_queue_after_keeping_the_finished_species_result() {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("O", Point::default());
        let b = app.tab.doc.add_atom("O", Point::new(100., 0.));
        let (arrow, species) = reaction(&mut app, a, b);
        saved_model(&mut app, b, Point::new(70., 40.));
        let original_b = app.tab.doc.models[0].clone();
        let _ = app.xtb_reaction_energy_begin(arrow, species, CalcTask::Optimize, None);
        model(&mut app);
        app.tab.doc.reactions[0].products[0].coefficient = 2;
        event(&mut app, optimized(&START, &MOVED));
        assert_eq!(app.tab.doc.science.results.len(), 1);
        assert_eq!(
            app.tab.doc.models[0], original_b,
            "a changed reaction is not placed"
        );
        assert!(app.tab.xtb.run.is_none());
        assert!(app.tab.xtb.energy_state.batch.is_none());
        assert!(
            app.status.contains("the reaction changed"),
            "{}",
            app.status
        );
    }
}
