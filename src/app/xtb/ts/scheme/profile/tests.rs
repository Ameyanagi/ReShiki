use super::*;
use crate::app::xtb::ts::scheme::{
    State,
    run::{SpeciesValue, StepOptions, StepValue},
};
use reshiki::graphics::{BracketSides, EnergyProfileModels, GraphicKind};
use reshiki::qcdata::{Task, XtbMethod};
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
            let result = crate::app::xtb::example_computed(-1. - index as f64 * 0.01, 1.)
                .to_calc_result(
                    &frame,
                    Task::Optimize,
                    &crate::app::xtb::example_engine(),
                    None,
                )
                .unwrap();
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
fn two_steps_build_five_states_with_cumulative_energies_and_added_material_labels() {
    let (doc, plan) = Plan::example();
    let report = report(&plan);
    let figure = build(&doc, &plan, &report, false, EnergyUnit::KcalPerMol).unwrap();
    assert_eq!(
        figure
            .states
            .iter()
            .map(|state| state.energy_kcal)
            .collect::<Vec<_>>(),
        [0., 10., -5., 15., -8.]
    );
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
    let shared = &figure.states[2].label;
    assert!(shared.contains("H₂O") && shared.contains("H₂"), "{shared}");
    assert!(
        figure
            .states
            .iter()
            .filter(|state| state.kind != Kind::TransitionState)
            .all(|state| state.geometry.is_some())
    );
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
    assert_eq!(
        gibbs
            .states
            .iter()
            .map(|state| state.energy_kcal)
            .collect::<Vec<_>>(),
        [0., 11., -4., 17., -6.]
    );
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
    assert_eq!(figure.states.last().unwrap().energy_kcal, -8.);
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
    app.tab.xtb.scheme.as_mut().unwrap().work.report.steps[1]
        .as_mut()
        .unwrap()
        .energy = Some(-7.);
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
    assert_eq!(
        graphic
            .energy_profile
            .as_deref()
            .unwrap()
            .states
            .last()
            .unwrap()
            .energy_kcal,
        -12.
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
        .gibbs = Some(30.);
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
    other.steps[1].source_key = Some(key);
    assert!(profile::source_current(&app.tab.doc, &other));
    assert!(app.xtb_scheme_profile_results(&other).is_empty());
}
