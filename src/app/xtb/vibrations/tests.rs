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
    assert_ne!(moved[0][0], MOVED[0][0] as f32);
    assert_eq!(moved[1], f32s(&MOVED)[1]);
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
    assert_eq!(third[0][0], MOVED[0][0] as f32);
    assert_ne!(third[0][2], MOVED[0][2] as f32);
    let small = third[0][2] - MOVED[0][2] as f32;
    vibrations(&mut app, Action::Amplitude(0.6));
    let large = shown(&app)[0][2] - MOVED[0][2] as f32;
    assert!((large / small - 2.).abs() < 1e-3, "{small} → {large}");
}

#[test]
fn arrows_show_on_the_animation_and_add_to_the_drawing_as_one_undo_step() {
    let mut app = opened();
    vibrations(&mut app, Action::Arrows(true));
    assert_eq!(
        app.display_document().arrows.len(),
        1,
        "one atom moves in mode 1"
    );
    assert!(app.tab.doc.arrows.is_empty());
    vibrations(&mut app, Action::PlaceArrows);
    let arrows = &app.tab.doc.arrows;
    assert_eq!(arrows.len(), 1);
    assert_eq!(arrows[0].purpose, reshiki::purpose::Purpose::Illustration);
    assert!(app.status.starts_with("Added 1 displacement arrows"));
    let _ = app.update(Message::Undo);
    assert!(app.tab.doc.arrows.is_empty());
}

#[test]
fn thermochemistry_uses_the_saved_values_at_the_saved_conditions_and_recomputes_otherwise() {
    let app = opened();
    let shown = app.xtb_shown().unwrap();
    let result = &shown.stored.result;
    let saved = result.thermo.clone().unwrap();
    let at = |kelvin| thermo_at(result, shown.frame, kelvin, 101_325.).unwrap();
    let hot = at(500.);
    let cold = at(298.15);
    assert!(hot.entropy_hartree_per_k > cold.entropy_hartree_per_k);
    assert!(hot.gibbs_correction_hartree < cold.gibbs_correction_hartree);
    assert_eq!(hot.symmetry_number, saved.symmetry_number);
    let rows = view::thermo_rows(&saved, result.energy_hartree);
    assert_eq!(rows[0], ("ZPVE", "0.019453 Eh".to_owned()));
    assert!(
        rows.iter()
            .any(|row| *row == ("Point group", "C2v".to_owned()))
    );
    let mut settings = State::default();
    settings.pressure = "0".into();
    assert!(settings.conditions().is_none());
    settings.pressure = "2".into();
    assert_eq!(settings.conditions(), Some((298.15, 202_650.)));
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
    vibrations(&mut app, Action::Push);
    let run = app.tab.xtb.run.as_ref().expect("Push starts a run");
    assert_eq!(run.config.task, CalcTask::OptFreq);
    let start = run.start.as_ref().expect("a pushed start");
    // Mode 1 moves atom 1 along x: by exactly the push distance.
    // (The model stores f32 coordinates.)
    assert!((start[0][0] - START[0][0] - PUSH_ANGSTROM).abs() < 1e-6);
    assert!((start[1][0] - START[1][0]).abs() < 1e-6);
    assert_eq!(start[1][1..], START[1][1..]);
    assert!(app.tab.xtb.vibrations.animation.is_none());
}

#[test]
fn an_unconverged_push_keeps_the_accepted_structure() {
    let mut app = saddle();
    vibrations(&mut app, Action::Push);
    let mut report = optfreq(&START, &MOVED);
    report.converged = false;
    event(&mut app, run::Event::Done(Ok(Arc::new(report))));
    assert_eq!(model(&app.tab.doc), f32s(&START), "{}", app.status);
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
    event(
        &mut app,
        run::Event::Done(Ok(Arc::new(optfreq(&START, &MOVED)))),
    );
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
    assert_eq!(app.tab.xtb.palette.view, View::Vibrations);
    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), f32s(&START), "one Undo step");
}

#[test]
fn a_light_result_recomputes_its_modes_when_the_tab_opens() {
    let (mut app, _) = water(CalcTask::OptFreq);
    event(
        &mut app,
        run::Event::Done(Ok(Arc::new(optfreq(&START, &MOVED)))),
    );
    let light = Arc::new(app.tab.doc.science.results[0].result.light(&[]));
    assert!(light.vibrations.as_ref().unwrap().modes.is_empty());
    app.tab.doc.science.results[0].result = light.clone();
    app.tab.xtb.palette.show(&light);
    send(
        &mut app,
        XtbAction::Palette(palette::Action::View(View::Vibrations)),
    );
    assert!(app.tab.xtb.vibrations.recomputing(&light));
    assert!(app.tab.xtb.vibrations.animation.is_none());

    let mut report = optfreq(&MOVED, &MOVED);
    report.positions = MOVED.to_vec();
    if let Some(v) = &mut report.computed.vibrations {
        v.frequencies_cm1[0] += 2.;
    }
    vibrations(&mut app, Action::Recomputed(1, Ok(Arc::new(report))));
    assert!(!app.tab.xtb.vibrations.recomputing(&light));
    assert!(app.xtb_modes(&light).is_some());
    assert!(app.tab.xtb.vibrations.animation.is_some());
    assert!(
        app.status
            .contains("differ from the saved ones by up to 2.0 cm⁻¹")
    );
}
