//! The Vibrations tab on a water Opt + freq result answered by hand.
use super::super::tests::{MOVED, START, event, f32s, model, optfreq, send, water};
use super::super::{Action as XtbAction, CalcTask, Outcome, palette, results::View, run};
use super::*;
use crate::app::Message;

fn vibrations(app: &mut App, action: Action) {
    send(app, XtbAction::Vibrations(action));
}

/// Water after Opt + freq, with the Vibrations tab open.
fn opened() -> App {
    let (mut app, _) = water(CalcTask::OptFreq);
    event(
        &mut app,
        run::Event::Done(Ok(Arc::new(optfreq(&START, &MOVED)))),
    );
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    app
}

/// The model positions the canvas shows now.
fn shown(app: &App) -> Vec<[f32; 3]> {
    model(app.display_document())
}

#[test]
fn opening_the_tab_animates_the_selected_mode_without_touching_the_document() {
    let mut app = opened();
    let frames = app
        .tab
        .xtb
        .vibrations
        .animation
        .as_ref()
        .map(|a| a.frames.len());
    assert_eq!(frames, Some(FRAMES), "{}", app.status);
    // Phase 0 is the equilibrium geometry.
    assert_eq!(shown(&app), f32s(&MOVED));
    vibrations(&mut app, Action::Tick);
    vibrations(&mut app, Action::Tick);
    let moved = shown(&app);
    assert_ne!(moved, f32s(&MOVED), "a tick moves the model");
    // Mode 1 of the example moves atom 1 along x only.
    let [first, second, ..] = MOVED;
    let [x, ..] = first;
    assert_ne!(moved.first().unwrap().first().copied(), Some(x as f32));
    assert_eq!(moved.get(1).copied(), Some(second.map(|x| x as f32)));
    assert_eq!(
        model(&app.tab.doc),
        f32s(&MOVED),
        "the document never moves"
    );
    assert!(app.status.starts_with("Mode 1 · 1590 cm⁻¹ · animating"));

    vibrations(&mut app, Action::Play(false));
    assert!(app.status.contains("paused"));
    assert_eq!(shown(&app), moved, "Pause keeps the current frame");

    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Summary)),
    );
    assert!(app.tab.xtb.vibrations.animation.is_none());
    assert_eq!(shown(&app), f32s(&MOVED));
}

#[test]
fn another_mode_or_amplitude_rebuilds_the_frames() {
    let mut app = opened();
    vibrations(&mut app, Action::Mode(2));
    for _ in 0..6 {
        vibrations(&mut app, Action::Tick);
    }
    let third = shown(&app);
    // Mode 3 of the example moves atom 1 along z.
    let [[x, _, z], ..] = MOVED;
    let [third_x, _, third_z] = *third.first().unwrap();
    assert_eq!(third_x, x as f32);
    assert_ne!(third_z, z as f32);
    let small = third_z - z as f32;
    vibrations(&mut app, Action::Amplitude(0.6));
    let [_, _, large_z] = *shown(&app).first().unwrap();
    let large = large_z - z as f32;
    assert!((large / small - 2.).abs() < 1e-3, "{small} → {large}");
}

#[test]
fn arrows_show_on_the_animation_and_add_to_the_drawing_as_one_undo_step() {
    let mut app = opened();
    use reshiki::models3d::{self, Display};
    models3d::set_display(&mut app.tab.doc, 0, Display::TwoD);
    vibrations(&mut app, Action::Arrows(true));
    assert_eq!(
        app.display_document().graphics.len(),
        1,
        "one atom moves in mode 1"
    );
    assert!(app.tab.doc.graphics.is_empty());
    vibrations(&mut app, Action::PlaceArrows);
    let arrows = &app.tab.doc.graphics;
    assert_eq!(arrows.len(), 1);
    let arrow = arrows.first().unwrap();
    assert!(arrow.heads.is_some(), "a pen path with an arrowhead");
    assert!(arrow.layer > 0, "in front of the model");
    assert!(app.tab.doc.arrows.is_empty(), "no reaction arrows");
    assert_eq!(app.tab.doc.models.first().unwrap().display, Display::Both);
    assert!(app.status.starts_with("Added 1 displacement arrows"));
    assert!(
        !app.tab.xtb.vibrations.playing,
        "the model stays at equilibrium under the placed arrows"
    );
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.graphics.is_empty());
    assert_eq!(app.tab.doc.models.first().unwrap().display, Display::TwoD);
}

#[test]
fn thermochemistry_changes_conditions_only_if_it_reproduces_the_saved_gibbs_correction() {
    let app = opened();
    let shown = app.xtb_shown().unwrap();
    let result = &*shown.stored.result;
    let vibrations = result.vibrations.as_ref().unwrap();
    let mut input = vibrational::ThermoInput {
        frequencies_cm1: &vibrations.frequencies_cm1,
        masses_amu: &vibrations.masses_amu,
        positions_angstrom: &shown.frame.positions_angstrom,
        linear: None,
        symmetry_number: 2,
        temperature_k: 298.15,
        pressure_pa: 101_325.,
        frequency_scale: 1.,
        rotor_cutoff_cm1: 50.,
        imaginary_cutoff_cm1: 20.,
    };
    assert!(!input.is_linear());
    let mut result = result.clone();
    for linear in [None, Some(true)] {
        input.linear = linear;
        input.temperature_k = 298.15;
        let mut saved = vibrational::thermochemistry(&input);
        saved.point_group = Some("c2v".into());
        result.thermo = Some(saved.clone());

        let hot = thermo_at(&result, shown.frame, 500., 101_325.).unwrap();
        input.temperature_k = 500.;
        assert_eq!(
            hot.gibbs_correction_hartree,
            vibrational::thermochemistry(&input).gibbs_correction_hartree
        );
        assert!(hot.entropy_hartree_per_k > saved.entropy_hartree_per_k);
        assert_eq!(hot.point_group, saved.point_group);
        assert!(thermo_at(&result, shown.frame, 0., 101_325.).is_none());

        saved.gibbs_correction_hartree += 0.01;
        result.thermo = Some(saved);
        assert!(thermo_at(&result, shown.frame, 500., 101_325.).is_none());
    }
    result.thermo.as_mut().unwrap().symmetry_number = 0;
    assert!(thermo_at(&result, shown.frame, 500., 101_325.).is_none());
}

#[test]
fn typed_conditions_have_finite_physical_bounds() {
    let mut settings = State {
        pressure: "1e-7".into(),
        ..State::default()
    };
    assert!(settings.conditions().is_none());
    settings.pressure = "2".into();
    assert_eq!(settings.conditions(), Some((298.15, 202_650.)));
    settings.temperature = "0.9".into();
    assert!(settings.conditions().is_none());
}

/// Water whose Frequencies result has one imaginary mode.
fn saddle() -> App {
    let (mut app, _) = water(CalcTask::Frequencies);
    let mut report = optfreq(&START, &START);
    report.steps = 0;
    report.path.clear();
    report.energies = vec![-5.070176];
    if let Some(v) = &mut report.computed.vibrations {
        v.frequencies_cm1 = vec![-212., 1590.13, 3468.58];
    }
    event(&mut app, run::Event::Done(Ok(Arc::new(report))));
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    app
}

#[test]
fn push_starts_opt_freq_from_the_displaced_geometry() {
    let mut app = saddle();
    let accepted = app.xtb_shown().unwrap().frame.positions_angstrom.clone();
    vibrations(&mut app, Action::Push);
    let run = app.tab.xtb.run.as_ref().expect("Push starts a run");
    assert_eq!(run.config.task, CalcTask::OptFreq);
    let start = run.start.as_ref().expect("a pushed start");
    // Mode 1 moves atom 1 along x by the push distance.
    let [[x, y, z], second, third] = accepted.as_slice() else {
        panic!("expected three atoms");
    };
    assert_eq!(
        start.positions,
        [[x + PUSH_ANGSTROM, *y, *z], *second, *third]
    );
    assert_eq!(start.from, accepted);
    assert_eq!(
        run.launch.as_ref().unwrap().frame.positions_angstrom,
        start.positions
    );
    assert!(app.tab.xtb.vibrations.animation.is_none());
}

#[test]
fn push_is_refused_if_the_model_moved_after_calculation() {
    let mut app = saddle();
    let shown = app.xtb_shown().unwrap();
    let mut config =
        super::super::results::config(&shown.stored.result, shown.frame, shown.atoms, 1).unwrap();
    config.task = CalcTask::OptFreq;
    let atom = *shown.atoms.iter().flatten().next().unwrap();
    let request = Request {
        atoms: vec![atom],
        config,
        keep_full: false,
        start: Some(Start {
            positions: vibrational::displaced(
                &START,
                &[1., 0., 0., 0., 0., 0., 0., 0., 0.],
                PUSH_ANGSTROM,
            ),
            from: START.to_vec(),
        }),
        search: None,
    };
    app.tab.doc = super::super::preview(&app.tab.doc, atom, &MOVED).unwrap();
    vibrations(&mut app, Action::Push);
    assert!(app.tab.xtb.run.is_none());
    assert_eq!(app.status, "The 3D model moved after this calculation");

    // Reject a request made before the move even at the worker boundary.
    let _ = app.xtb_begin(request);
    assert!(app.tab.xtb.run.is_none());
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Failed(failed)) if failed.failure == state::Failure::Other(
            "The 3D model moved after this calculation".into()
        )
    ));
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
}

#[test]
fn a_different_model_geometry_disables_animation() {
    let mut app = opened();
    let result = app.xtb_shown().unwrap();
    let atom = *result.atoms.iter().flatten().next().unwrap();
    app.tab.doc = super::super::preview(&app.tab.doc, atom, &START).unwrap();
    app.xtb_vibrations_sync();
    assert!(app.xtb_vibration_preview().is_none());
    assert_eq!(shown(&app), f32s(&START));
}

#[test]
fn an_unconverged_push_keeps_the_accepted_structure() {
    let mut app = saddle();
    vibrations(&mut app, Action::Push);
    let start = app.tab.xtb.run.as_ref().unwrap().start.as_ref().unwrap();
    let mut report = optfreq(&start.positions, &MOVED);
    report.converged = false;
    event(&mut app, run::Event::Done(Ok(Arc::new(report))));
    assert_eq!(model(&app.tab.doc), f32s(&START), "{}", app.status);
    assert_eq!(app.tab.xtb.palette.view, View::Summary);
    assert!(matches!(
        &app.tab.xtb.outcome,
        Some(Outcome::Done { notes, .. })
            if notes.iter().any(|n| n.contains("accepted structure is unchanged"))
    ));
    assert_eq!(app.tab.doc.science.results.len(), 2, "the result is kept");
}

#[test]
fn a_converged_push_replaces_the_structure_and_shows_the_vibrations() {
    let mut app = saddle();
    vibrations(&mut app, Action::Push);
    let displaced = app
        .tab
        .xtb
        .run
        .as_ref()
        .unwrap()
        .start
        .as_ref()
        .unwrap()
        .positions
        .clone();
    event(
        &mut app,
        run::Event::Done(Ok(Arc::new(optfreq(&displaced, &MOVED)))),
    );
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
    assert_eq!(app.tab.xtb.palette.view, View::Vibrations);
    let shown = app.xtb_shown().unwrap();
    assert_eq!(shown.input.positions_angstrom, displaced);
    let light = shown.stored.result.light(&[]);
    assert_eq!(light.input_frame, Some(shown.input.hash()));
    assert_eq!(
        super::super::results::step_positions(&light, 0, shown.input, shown.frame),
        Some(displaced)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), f32s(&START), "one Undo step");
}

fn light_result(frequency: f64) -> (App, Arc<CalcResult>) {
    let (mut app, _) = water(CalcTask::OptFreq);
    let mut report = optfreq(&START, &MOVED);
    *report
        .computed
        .vibrations
        .as_mut()
        .unwrap()
        .frequencies_cm1
        .first_mut()
        .unwrap() = frequency;
    event(&mut app, run::Event::Done(Ok(Arc::new(report))));
    let stored = app.tab.doc.science.results.first_mut().unwrap();
    let light = Arc::new(stored.result.light(&[]));
    assert!(light.vibrations.as_ref().unwrap().modes.is_empty());
    stored.result = light.clone();
    app.tab.xtb.palette.show(&light);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    assert!(app.tab.xtb.vibrations.animation.is_none());
    (app, light)
}

/// Answer the pending recompute; only engine builds start one.
#[cfg(all(not(windows), feature = "xtb"))]
fn recomputed(app: &mut App, frequency: f64) {
    let mut report = optfreq(&MOVED, &MOVED);
    *report
        .computed
        .vibrations
        .as_mut()
        .unwrap()
        .frequencies_cm1
        .first_mut()
        .unwrap() = frequency;
    let pending = app.tab.xtb.vibrations.recompute.as_ref().unwrap();
    vibrations(
        app,
        Action::Recomputed(pending.epoch, pending.serial, Ok(Arc::new(report))),
    );
}

#[test]
#[cfg(all(not(windows), feature = "xtb"))]
fn a_light_result_recomputes_its_modes_when_the_tab_opens() {
    let (mut app, light) = light_result(1590.13);
    assert!(app.tab.xtb.vibrations.recomputing(&light));
    recomputed(&mut app, 1592.13);
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    assert!(app.xtb_modes(&light).is_some());
    assert!(app.tab.xtb.vibrations.animation.is_some());
    assert!(app.status.contains("differ by up to 2.0 cm⁻¹"));
}

#[test]
#[cfg(all(not(windows), feature = "xtb"))]
fn recomputed_modes_require_matching_frequency_signs_and_values() {
    for (saved, fresh, drift) in [(-0.5, 0.5, "1.0"), (1590.13, 1593.13, "3.0")] {
        let (mut app, light) = light_result(saved);
        recomputed(&mut app, fresh);
        assert!(app.xtb_modes(&light).is_none());
        assert!(app.xtb_vibration_preview().is_none());
        assert_eq!(
            app.tab.xtb.vibrations.problem.as_ref().unwrap().1,
            format!(
                "The recomputed frequencies differ from the saved ones (up to {drift} cm⁻¹); the modes are not shown."
            )
        );
        send(
            &mut app,
            XtbAction::Palette(palette::Action::View(View::Summary)),
        );
        send(
            &mut app,
            XtbAction::Palette(palette::Action::View(View::Vibrations)),
        );
        assert!(app.tab.xtb.vibrations.problem.is_none());
        assert!(app.tab.xtb.vibrations.recomputing(&light));
    }
}

#[test]
#[cfg(all(not(windows), feature = "xtb"))]
fn a_recompute_reply_from_a_previous_file_epoch_is_ignored() {
    let (mut app, light) = light_result(1590.13);
    app.tab.file_epoch += 1;
    recomputed(&mut app, 1590.13);
    assert!(app.xtb_modes(&light).is_none());
    assert!(app.xtb_vibration_preview().is_none());
}

#[test]
#[cfg(all(not(windows), feature = "xtb"))]
fn switching_away_from_docked_vibrations_cancels_recomputation() {
    let (mut app, light) = light_result(1590.13);
    send(&mut app, XtbAction::Palette(palette::Action::Dock));
    send(&mut app, XtbAction::Page(Page::Nmr));
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    send(&mut app, XtbAction::Page(Page::Xtb));
    assert!(app.tab.xtb.vibrations.recomputing(&light));
}

#[test]
#[cfg(all(not(windows), feature = "xtb"))]
fn undoing_the_result_cancels_recomputation() {
    let (mut app, light) = light_result(1590.13);
    let _ = app.update(Message::Undo);
    assert!(app.xtb_shown().is_none());
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
}

#[test]
fn animation_ticks_leave_the_view_and_style_choosers_open() {
    let mut app = opened();
    app.view_open = true;
    app.status_style_open = true;
    let _ = app.update(Message::Xtb(XtbAction::Vibrations(Action::Tick)));
    assert!(app.view_open && app.status_style_open);
}

/// A build without the engine (or Windows, until code signing) keeps the
/// saved frequencies and explains why nothing animates.
#[test]
#[cfg(any(windows, not(feature = "xtb")))]
fn without_the_engine_a_light_result_does_not_recompute() {
    let (app, light) = light_result(1590.13);
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    assert!(app.xtb_modes(&light).is_none());
}
