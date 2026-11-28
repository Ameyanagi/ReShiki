use super::*;
use reshiki::document::{Arrow, Point};
use reshiki::models3d::{self, Hydrogen, Model3d, Placed};
use reshiki::reactions::{Participant, Reaction, ts_plan::Side};

fn drawing() -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let atoms: Vec<_> = [0., 240., 480.]
        .into_iter()
        .map(|x| doc.add_atom("C", Point::new(x, 0.)))
        .collect();
    let mut arrows = Vec::new();
    for index in 0..2 {
        let arrow = doc.next_id();
        doc.arrows.push(Arrow::new(
            arrow,
            Point::new(80. + index as f32 * 240., 0.),
            Point::new(160. + index as f32 * 240., 0.),
            Default::default(),
            Default::default(),
        ));
        let mut reaction = Reaction::new(arrow);
        reaction.reactants = vec![Participant {
            atoms: vec![atoms[index]],
            coefficient: 1,
        }];
        reaction.products = vec![Participant {
            atoms: vec![atoms[index + 1]],
            coefficient: 1,
        }];
        doc.reactions.push(reaction);
        arrows.push(arrow);
    }
    (doc, arrows)
}

fn plan() -> Plan {
    let (doc, arrows) = drawing();
    Plan::read(&doc, &arrows).unwrap()
}

fn input() -> TsInput {
    TsInput {
        document: Document::default(),
        plan: TsPlan {
            start: Side::Reactant,
            start_atoms: vec![1],
            mapped: vec![],
            mapped_hydrogens: vec![],
            form: vec![],
            break_: vec![],
            hydrogen_moves: vec![],
            shape: (1, 1),
            drawn_charges: (0, 0),
            atoms: 1,
        },
        images: 10,
        charge: 0,
        multiplicity: 1,
        automatic: false,
    }
}

fn request() -> Request {
    let plan = plan();
    Request {
        options: vec![StepOptions::default(); plan.steps.len()],
        inputs: vec![Ok(Some(input())); plan.steps.len()],
        plan,
        method: XtbMethod::Gfn2Xtb,
        threads: 1,
        retry: None,
        cache: Vec::new(),
        serial: 1,
    }
}

fn distinct_request() -> Request {
    let mut request = request();
    let representative = request.plan.species[0].clone();
    request.plan.species = (0..3)
        .map(|index| {
            let mut species = representative.clone();
            species.key = format!("fake species {index}");
            species
        })
        .collect();
    for (index, step) in request.plan.steps.iter_mut().enumerate() {
        step.reactants = vec![super::super::super::plan::Term {
            species: index,
            coefficient: 1,
        }];
        step.products = vec![super::super::super::plan::Term {
            species: index + 1,
            coefficient: 1,
        }];
    }
    request
}

fn species_value(index: usize, method: XtbMethod) -> SpeciesValue {
    let frame = Arc::new(Frame {
        numbers: vec![6],
        positions_angstrom: vec![[0.; 3]],
        isotopes: Vec::new(),
        masses_amu: Vec::new(),
        charge: 0,
        multiplicity: 1,
        drawing_atoms: Vec::new(),
    });
    let mut computed = xtb::run::example_computed(-1., 1.);
    computed.method = driver::worker_method(method);
    let mut result = computed
        .to_calc_result(
            &frame,
            qcdata::Task::Optimize,
            &xtb::run::example_engine(),
            None,
        )
        .unwrap();
    result.status = qcdata::Status::Converged;
    SpeciesValue {
        species: index,
        method,
        frame,
        result: Arc::new(result),
        launch: None,
    }
}

fn thermo_value(index: usize, energy: f64, correction: f64) -> SpeciesValue {
    let frame = Arc::new(Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: xtb::tests::START.to_vec(),
        isotopes: Vec::new(),
        masses_amu: Vec::new(),
        charge: 0,
        multiplicity: 1,
        drawing_atoms: Vec::new(),
    });
    let report = xtb::tests::optfreq(&frame.positions_angstrom, &frame.positions_angstrom);
    let mut result = report
        .computed
        .to_calc_result(
            &frame,
            qcdata::Task::Optimize,
            &xtb::run::example_engine(),
            None,
        )
        .unwrap();
    result.energy_hartree = Some(energy);
    result.status = qcdata::Status::Converged;
    let thermo = result.thermo.as_mut().unwrap();
    thermo.gibbs_correction_hartree = correction;
    thermo.enthalpy_correction_hartree =
        correction + thermo.temperature_k * thermo.entropy_hartree_per_k;
    assert!(transition_thermo(&result, &frame));
    SpeciesValue {
        species: index,
        method: XtbMethod::Gfn2Xtb,
        frame,
        result: Arc::new(result),
        launch: None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Job {
    Species(usize),
    Transition(u64),
}

#[derive(Clone, Copy)]
enum MockOptimization {
    InvertedCentre,
    IncompleteFrame,
}

#[derive(Default)]
struct Fake {
    species: Vec<(usize, bool)>,
    transitions: Vec<u64>,
    fail_ts: BTreeSet<u64>,
    frequency_warning: bool,
    cancel_after_species: bool,
    cancel_species: Option<usize>,
    cancel_hessian: Option<usize>,
    optimized_starts: usize,
    order: Vec<Job>,
    thermo: bool,
    mock_optimization: Option<(usize, MockOptimization)>,
}

impl Backend for Fake {
    fn species(
        &mut self,
        job: SpeciesJob<'_>,
        cancel: &Cancel,
        minimum: &mut dyn FnMut(&SpeciesValue),
    ) -> Result<SpeciesComputed, SpeciesFailure> {
        self.species.push((job.index, job.frequencies));
        self.order.push(Job::Species(job.index));
        if let Some((index, mock)) = self.mock_optimization
            && job.index == index
        {
            let native = Native {
                executable: PathBuf::from("/this/path/must/not/be/executed"),
                threads: 1,
            };
            return native.compute_species(job, cancel, minimum, |input| {
                let mut frame = input.clone();
                if matches!(mock, MockOptimization::InvertedCentre) {
                    for xyz in &mut frame.positions_angstrom {
                        xyz[0] = -xyz[0];
                    }
                }
                let result = xtb::run::example_computed(-1., 1.)
                    .to_calc_result(
                        &frame,
                        qcdata::Task::Optimize,
                        &xtb::run::example_engine(),
                        None,
                    )
                    .unwrap();
                if matches!(mock, MockOptimization::IncompleteFrame) {
                    frame.positions_angstrom.pop();
                }
                Ok((Arc::new(frame), Arc::new(result)))
            });
        }
        let SpeciesJob {
            index,
            method,
            frequencies,
            cached,
            ..
        } = job;
        if frequencies && self.cancel_hessian == Some(index) {
            minimum(&cached.unwrap_or_else(|| species_value(index, method)));
            cancel.cancel();
            return Err(SpeciesFailure::Calculation(
                "Scheme calculation canceled".into(),
            ));
        }
        if self.cancel_after_species || self.cancel_species == Some(index) {
            cancel.cancel();
            return Err(SpeciesFailure::Calculation(
                "Scheme calculation canceled".into(),
            ));
        }
        Ok(SpeciesComputed {
            value: cached.unwrap_or_else(|| {
                if self.thermo && frequencies {
                    thermo_value(index, -1., 0.01 * (index + 1) as f64)
                } else {
                    species_value(index, method)
                }
            }),
            warning: (self.frequency_warning && frequencies).then(|| "Fake Hessian failure".into()),
        })
    }
    fn transition(
        &mut self,
        job: TransitionJob<'_>,
        _: &Cancel,
        progress: &mut dyn FnMut(job::Event),
    ) -> Result<TransitionComputed, String> {
        let TransitionJob {
            optimized,
            arrow,
            method,
            free,
            ..
        } = job;
        self.transitions.push(arrow);
        self.order.push(Job::Transition(arrow));
        self.optimized_starts += usize::from(optimized.is_some());
        progress(job::Event::Stage(Stage::Saddle));
        if self.fail_ts.contains(&arrow) {
            return Err("Fake TS worker failure".into());
        }
        let value = if self.thermo && free {
            thermo_value(0, -0.9, 0.05)
        } else {
            species_value(0, method)
        };
        let mut result = (*value.result).clone();
        result.task = qcdata::Task::TransitionState;
        result.energy_hartree = Some(-0.9);
        Ok(TransitionComputed {
            result: Arc::new(result),
            frames: vec![Arc::clone(&value.frame)],
            launch: Launch {
                frame: value.frame,
                atoms: vec![Some(1)],
                baseline: "fake".into(),
            },
        })
    }
}

fn step_values(events: &[EventKind]) -> Vec<(usize, &StepValue)> {
    events
        .iter()
        .filter_map(|event| match event {
            EventKind::Step { index, value } => Some((*index, value)),
            _ => None,
        })
        .collect()
}

#[test]
fn d2_defaults_are_electronic_reaction_energy_and_barrier_only() {
    let options = StepOptions::default();
    assert!(options.reaction_energy && options.barrier);
    assert!(!options.free_energy && !options.free_barrier && !options.frequencies());
}

#[test]
fn all_distinct_minima_finish_before_the_first_ts_job() {
    let mut request = request();
    request.plan = Plan::example().1;
    let mut fake = Fake::default();
    calculate(&request, &mut fake, &Cancel::default(), |_| {}).unwrap();
    let species = request.plan.species.len();
    assert!(species > 2);
    assert_eq!(fake.species.len(), species);
    assert!(
        fake.order[..species]
            .iter()
            .all(|job| matches!(job, Job::Species(_)))
    );
    assert!(
        fake.order[species..]
            .iter()
            .all(|job| matches!(job, Job::Transition(_)))
    );
}

#[test]
fn endpoint_only_steps_complete_before_an_earlier_arrow_starts_its_ts() {
    let mut request = request();
    request.plan = Plan::example().1;
    request.options[1].barrier = false;
    let mut fake = Fake::default();
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    let finished_endpoint = events
        .iter()
        .position(|event| matches!(event, EventKind::Step { index: 1, .. }))
        .unwrap();
    let first_ts = events
        .iter()
        .position(|event| matches!(event, EventKind::Progress(Progress { stage: Some(_), .. })))
        .unwrap();
    assert!(finished_endpoint < first_ts);
    assert_eq!(
        step_values(&events)
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        [1, 0]
    );
}

#[test]
fn a_free_barrier_only_requests_frequencies_for_its_separate_reactants() {
    let mut request = request();
    request.plan = Plan::example().1;
    request.options = vec![
        StepOptions {
            reaction_energy: false,
            barrier: false,
            free_energy: false,
            free_barrier: false
        };
        2
    ];
    request.options[1].barrier = true;
    request.options[1].free_barrier = true;
    let step = &request.plan.steps[1];
    assert!(
        step.reactants.len() > 1,
        "fixture is an association with separate reactants"
    );
    let expected_reactants: BTreeSet<_> = step.reactants.iter().map(|term| term.species).collect();
    let mut fake = Fake {
        thermo: true,
        ..Default::default()
    };
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    let frequencies: BTreeSet<_> = fake
        .species
        .iter()
        .filter_map(|&(index, frequency)| frequency.then_some(index))
        .collect();
    assert_eq!(frequencies, expected_reactants);
    let value = step_values(&events)[0].1;
    let electronic: f64 = step
        .reactants
        .iter()
        .map(|term| -f64::from(term.coefficient))
        .sum();
    let reactant_g: f64 = step
        .reactants
        .iter()
        .map(|term| f64::from(term.coefficient) * (-1. + 0.01 * (term.species + 1) as f64))
        .sum();
    assert!((value.barrier.unwrap() - (-0.9 - electronic) * qcdata::KCAL_PER_HARTREE).abs() < 1e-8);
    assert!(
        (value.free_barrier.unwrap() - (-0.9 + 0.05 - reactant_g) * qcdata::KCAL_PER_HARTREE).abs()
            < 1e-8
    );
    assert!(value.error.is_none());
    assert!(value.gibbs.is_none());
}

#[test]
fn one_shared_species_job_serves_both_steps_and_coefficients_weight_the_sum() {
    let mut request = request();
    assert_eq!(
        request.plan.species.len(),
        1,
        "identical methane drawings share one calculation"
    );
    request.plan.steps[0].products[0].coefficient = 2;
    let mut fake = Fake::default();
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    assert_eq!(fake.species.len(), 1);
    assert_eq!(fake.transitions, request.plan.arrows);
    let values = step_values(&events);
    assert_eq!(values.len(), 2);
    assert!((values[0].1.energy.unwrap() + qcdata::KCAL_PER_HARTREE).abs() < 1e-8);
    assert_eq!(values[1].1.energy, Some(0.));
    assert!(
        values
            .iter()
            .all(|(_, value)| value.barrier.is_some() && value.error.is_none())
    );
}

#[test]
fn failed_ts_keeps_its_endpoint_energy_and_the_next_step_runs() {
    let request = request();
    let mut fake = Fake::default();
    fake.fail_ts.insert(request.plan.steps[0].arrow);
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    let values = step_values(&events);
    assert_eq!(values[0].1.energy, Some(0.));
    assert!(values[0].1.barrier.is_none());
    assert!(
        values[0]
            .1
            .error
            .as_ref()
            .unwrap()
            .contains("Fake TS worker failure")
    );
    assert!(values[1].1.barrier.is_some());
    assert_eq!(fake.transitions.len(), 2);
}

fn chiral_drawing() -> Document {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    let fluorine = doc.add_atom("F", Point::new(0., -28.));
    let chlorine = doc.add_atom("Cl", Point::new(-28., 14.));
    let bromine = doc.add_atom("Br", Point::new(28., 14.));
    doc.add_bond(carbon, fluorine, 1, "wedge");
    doc.add_bond(carbon, chlorine, 1, "plain");
    doc.add_bond(carbon, bromine, 1, "plain");
    let atoms = [carbon, fluorine, chlorine, bromine]
        .into_iter()
        .zip([[0., 0., 0.], [0., 1., 0.8], [-1., -0.5, 0.], [1., -0.5, 0.]])
        .map(|(id, xyz)| Placed { id, xyz })
        .collect();
    let hydrogens = vec![Hydrogen {
        parent: carbon,
        xyz: [0., 0., -1.],
    }];
    doc.models.push(Model3d::new(
        &doc,
        atoms,
        hydrogens,
        models3d::ForceField::Mmff94s,
    ));
    models3d::require_version(&mut doc);
    doc
}

#[test]
fn rejected_species_stereo_fails_all_dependent_steps_and_keeps_the_queue_running() {
    let mut request = distinct_request();
    request.plan.species[1].calculation = chiral_drawing();
    request.plan.species[1].label = "2".into();
    request.options[0].free_energy = true;
    let mut independent = request.plan.steps[1].clone();
    independent.arrow += 1;
    independent.reactants[0].species = 0;
    let independent_arrow = independent.arrow;
    request.plan.steps.push(independent);
    request.options.push(StepOptions::default());
    request.inputs.push(Ok(Some(input())));

    for (mock, expected) in [
        (
            MockOptimization::InvertedCentre,
            "2: Optimization changed the configuration at C1 (inverted).",
        ),
        (
            MockOptimization::IncompleteFrame,
            "2: The stereo check could not run:",
        ),
    ] {
        let mut fake = Fake {
            mock_optimization: Some((1, mock)),
            thermo: true,
            ..Default::default()
        };
        let mut state = State::default();
        state.serial = request.serial;
        state.running = true;
        state.steps = vec![StepStatus::Pending; 3];
        state.report.steps = vec![None; 3];
        calculate(&request, &mut fake, &Cancel::default(), |kind| {
            assert!(state.accept(Event {
                serial: request.serial,
                kind,
            }));
        })
        .unwrap();
        assert_eq!(fake.species, [(0, true), (1, true), (2, false)]);
        assert_eq!(fake.transitions, [independent_arrow]);
        assert_eq!(state.cache.len(), 2);
        assert!(state.report.species(1).is_none());
        assert!(
            state
                .cache
                .iter()
                .all(|v| v.key != request.plan.species[1].key)
        );
        for index in 0..2 {
            assert!(
                matches!(&state.steps[index], StepStatus::Failed(reason) if reason.starts_with(expected))
            );
            let value = state.step(index).unwrap();
            assert!(value.energy.is_none() && value.gibbs.is_none() && value.barrier.is_none());
        }
        assert_eq!(state.steps[2], StepStatus::Done);
        assert_eq!(state.step(2).unwrap().energy, Some(0.));
        assert!(state.step(2).unwrap().barrier.is_some());
    }
}

#[test]
fn frequency_failure_only_affects_steps_that_request_free_energy() {
    let mut request = request();
    request.options[0].barrier = false;
    request.options[1].barrier = false;
    request.options[1].free_energy = true;
    let mut fake = Fake {
        frequency_warning: true,
        ..Default::default()
    };
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    assert_eq!(
        fake.species,
        [(0, true)],
        "one frequency request upgrades the shared species once"
    );
    let values = step_values(&events);
    assert_eq!(values[0].1.energy, Some(0.));
    assert!(values[0].1.error.is_none());
    assert_eq!(values[1].1.energy, Some(0.));
    assert!(values[1].1.gibbs.is_none());
    assert!(
        values[1]
            .1
            .error
            .as_ref()
            .unwrap()
            .contains("Fake Hessian failure")
    );
}

#[test]
fn retry_only_runs_the_requested_ts_and_reuses_shared_species() {
    let mut request = request();
    let value = species_value(0, request.method);
    request.cache.push(Cached {
        key: request.plan.species[0].key.clone(),
        method: request.method,
        frame: value.frame,
        result: value.result,
        launch: value.launch,
    });
    request.retry = Some(1);
    let mut fake = Fake::default();
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    assert!(fake.species.is_empty());
    assert_eq!(fake.transitions, [request.plan.steps[1].arrow]);
    assert_eq!(
        step_values(&events)
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        [1]
    );
}

#[test]
fn retry_keeps_the_exact_minima_used_by_retained_rows() {
    let mut request = distinct_request();
    let originals: Vec<_> = (0..3)
        .map(|index| {
            let mut value = species_value(index, request.method);
            Arc::make_mut(&mut value.result).energy_hartree = Some(-1. - index as f64);
            value
        })
        .collect();
    let original = &originals[1];
    let mut newer = (*original).clone();
    Arc::make_mut(&mut newer.result).energy_hartree = Some(-2.01);
    let mut state = State::default();
    state.report.species = originals.clone();
    let mut retained = StepValue::empty(request.plan.steps[0].arrow, request.method);
    retained.energy = Some(-qcdata::KCAL_PER_HARTREE);
    state.report.steps = vec![Some(retained), None];
    state.cache_value(
        request.plan.species[1].key.clone(),
        request.method,
        newer.frame,
        newer.result,
        None,
    );
    request.retry = Some(1);
    request.cache = state.request_cache(&request.plan, request.method, true);
    assert!(
        request
            .cache
            .iter()
            .any(|value| Arc::ptr_eq(&value.result, &original.result))
    );
    let mut fake = Fake::default();
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    assert!(fake.species.is_empty());
    let reused = events
        .iter()
        .find_map(|event| match event {
            EventKind::Species { value, .. } if value.species == 1 => Some(value),
            _ => None,
        })
        .unwrap();
    assert!(Arc::ptr_eq(&reused.result, &original.result));
    let overall =
        state.step(0).unwrap().energy.unwrap() + step_values(&events)[0].1.energy.unwrap();
    assert!((overall + 2. * qcdata::KCAL_PER_HARTREE).abs() < 1e-8);
}

#[test]
fn cache_seeding_finds_an_eligible_result_on_a_later_drawn_copy() {
    let (mut doc, arrows) = drawing();
    // Isolated oxygen drawings are water, for which the existing minimum
    // fixture supplies a valid atom order and ordinary-cache eligibility.
    for atom in &mut doc.atoms {
        atom.element = "O".into();
    }
    let plan = Plan::read(&doc, &arrows).unwrap();
    assert_eq!(plan.species.len(), 1);
    let species = &plan.species[0];
    let second = species.instances[1][0];
    let value = thermo_value(0, -1., 0.01);
    let mut result = (*value.result).clone();
    if let qcdata::Settings::Xtb(settings) = &mut result.settings {
        settings.convergence = Some(qcdata::Convergence::Tight);
    }
    let launch = Launch {
        frame: value.frame,
        atoms: vec![Some(second), None, None],
        baseline: science::baseline(&doc, &[second]),
    };
    science::attach(&mut doc, &launch, vec![(*launch.frame).clone()], result).unwrap();
    let mut state = State::default();
    state.seed(
        &doc,
        &plan,
        XtbMethod::Gfn2Xtb,
        &[StepOptions::default(); 2],
    );
    assert_eq!(state.cache.len(), 1);
    assert_eq!(state.cache[0].launch.as_ref().unwrap().atoms, launch.atoms);
    let mut request = request();
    request.plan = plan;
    request.cache = std::mem::take(&mut state.cache);
    let mut fake = Fake::default();
    calculate(&request, &mut fake, &Cancel::default(), |_| {}).unwrap();
    assert!(
        fake.species.is_empty(),
        "the eligible duplicate must be reused"
    );
}

#[test]
fn endpoint_results_survive_cancel_during_an_unrelated_minimum_with_or_without_a_barrier() {
    for barrier in [false, true] {
        let mut request = distinct_request();
        for options in &mut request.options {
            options.barrier = barrier;
        }
        let mut fake = Fake {
            cancel_species: Some(2),
            ..Default::default()
        };
        let mut events = Vec::new();
        assert!(
            calculate(&request, &mut fake, &Cancel::default(), |event| events
                .push(event))
            .is_err()
        );
        let published: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                EventKind::Step { index, .. } | EventKind::Endpoints { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(published, [0]);
        let mut state = State::default();
        state.serial = request.serial;
        state.running = true;
        state.steps = vec![StepStatus::Pending; 2];
        state.report.steps = vec![None; 2];
        for kind in events {
            assert!(state.accept(Event {
                serial: request.serial,
                kind
            }));
        }
        state.cancel();
        assert_eq!(
            state.steps[0],
            if barrier {
                StepStatus::Cancelled
            } else {
                StepStatus::Done
            }
        );
        assert_eq!(state.steps[1], StepStatus::Cancelled);
        assert_eq!(state.step(0).unwrap().energy, Some(0.));
        assert!(state.step(0).unwrap().barrier.is_none());
    }
}

#[test]
fn electronic_row_is_published_before_the_shared_products_hessian() {
    for free_energy in [false, true] {
        let mut request = distinct_request();
        request.options[0].barrier = false;
        request.options[0].free_energy = free_energy;
        request.options[1].reaction_energy = false;
        request.options[1].barrier = false;
        request.options[1].free_energy = true;
        let mut fake = Fake {
            cancel_hessian: Some(1),
            thermo: true,
            ..Default::default()
        };
        let cancel = Cancel::default();
        let mut state = State::default();
        state.serial = request.serial;
        state.running = true;
        state.steps = vec![StepStatus::Pending; 2];
        state.report.steps = vec![None; 2];
        assert!(
            calculate(&request, &mut fake, &cancel, |kind| {
                assert!(
                    !cancel.is_cancelled(),
                    "publish before starting the Hessian"
                );
                assert!(state.accept(Event {
                    serial: request.serial,
                    kind,
                }));
            })
            .is_err()
        );
        state.cancel();
        assert_eq!(fake.species, [(0, free_energy), (1, true)]);
        assert_eq!(state.report.species.len(), 2);
        assert_eq!(state.step(0).unwrap().energy, Some(0.));
        assert!(state.step(0).unwrap().gibbs.is_none());
        assert_eq!(
            state.steps[0],
            if free_energy {
                StepStatus::Cancelled
            } else {
                StepStatus::Done
            }
        );
        assert_eq!(state.steps[1], StepStatus::Cancelled);
        assert!(state.step(1).is_none());
    }
}

#[test]
fn a_balancing_species_is_visited_before_a_later_steps_unrelated_minimum() {
    let mut request = distinct_request();
    // Species 2 represents a detached balancing product discovered after all
    // drawn species; it still belongs to the first row's dependency set.
    request.plan.steps[0].products[0].species = 2;
    request.plan.steps[1].reactants[0].species = 2;
    request.plan.steps[1].products[0].species = 1;
    let mut fake = Fake::default();
    calculate(&request, &mut fake, &Cancel::default(), |_| {}).unwrap();
    assert_eq!(
        fake.species
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        [0, 2, 1]
    );
}

#[test]
fn cache_entries_never_cross_electronic_methods() {
    let mut request = request();
    let value = species_value(0, request.method);
    request.cache.push(Cached {
        key: request.plan.species[0].key.clone(),
        method: request.method,
        frame: value.frame,
        result: value.result,
        launch: value.launch,
    });
    request.method = XtbMethod::GXtb;
    let mut fake = Fake::default();
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    assert_eq!(fake.species.len(), 1);
    assert!(
        step_values(&events)
            .iter()
            .all(|(_, value)| value.method == XtbMethod::GXtb)
    );
}

#[test]
fn a_proven_frame_correspondence_reuses_the_shared_optimized_ts_start() {
    let mut request = request();
    let species = &request.plan.species[0];
    let atoms: Vec<_> = species
        .calculation
        .atoms
        .iter()
        .map(|atom| atom.id)
        .collect();
    let mut value = species_value(0, request.method);
    value.launch = Some(Launch {
        frame: Arc::clone(&value.frame),
        atoms: atoms.iter().copied().map(Some).collect(),
        baseline: science::baseline(&species.calculation, &atoms),
    });
    for reviewed in &mut request.inputs {
        let reviewed = reviewed.as_mut().unwrap().as_mut().unwrap();
        reviewed.document = species.calculation.clone();
        reviewed.plan.start_atoms = atoms.clone();
    }
    request.cache.push(Cached {
        key: species.key.clone(),
        method: request.method,
        frame: value.frame,
        result: value.result,
        launch: value.launch,
    });
    let mut fake = Fake::default();
    calculate(&request, &mut fake, &Cancel::default(), |_| {}).unwrap();
    assert!(fake.species.is_empty());
    assert_eq!(fake.optimized_starts, 2);
    // A distinct drawn copy has no established implicit-H correspondence.
    let reviewed = request.inputs[1].as_mut().unwrap().as_mut().unwrap();
    reviewed.plan.start_atoms = vec![9999];
    let mut fake = Fake::default();
    calculate(&request, &mut fake, &Cancel::default(), |_| {}).unwrap();
    assert_eq!(fake.optimized_starts, 1);
}

#[test]
fn cancellation_stops_before_a_ts_or_later_job_is_started() {
    let request = request();
    let mut fake = Fake {
        cancel_after_species: true,
        ..Default::default()
    };
    let cancel = Cancel::default();
    let mut events = Vec::new();
    assert!(calculate(&request, &mut fake, &cancel, |event| events.push(event)).is_err());
    assert!(fake.transitions.is_empty());
    assert!(step_values(&events).is_empty());
}

#[test]
fn missing_ts_review_is_local_and_endpoints_still_complete() {
    let mut request = request();
    request.inputs[0] = Err("Map needs review".into());
    let mut fake = Fake::default();
    let mut events = Vec::new();
    calculate(&request, &mut fake, &Cancel::default(), |event| {
        events.push(event)
    })
    .unwrap();
    let values = step_values(&events);
    assert_eq!(values[0].1.energy, Some(0.));
    assert_eq!(values[0].1.error.as_deref(), Some("Map needs review"));
    assert_eq!(fake.transitions, [request.plan.steps[1].arrow]);
}

#[test]
fn state_retains_finished_values_on_cancel_and_rejects_late_events() {
    let mut state = State::default();
    state.serial = 14;
    state.running = true;
    state.steps = vec![StepStatus::Running, StepStatus::Pending];
    state.report.steps = vec![None, None];
    let value = StepValue::empty(10, XtbMethod::Gfn2Xtb);
    assert!(state.accept(Event {
        serial: 14,
        kind: EventKind::Step { index: 0, value }
    }));
    state.cancel();
    assert!(state.step(0).is_some());
    assert_eq!(state.steps, [StepStatus::Done, StepStatus::Cancelled]);
    assert!(!state.accept(Event {
        serial: 14,
        kind: EventKind::Finished(Ok(()))
    }));
}

#[test]
fn invalid_thermochemistry_never_replaces_a_valid_cached_frequency_check() {
    let frame = Arc::new(Frame {
        numbers: vec![8, 1, 1],
        positions_angstrom: xtb::tests::START.to_vec(),
        isotopes: Vec::new(),
        masses_amu: Vec::new(),
        charge: 0,
        multiplicity: 1,
        drawing_atoms: Vec::new(),
    });
    let report = xtb::tests::optfreq(&frame.positions_angstrom, &frame.positions_angstrom);
    let valid = Arc::new(
        report
            .computed
            .to_calc_result(
                &frame,
                qcdata::Task::Optimize,
                &xtb::run::example_engine(),
                None,
            )
            .unwrap(),
    );
    assert!(transition_thermo(&valid, &frame));
    let mut state = State::default();
    state.cache_value(
        "water".into(),
        XtbMethod::Gfn2Xtb,
        Arc::clone(&frame),
        Arc::clone(&valid),
        None,
    );
    let mut invalid = (*valid).clone();
    invalid.thermo.as_mut().unwrap().temperature_k = 350.;
    state.cache_value(
        "water".into(),
        XtbMethod::Gfn2Xtb,
        Arc::clone(&frame),
        Arc::new(invalid),
        None,
    );
    assert!(Arc::ptr_eq(&state.cache[0].result, &valid));
    let mut imaginary = (*valid).clone();
    imaginary.vibrations.as_mut().unwrap().frequencies_cm1[0] = -100.;
    state.cache_value(
        "water".into(),
        XtbMethod::Gfn2Xtb,
        frame,
        Arc::new(imaginary),
        None,
    );
    assert!(Arc::ptr_eq(&state.cache[0].result, &valid));
}

#[test]
fn native_jobs_check_cancellation_before_any_worker_can_be_spawned() {
    let native = Native {
        executable: PathBuf::from("/this/path/must/not/be/executed"),
        threads: 1,
    };
    let cancel = Cancel::default();
    cancel.cancel();
    let input = species_value(0, XtbMethod::Gfn2Xtb);
    let error = native
        .ordinary(&input.frame, XtbMethod::Gfn2Xtb, true, false, &cancel)
        .unwrap_err();
    assert!(error.contains("canceled"));
}
