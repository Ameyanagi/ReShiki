use super::*;
use reshiki::graphics::{EnergyProfileKind, EnergyProfileState, EnergyProfileStep};

fn value() -> StepValue {
    StepValue {
        arrow: 1,
        method: XtbMethod::Gfn2Xtb,
        ts: None,
        frames: vec![],
        launch: None,
        energy: Some(4.2),
        gibbs: Some(-1.3),
        barrier: None,
        free_barrier: None,
        error: None,
    }
}

#[test]
fn setup_has_four_independent_quantities_with_the_reaction_defaults() {
    let options = StepOptions::default();
    let choices = quantities(&options);
    assert_eq!(
        choices.map(|(_, _, _, checked)| checked),
        [true, true, false, false]
    );
    assert_eq!(
        choices.map(|(_, id, _, _)| id),
        ["energy", "barrier", "free-energy", "free-barrier"]
    );
    assert!(any_quantity(&[options]));
    assert!(!any_quantity(&[StepOptions {
        reaction_energy: false,
        barrier: false,
        free_energy: false,
        free_barrier: false
    }]));
    assert!(any_quantity(&[StepOptions {
        reaction_energy: false,
        barrier: false,
        free_energy: false,
        free_barrier: true
    }]));
}

#[test]
fn progress_retains_the_specific_failure_and_cancellation_explanation() {
    assert_eq!(status_line(&StepStatus::Pending).0, "Waiting");
    assert_eq!(status_line(&StepStatus::Running).0, "Calculating…");
    assert_eq!(status_line(&StepStatus::Done).0, "Done");
    assert_eq!(
        status_line(&StepStatus::Failed("No saddle found".into())).0,
        "No saddle found"
    );
    assert!(
        status_line(&StepStatus::Cancelled)
            .0
            .contains("finished results kept")
    );
}

#[test]
fn result_rows_never_present_a_missing_barrier_as_zero() {
    let rows = energy_rows(&value(), EnergyUnit::KcalPerMol);
    assert_eq!(
        rows,
        [
            ("ΔE".into(), "+4.2 kcal/mol".into()),
            ("ΔG (298 K)".into(), "−1.3 kcal/mol".into())
        ]
    );
    let mut nonfinite = value();
    nonfinite.barrier = Some(f64::NAN);
    nonfinite.free_barrier = Some(f64::INFINITY);
    assert_eq!(energy_rows(&nonfinite, EnergyUnit::KcalPerMol), rows);
    assert_eq!(signed_energy(EnergyUnit::KcalPerMol, 0.2), "+0.2 kcal/mol");
    assert_eq!(signed_energy(EnergyUnit::KcalPerMol, 0.), "0.0 kcal/mol");
}

fn profile() -> EnergyProfile {
    let states = [
        ("1", EnergyProfileStateKind::Reactant, 0.),
        ("2", EnergyProfileStateKind::Intermediate, 4.),
        ("TS1‡", EnergyProfileStateKind::TransitionState, 12.),
        ("3", EnergyProfileStateKind::Product, -2.),
    ]
    .map(|(label, kind, energy_kcal)| EnergyProfileState {
        kind,
        label: label.into(),
        energy_kcal,
        geometry: None,
    })
    .into();
    let steps = vec![
        EnergyProfileStep {
            from: 0,
            to: 1,
            transition: Some(2),
            source: None,
            source_key: None,
            irc: vec![],
        },
        EnergyProfileStep {
            from: 1,
            to: 3,
            transition: None,
            source: None,
            source_key: None,
            irc: vec![],
        },
    ];
    let mut profile = EnergyProfile::new(1, "0".repeat(64), "GFN2-xTB".into(), states, steps);
    profile.energy_kind = EnergyProfileKind::Gibbs;
    profile.unit = EnergyUnit::KjPerMol;
    profile
}

#[test]
fn multistep_profile_uses_step_coordinates_labels_and_selected_units() {
    let profile = profile();
    assert_eq!(profile_coordinates(&profile), [0., 1., 0.5, 2.]);
    let chart = SchemeChart::new(profile, &reshiki::document::Document::default(), false);
    assert_eq!(
        chart
            .profile
            .samples
            .iter()
            .map(|sample| sample.x)
            .collect::<Vec<_>>(),
        [0., 0.5, 1., 2.]
    );
    assert_eq!(chart.profile.energy_bounds(), (-2., 12.));
    let name = chart.accessible_name();
    assert!(name.contains("ΔG (kJ/mol)"));
    assert!(name.contains("TS1‡ 50.2"));
    assert!(name.contains("3 −8.4"));
}

#[test]
fn saved_irc_samples_keep_their_real_shape_and_untraced_steps_keep_dashed_guides() {
    use reshiki::graphics::EnergyProfileSample;
    let mut profile = profile();
    profile.steps[0].irc = [(0., 0.), (0.3, 15.), (1., 4.)]
        .map(|(x, energy_kcal)| EnergyProfileSample { x, energy_kcal })
        .into();
    let chart = SchemeChart::new(
        profile.clone(),
        &reshiki::document::Document::default(),
        false,
    );
    assert!(chart.traced);
    assert_eq!(chart.profile.style, Style::Line);
    assert_eq!(
        chart
            .profile
            .samples
            .iter()
            .map(|sample| (sample.x, sample.energy))
            .collect::<Vec<_>>(),
        [(0., 0.), (0.3, 15.), (1., 4.)]
    );
    assert_eq!(chart.profile.energy_bounds(), (-2., 15.));
    assert_eq!(chart.untraced_steps.len(), 1);
    assert_eq!(
        chart.untraced_steps[0]
            .iter()
            .map(|sample| sample.x)
            .collect::<Vec<_>>(),
        [1., 2.]
    );
    profile.steps[1].irc = [(0., 4.), (0.7, 8.), (1., -2.)]
        .map(|(x, energy_kcal)| EnergyProfileSample { x, energy_kcal })
        .into();
    let both = SchemeChart::new(profile, &reshiki::document::Document::default(), false);
    assert!(both.untraced_steps.is_empty());
    assert!(
        both.profile.samples[3].x.is_nan(),
        "separate IRC traces must not create an invented connector"
    );
    assert_eq!(both.profile.samples[5].x, 1.7);
}

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
fn selected_scheme() -> App {
    use reshiki::document::{Arrow, Point};
    use reshiki::reactions::{Participant, Reaction};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let doc = &mut app.tab.doc;
    let species = [0., 180., 360.].map(|x| doc.add_atom("C", Point::new(x, 0.)));
    for pair in species.windows(2) {
        let arrow = doc.next_id();
        doc.arrows.push(Arrow::new(
            arrow,
            Point::new(50., 0.),
            Point::new(130., 0.),
            Default::default(),
            Default::default(),
        ));
        let mut reaction = Reaction::new(arrow);
        reaction.reactants.push(Participant {
            atoms: vec![pair[0]],
            coefficient: 1,
        });
        reaction.products.push(Participant {
            atoms: vec![pair[1]],
            coefficient: 1,
        });
        doc.reactions.push(reaction);
        app.tab.selected.push(arrow);
    }
    let _ = app.update(send(Action::Open));
    app
}

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn setup_controls_publish_unique_ids_and_toggle_only_the_chosen_step() {
    use crate::app::xtb::ts::card::tests::ViewHarness;
    let ui = ViewHarness::new().await;
    let mut app = selected_scheme();
    assert!(app.scheme_sheet_visible());
    let snapshot = ui.inspect(app.with_scheme_sheet(text("").into()));
    assert!(snapshot.semantics.duplicate_ids.is_empty());
    for index in 0..2 {
        for (_, id, _, _) in quantities(&StepOptions::default()) {
            let id = format!("xtb-scheme-{index}-{id}");
            assert!(
                snapshot.semantics.nodes.iter().any(|node| node.id == id),
                "{id}"
            );
        }
    }
    let action = ui
        .activate(
            app.with_scheme_sheet(text("").into()),
            "xtb-scheme-1-free-barrier",
        )
        .unwrap();
    assert!(matches!(
        &action,
        Message::Xtb(xtb::Action::Ts(super::super::super::Action::Scheme(
            Action::Toggle(1, Quantity::FreeBarrier)
        )))
    ));
    let _ = app.update(action);
    let options = &app.tab.xtb.scheme.as_ref().unwrap().options;
    assert!(!options[0].free_barrier);
    assert!(options[1].free_barrier);
}

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn path_keeps_partial_energies_and_one_failure_reason_with_retry() {
    use crate::app::xtb::ts::card::tests::ViewHarness;
    let ui = ViewHarness::new().await;
    let mut app = selected_scheme();
    let state = app.tab.xtb.scheme.as_mut().unwrap();
    state.sheet = false;
    let reason = "No saddle found";
    let skipped = "Barrier skipped: the map could not include the added H₂";
    state.plan.steps[0].notes.push(skipped.into());
    state.work.steps = vec![StepStatus::Done, StepStatus::Failed(reason.into())];
    let mut first = value();
    first.arrow = state.plan.arrows[0];
    let mut second = value();
    second.arrow = state.plan.arrows[1];
    second.energy = Some(-1.4);
    second.gibbs = None;
    second.error = Some(reason.into());
    state.work.report.steps = vec![Some(first), Some(second)];
    state.work.error = Some("The xTB worker could not start".into());
    let snapshot = ui.inspect(app.xtb_scheme_path(true));
    assert_eq!(
        snapshot
            .texts
            .iter()
            .filter(|text| *text == skipped)
            .count(),
        1
    );
    assert_eq!(
        snapshot.texts.iter().filter(|text| *text == reason).count(),
        1
    );
    assert!(snapshot.texts.iter().any(|text| text == "Overall ΔE"));
    assert!(snapshot.texts.iter().any(|text| text == "+2.8 kcal/mol"));
    assert!(
        snapshot
            .texts
            .iter()
            .any(|text| text == "The xTB worker could not start")
    );
    assert!(
        !snapshot
            .texts
            .iter()
            .any(|text| text == "Overall ΔG (298 K)")
    );
    assert!(snapshot.semantics.duplicate_ids.is_empty());
    assert!(matches!(
        ui.activate(app.xtb_scheme_path(true), "xtb-scheme-retry-1"),
        Some(Message::Xtb(xtb::Action::Ts(
            super::super::super::Action::Scheme(Action::Retry(1))
        )))
    ));
}

#[cfg(all(not(target_arch = "wasm32"), feature = "xtb"))]
#[tokio::test]
#[ignore = "Opt-in renderer check: needs a graphics adapter"]
async fn atom_map_review_preparation_has_progress_and_cancel_in_the_card() {
    use crate::app::xtb::ts::card::tests::ViewHarness;
    use std::collections::VecDeque;
    let ui = ViewHarness::new().await;
    let mut app = selected_scheme();
    let state = app.tab.xtb.scheme.as_mut().unwrap();
    state.sheet = false;
    state.reviews = Some(super::super::MapReviews {
        id: 1,
        pending: VecDeque::from([1]),
        active: Some(0),
        inputs: vec![Ok(None); 2],
        retry: None,
    });
    state.work.error = Some("Could not save the completed results".into());
    let snapshot = ui.inspect(app.xtb_scheme_card());
    assert!(
        snapshot
            .texts
            .iter()
            .any(|text| text == "Reviewing the atom map · step 1 of 2")
    );
    assert!(
        snapshot
            .texts
            .iter()
            .any(|text| text == "Could not save the completed results")
    );
    assert!(matches!(
        ui.activate(app.xtb_scheme_card(), "xtb-scheme-cancel"),
        Some(Message::Xtb(xtb::Action::Ts(
            super::super::super::Action::Scheme(Action::Cancel)
        )))
    ));
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "xtb")))]
#[test]
fn predict_scheme_calculate_is_disabled_without_xtb_like_the_context_menu() {
    let (doc, plan) = super::super::plan::Plan::example();
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = doc;
    app.tab.selected = plan.arrows;
    assert!(
        !app.context_commands()
            .iter()
            .any(|command| command.menu == "Calculate scheme…")
    );
    assert!(app.xtb_scheme_open_action().is_none());
}
