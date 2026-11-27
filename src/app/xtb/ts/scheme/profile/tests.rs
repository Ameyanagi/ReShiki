use super::*;
use crate::app::xtb::ts::scheme::{
    State,
    run::{SpeciesValue, StepOptions, StepValue},
};
use reshiki::graphics::{BracketSides, EnergyProfileModels, GraphicKind};
use reshiki::qcdata::{Task, Thermo, Vibrations, XtbMethod};
use std::cell::Cell;

fn report(plan: &Plan) -> Report {
    let species = plan
        .species
        .iter()
        .enumerate()
        .map(|(index, species)| {
            let atoms = &species.calculation.atoms;
            let frame = Arc::new(Frame {
                numbers: atoms
                    .iter()
                    .map(|atom| match atom.element.as_str() {
                        "H" => 1,
                        "C" => 6,
                        "O" => 8,
                        element => panic!("unexpected fixture element {element}"),
                    })
                    .collect(),
                positions_angstrom: atoms
                    .iter()
                    .enumerate()
                    .map(|(i, _)| [i as f64 * 1.35, 0., 0.])
                    .collect(),
                isotopes: vec![],
                masses_amu: vec![],
                charge: species.charge,
                multiplicity: species.multiplicity,
                drawing_atoms: vec![],
            });
            let (energy, correction) = match species.formula.as_str() {
                "C2H6O" => (-1000., 0.),
                "C2H4" => (-905., 1.),
                "C2H6" => (-958., 2.),
                "H2O" => (-100., 0.),
                "H2" => (-50., 0.),
                formula => panic!("unexpected fixture formula {formula}"),
            };
            let mut result = crate::app::xtb::example_computed(energy / KCAL_PER_HARTREE, 1.)
                .to_calc_result(
                    &frame,
                    Task::Optimize,
                    &crate::app::xtb::example_engine(),
                    None,
                )
                .unwrap();
            result.vibrations = Some(Vibrations {
                frequencies_cm1: vec![100.],
                ir_intensities_km_mol: vec![],
                modes: vec![],
                reduced_masses_amu: vec![],
                masses_amu: frame.numbers.iter().map(|_| 12.).collect(),
            });
            result.thermo = Some(Thermo {
                temperature_k: 298.15,
                pressure_pa: 101_325.,
                zero_point_energy_hartree: 0.,
                enthalpy_correction_hartree: correction / KCAL_PER_HARTREE,
                entropy_hartree_per_k: 0.,
                gibbs_correction_hartree: correction / KCAL_PER_HARTREE,
                heat_capacity_hartree_per_k: None,
                frequency_scale: 1.,
                rotor_cutoff_cm1: 50.,
                imaginary_cutoff_cm1: 20.,
                symmetry_number: 1,
                point_group: Some("C1".into()),
            });
            SpeciesValue {
                species: index,
                method: XtbMethod::Gfn2Xtb,
                frame,
                result: Arc::new(result),
                launch: None,
            }
        })
        .collect();
    let steps = plan
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            Some(StepValue {
                arrow: step.arrow,
                method: XtbMethod::Gfn2Xtb,
                ts: None,
                frames: vec![],
                launch: None,
                energy: Some(if index == 0 { -5. } else { -3. }),
                gibbs: Some(if index == 0 { -4. } else { -2. }),
                barrier: Some(if index == 0 { 10. } else { 20. }),
                free_barrier: Some(if index == 0 { 11. } else { 21. }),
                error: None,
            })
        })
        .collect();
    Report { species, steps }
}

fn energies(figure: &EnergyProfile) -> Vec<f64> {
    figure
        .states
        .iter()
        .map(|state| (state.energy_kcal * 1e9).round() / 1e9)
        .collect()
}

fn app() -> App {
    let (doc, plan) = Plan::example();
    let result = report(&plan);
    let (mut app, _) = App::new();
    app.tab.doc = doc.clone();
    app.tab.saved = doc;
    let mut work = super::super::run::State::default();
    work.report = result;
    app.tab.selected = plan.arrows.clone();
    app.tab.xtb.scheme = Some(State {
        options: vec![StepOptions::default(); plan.steps.len()],
        plan,
        method: XtbMethod::Gfn2Xtb,
        sheet: false,
        original: None,
        reviews: None,
        work,
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        checked: Cell::new(None),
        #[cfg(feature = "xtb")]
        pending_save: None,
    });
    app
}

#[test]
fn two_steps_use_one_inventory_for_energies_labels_and_models() {
    let (doc, plan) = Plan::example();
    let report = report(&plan);
    let figure = build(&doc, &plan, &report, false, EnergyUnit::KcalPerMol).unwrap();
    assert_eq!(energies(&figure), [0., 10., -5., 15., -8.]);
    assert_eq!(
        figure
            .states
            .iter()
            .map(|state| state.kind)
            .collect::<Vec<_>>(),
        [
            Kind::Reactant,
            Kind::TransitionState,
            Kind::Intermediate,
            Kind::TransitionState,
            Kind::Product
        ]
    );
    assert_eq!(figure.states[1].label, "TS1‡");
    assert_eq!(figure.states[3].label, "TS2‡");
    assert_eq!(figure.states[0].label, "C2H6O + H₂");
    assert_eq!(figure.states[2].label, "C2H4 + H₂O + H₂");
    assert_eq!(figure.states[4].label, "C2H6 + H₂O");
    for (state, inventory) in figure
        .states
        .iter()
        .filter(|state| state.kind != Kind::TransitionState)
        .zip(plan.levels().unwrap())
    {
        let numbers = inventory
            .terms()
            .flat_map(|term| {
                std::iter::repeat_n(
                    report.species(term.species).unwrap().frame.numbers.clone(),
                    term.coefficient as usize,
                )
                .flatten()
            })
            .collect::<Vec<_>>();
        assert_eq!(state.geometry.as_ref().unwrap().frame.numbers, numbers);
    }
    assert!((overall(&plan, &report, false).unwrap() + 8.).abs() < 1e-9);
    assert_eq!(
        (
            figure.steps[1].from,
            figure.steps[1].transition,
            figure.steps[1].to
        ),
        (2, Some(3), 4)
    );
    assert!(profile::source_current(&doc, &figure));
    let gibbs = build(&doc, &plan, &report, true, EnergyUnit::KjPerMol).unwrap();
    assert_eq!(gibbs.energy_kind, EnergyProfileKind::Gibbs);
    assert_eq!(energies(&gibbs), [0., 11., -4., 17., -6.]);
}

#[test]
fn free_barriers_choose_available_electronic_levels_and_saddles() {
    let mut app = app();
    let state = app.tab.xtb.scheme.as_mut().unwrap();
    state.options.fill(StepOptions {
        reaction_energy: false,
        barrier: false,
        free_energy: false,
        free_barrier: true,
    });
    for (index, step) in state.plan.steps.iter().enumerate() {
        let reactant = sum_energy(step.reactants.iter().copied(), |species| {
            super::super::run::species_energy(state.work.report.species(species)?, false)
        })
        .unwrap();
        let mut saddle = (*state
            .work
            .report
            .species(step.reactants[0].species)
            .unwrap()
            .result)
            .clone();
        saddle.energy_hartree =
            Some(reactant + if index == 0 { 10. } else { 20. } / KCAL_PER_HARTREE);
        saddle.status = reshiki::qcdata::Status::Converged;
        let value = state.work.report.steps[index].as_mut().unwrap();
        value.ts = Some(Arc::new(saddle));
        value.energy = None;
        value.gibbs = None;
        value.barrier = None;
    }
    let product = state.plan.steps.last().unwrap().products[0].species;
    let value = state
        .work
        .report
        .species
        .iter_mut()
        .find(|value| value.species == product)
        .unwrap();
    Arc::make_mut(&mut value.result).thermo = None;
    assert!(overall(&state.plan, &state.work.report, true).is_none());
    let figure = app.xtb_scheme_profile().unwrap();
    assert_eq!(figure.energy_kind, EnergyProfileKind::Electronic);
    assert_eq!(energies(&figure), [0., 10., -5., 15., -8.]);
    assert!(app.xtb_scheme_profile_available());
    app.xtb_place_scheme_profile();
    assert!(
        app.tab
            .doc
            .graphics
            .last()
            .unwrap()
            .energy_profile
            .is_some()
    );
    assert!(
        app.tab
            .xtb
            .scheme
            .as_ref()
            .unwrap()
            .work
            .report
            .steps
            .iter()
            .flatten()
            .all(|value| value.energy.is_none()
                && value.barrier.is_none()
                && value.gibbs.is_none())
    );
}

#[test]
fn missing_species_energy_does_not_use_a_retained_step_delta() {
    let (doc, plan) = Plan::example();
    let mut report = report(&plan);
    let product = plan.steps.last().unwrap().products[0].species;
    let value = report
        .species
        .iter_mut()
        .find(|value| value.species == product)
        .unwrap();
    Arc::make_mut(&mut value.result).energy_hartree = None;
    assert!(build(&doc, &plan, &report, false, EnergyUnit::KcalPerMol).is_none());
    assert_eq!(overall(&plan, &report, false), None);
    assert!(
        report
            .steps
            .iter()
            .flatten()
            .all(|value| value.energy.is_some())
    );
}

#[test]
fn failed_saddle_energy_does_not_create_a_fallback_barrier() {
    let (doc, plan) = Plan::example();
    let mut report = report(&plan);
    let mut result = (*report.species[0].result).clone();
    result.energy_hartree = Some(-0.5);
    for value in report.steps.iter_mut().flatten() {
        value.barrier = None;
    }
    for status in [
        reshiki::qcdata::Status::Failed {
            reason: "Search failed".into(),
        },
        reshiki::qcdata::Status::NotConverged {
            reason: "Saddle not converged".into(),
        },
    ] {
        result.status = status;
        report.steps[0].as_mut().unwrap().ts = Some(Arc::new(result.clone()));
        let figure = build(&doc, &plan, &report, false, EnergyUnit::KcalPerMol).unwrap();
        assert_eq!(energies(&figure), [0., -5., -8.]);
        assert!(
            figure
                .states
                .iter()
                .all(|state| state.kind != Kind::TransitionState)
        );
    }
}

#[test]
fn saddle_models_include_inventory_spectators_and_keep_changing_bonds() {
    let (doc, plan) = Plan::example();
    let mut report = report(&plan);
    let species = plan.steps[0].reactants[0].species;
    let mut reactant = (*report.species(species).unwrap().frame).clone();
    reactant.positions_angstrom = vec![[0., 0., 0.], [1.4, 0., 0.], [4., 0., 0.]];
    let mut saddle = reactant.clone();
    saddle.positions_angstrom = vec![[0., 0., 0.], [2., 0., 0.], [2., 1., 0.]];
    let mut product = reactant.clone();
    product.positions_angstrom = vec![[0., 0., 0.], [4., 0., 0.], [1.3, 0., 0.]];
    let mut result = (*report.species(species).unwrap().result).clone();
    result.frame = saddle.hash();
    result.endpoints = Some(reshiki::qcdata::Endpoints {
        reactant: reactant.hash(),
        product: product.hash(),
    });
    let ([_, original, _], _) =
        thumbnails::profile_geometry_from_frames(&result, &reactant, &saddle, &product).unwrap();
    assert_eq!(
        side_geometry(&plan, &report, [], Some(original.clone())),
        Some(original.clone())
    );
    let value = report.steps[0].as_mut().unwrap();
    value.ts = Some(Arc::new(result));
    value.frames = vec![Arc::new(reactant), Arc::new(saddle), Arc::new(product)];
    let figure = build(&doc, &plan, &report, false, EnergyUnit::KcalPerMol).unwrap();
    let geometry = figure.states[1].geometry.as_ref().unwrap();
    assert_eq!(geometry.frame.numbers, [6, 6, 8, 1, 1]);
    assert_eq!(geometry.forming, original.forming);
    assert_eq!(geometry.breaking, original.breaking);
    assert_eq!(geometry.forming, [[0, 2]]);
    assert_eq!(geometry.breaking, [[0, 1]]);
    geometry.validate().unwrap();
}

#[test]
fn placed_figure_numbers_a_lone_transition_state_by_its_step() {
    let (doc, plan) = Plan::example();
    let mut report = report(&plan);
    report.steps[0].as_mut().unwrap().barrier = None;
    let figure = build(&doc, &plan, &report, false, EnergyUnit::KcalPerMol).unwrap();
    assert!(
        figure
            .states_with_labels()
            .any(|(state, role, name)| state.kind == Kind::TransitionState
                && role == "TS2‡"
                && name.is_none())
    );
}

#[test]
fn unavailable_saddles_do_not_hide_completed_minima_or_create_zero_barriers() {
    let (doc, plan) = Plan::example();
    let mut report = report(&plan);
    report.steps[1].as_mut().unwrap().barrier = None;
    report.steps[1].as_mut().unwrap().error = Some("Saddle not verified".into());
    let figure = build(&doc, &plan, &report, false, EnergyUnit::KcalPerMol).unwrap();
    assert_eq!(figure.states.len(), 4);
    assert_eq!(figure.steps[1].transition, None);
    assert!((figure.states.last().unwrap().energy_kcal + 8.).abs() < 1e-9);
    report.steps[1] = None;
    assert!(build(&doc, &plan, &report, false, EnergyUnit::KcalPerMol).is_none());
}

#[test]
fn scheme_placement_skips_obstacles_selects_only_the_matching_chain_and_undoes_once() {
    let mut app = app();
    let figure = app.xtb_scheme_profile().unwrap();
    let mut single = figure.clone();
    single.states.truncate(3);
    single.states[2].kind = Kind::Product;
    single.steps.truncate(1);
    let mut other_chain = figure.clone();
    other_chain.steps[1].source_key = Some("b".repeat(64));
    for profile in [single, other_chain] {
        let id = app.tab.doc.next_id();
        app.tab.doc.graphics.push(Graphic::energy_profile(
            id,
            Point::new(-900., 0.),
            Point::new(100., 100.),
            profile,
            Default::default(),
        ));
    }
    let obstacle = app.tab.doc.next_id();
    app.tab.doc.graphics.push(Graphic::dragged(
        obstacle,
        GraphicKind::Rectangle,
        Point::new(-1000., 80.),
        Point::new(2000., 160.),
        Default::default(),
        BracketSides::Both,
        false,
    ));
    let before = app.tab.doc.clone();
    app.xtb_place_energy_profile(None);
    let [id] = app.tab.selected.as_slice() else {
        panic!("{}", app.status);
    };
    let id = *id;
    assert_eq!(app.tab.doc.graphics.len(), before.graphics.len() + 1);
    let (lo, _) = reshiki::scene::selection_bounds(&app.tab.doc, &[id]).unwrap();
    let (_, hi) = reshiki::scene::selection_bounds(&app.tab.doc, &[obstacle]).unwrap();
    assert!(lo.y > hi.y);
    app.tab.selected = app.tab.xtb.scheme.as_ref().unwrap().plan.arrows.clone();
    app.xtb_place_energy_profile(None);
    assert_eq!(app.tab.selected, [id]);
    assert_eq!(app.tab.doc.graphics.len(), before.graphics.len() + 1);
    let _ = app.update(crate::app::Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn refreshed_scheme_keeps_its_authored_frame_style_models_and_one_undo() {
    let mut app = app();
    app.xtb_place_scheme_profile();
    let graphic = app.tab.doc.graphics.last_mut().unwrap();
    graphic.origin = graphic.origin.offset(25., 40.);
    graphic.axis_x.x *= 1.2;
    graphic.axis_y.y *= 0.9;
    graphic.style.width_pt = 1.4;
    graphic.energy_profile.as_deref_mut().unwrap().models = EnergyProfileModels::RowAbove;
    let before = app.tab.doc.clone();
    let state = app.tab.xtb.scheme.as_mut().unwrap();
    let product = state.plan.steps[1].products[0].species;
    let value = state
        .work
        .report
        .species
        .iter_mut()
        .find(|value| value.species == product)
        .unwrap();
    *Arc::make_mut(&mut value.result)
        .energy_hartree
        .as_mut()
        .unwrap() -= 4. / KCAL_PER_HARTREE;
    app.xtb_refresh_scheme_profile(before.clone());
    let graphic = app.tab.doc.graphics.last().unwrap();
    let old = before.graphics.last().unwrap();
    assert_eq!(
        (
            graphic.origin,
            graphic.axis_x,
            graphic.axis_y,
            &graphic.style
        ),
        (old.origin, old.axis_x, old.axis_y, &old.style)
    );
    assert_eq!(
        graphic.energy_profile.as_deref().unwrap().models,
        EnergyProfileModels::RowAbove
    );
    assert!(
        (graphic
            .energy_profile
            .as_deref()
            .unwrap()
            .states
            .last()
            .unwrap()
            .energy_kcal
            + 12.)
            .abs()
            < 1e-9
    );
    let _ = app.update(crate::app::Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn linked_scheme_survives_native_reopen_and_becomes_stale_when_only_the_later_step_changes() {
    let mut app = app();
    app.xtb_place_scheme_profile();
    let reopened = Document::from_native_file(&app.tab.doc.file_json().unwrap()).unwrap();
    let figure = reopened
        .graphics
        .last()
        .unwrap()
        .energy_profile
        .as_deref()
        .unwrap();
    assert!(profile::source_current(&reopened, figure));
    let mut stale = reopened.clone();
    let atom = stale.reactions.last().unwrap().products[0].atoms[0];
    stale.atom_mut(atom).unwrap().charge = 1;
    assert!(!profile::source_current(&stale, figure));
    assert_eq!(profile::source_reaction_prefix(&stale, figure).len(), 1);
}

#[test]
fn free_only_scheme_places_gibbs_and_detached_figures_do_not_refresh() {
    let mut app = app();
    app.tab
        .xtb
        .scheme
        .as_mut()
        .unwrap()
        .options
        .fill(StepOptions {
            reaction_energy: false,
            barrier: false,
            free_energy: true,
            free_barrier: true,
        });
    assert_eq!(
        app.xtb_scheme_profile().unwrap().energy_kind,
        EnergyProfileKind::Gibbs
    );
    app.xtb_place_scheme_profile();
    app.tab
        .doc
        .graphics
        .last_mut()
        .unwrap()
        .energy_profile
        .as_deref_mut()
        .unwrap()
        .unlink();
    let before = app.tab.doc.clone();
    app.tab.xtb.scheme.as_mut().unwrap().work.report.steps[0]
        .as_mut()
        .unwrap()
        .free_barrier = Some(30.);
    app.xtb_refresh_scheme_profile(before.clone());
    assert_eq!(app.tab.doc, before);
}

#[test]
fn transient_irc_candidates_match_the_whole_chain_instead_of_only_its_first_arrow() {
    let mut app = app();
    let expected = app.xtb_scheme_profile().unwrap();
    let candidate = Arc::clone(&app.tab.xtb.scheme.as_ref().unwrap().work.report.species[0].result);
    app.tab.xtb.scheme.as_mut().unwrap().work.report.steps[0]
        .as_mut()
        .unwrap()
        .ts = Some(candidate);
    assert_eq!(app.xtb_scheme_profile_results(&expected).len(), 1);
    let arrow = app.tab.doc.next_id();
    let mut drawing = app.tab.doc.arrows[1].clone();
    drawing.id = arrow;
    app.tab.doc.arrows.push(drawing);
    let mut reaction = app.tab.doc.reactions[1].clone();
    reaction.arrow = arrow;
    reaction.products = app.tab.doc.reactions[0].reactants.clone();
    let key = profile::source_key(&app.tab.doc, &reaction);
    app.tab.doc.reactions.push(reaction);
    let mut other = expected;
    other.steps[1].source = Some(arrow);
    other.steps[1].source_key = Some(key);
    assert!(profile::source_current(&app.tab.doc, &other));
    assert!(app.xtb_scheme_profile_results(&other).is_empty());
}

#[test]
fn identical_branch_chemistry_keeps_placement_refresh_and_playback_on_the_selected_arrows() {
    let mut app = app();
    let doc = &mut app.tab.doc;
    let first = doc.reactions[0].arrow;
    let second = doc.reactions[1].arrow;
    let original_product = doc.reactions[1].products[0].atoms.clone();
    let a = doc.add_atom("C", Point::new(480., 100.));
    let b = doc.add_atom("C", Point::new(516., 100.));
    doc.add_bond(a, b, 1, "plain");
    let branch = doc.next_id();
    doc.arrows.push(reshiki::document::Arrow {
        id: branch,
        ..doc.arrows[1].clone()
    });
    let mut reaction = doc.reactions[1].clone();
    reaction.arrow = branch;
    reaction.products[0].atoms = vec![a, b];
    doc.reactions.push(reaction);
    reshiki::compound_numbers::number(doc, &original_product, Default::default()).unwrap();
    reshiki::compound_numbers::number(doc, &[a, b], Default::default()).unwrap();

    let run = |app: &mut App, second| {
        let plan = Plan::read(&app.tab.doc, &[first, second]).unwrap();
        app.tab.selected = plan.arrows.clone();
        let scheme = app.tab.xtb.scheme.as_mut().unwrap();
        scheme.work.report = report(&plan);
        scheme.plan = plan;
    };
    run(&mut app, second);
    app.xtb_place_scheme_profile();
    let original = app.tab.doc.graphics.last().unwrap().clone();
    let old = original.energy_profile.as_deref().unwrap();
    run(&mut app, branch);
    let fresh = app.xtb_scheme_profile().unwrap();
    assert_eq!(old.steps[1].source_key, fresh.steps[1].source_key);
    assert_ne!(
        old.states.last().unwrap().label,
        fresh.states.last().unwrap().label
    );
    for (figure, expected) in [(old, second), (&fresh, branch)] {
        let resolved = profile::source_reaction_prefix(&app.tab.doc, figure);
        assert_eq!(
            resolved.iter().map(|r| r.arrow).collect::<Vec<_>>(),
            [first, expected]
        );
    }
    let mut legacy = old.clone();
    legacy.steps[1].source = None;
    assert!(!profile::source_current(&app.tab.doc, &legacy));
    app.xtb_refresh_scheme_profile(app.tab.doc.clone());
    assert_eq!(app.tab.doc.graphics.last().unwrap(), &original);
    app.xtb_place_scheme_profile();
    let placed = app.tab.doc.graphics.last().unwrap();
    assert_ne!(placed.id, original.id);
    assert_eq!(placed.energy_profile.as_deref(), Some(&fresh));
    assert_eq!(app.tab.selected, [placed.id]);
}

#[test]
fn placing_gibbs_after_electronic_selects_the_matching_quantity() {
    let mut app = app();
    app.xtb_place_scheme_profile();
    let electronic = app.tab.selected.clone();
    app.tab
        .xtb
        .scheme
        .as_mut()
        .unwrap()
        .options
        .fill(StepOptions {
            reaction_energy: false,
            barrier: false,
            free_energy: true,
            free_barrier: true,
        });
    app.xtb_place_scheme_profile();
    let gibbs = app.tab.selected.clone();
    assert_ne!(gibbs, electronic);
    assert_eq!(
        app.selected_energy_profile()
            .unwrap()
            .energy_profile
            .as_ref()
            .unwrap()
            .energy_kind,
        EnergyProfileKind::Gibbs
    );
    app.xtb_place_scheme_profile();
    assert_eq!(app.tab.selected, gibbs);
}

#[test]
fn rerun_upgrades_an_unambiguous_legacy_link_in_place() {
    let mut app = app();
    app.xtb_place_scheme_profile();
    let graphic = app.tab.doc.graphics.last_mut().unwrap();
    let id = graphic.id;
    let step = &mut graphic.energy_profile.as_deref_mut().unwrap().steps[1];
    let arrow = step.source.take().unwrap();
    let before = app.tab.doc.clone();
    assert_eq!(before.file_version(), reshiki::document::ENERGY_PROFILES);
    app.xtb_refresh_scheme_profile(before);
    let upgraded = app.tab.doc.graphics.last().unwrap();
    assert_eq!(upgraded.id, id);
    assert_eq!(
        upgraded.energy_profile.as_deref().unwrap().steps[1].source,
        Some(arrow)
    );
    assert_eq!(
        app.tab.doc.file_version(),
        reshiki::document::SCHEME_PROFILE_LINKS
    );
}

#[cfg(all(not(windows), feature = "xtb"))]
#[test]
fn reopening_a_reordered_drawing_keeps_species_indices_for_retained_energies() {
    let mut app = app();
    let figure = app.xtb_scheme_profile().unwrap();
    app.tab.doc.arrows.reverse();
    app.tab.revision += 1;
    let _ = app.update(super::super::open_message());
    assert!(app.scheme_sheet_visible());
    assert_eq!(app.xtb_scheme_profile().unwrap(), figure);
}
