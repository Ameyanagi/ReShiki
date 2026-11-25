//! The results palette after runs answered by hand, as in the card's tests.
use super::super::results::Evaluation;
use super::super::tests::{MOVED, START, event, f32s, halocarbon, model, optimized, send, water};
use super::super::{
    Action as XtbAction, CalcTask, Outcome, RunEvent as Event, RunReport as Report,
};
use super::*;
use crate::app::prompt::{Answer, Prompt};
use reshiki::document::Point;
use reshiki::geometry::Point3;

const MIDDLE: [[f64; 3]; 3] = [[0., 0., 0.005], [0.958, 0., 0.], [-0.24, 0.928, 0.]];

fn palette(app: &mut App, action: Action) {
    send(app, XtbAction::Palette(action));
}

/// A finished optimization through `frames`, the first being the start.
fn path(frames: &[&[[f64; 3]]]) -> Event {
    let energies: Vec<f64> = (0..frames.len())
        .map(|i| -5.06 - 0.005 * i as f64)
        .collect();
    let last = frames.last().expect("frames");
    Event::Done(Ok(std::sync::Arc::new(Report {
        engine_version: "0.2.0".into(),
        engine_rev: "9d0388c".into(),
        energy: *energies.last().expect("energies"),
        gap_ev: Some(0.05),
        near_degenerate: true,
        converged: true,
        positions: last.to_vec(),
        gradient: vec![0.; last.len() * 3],
        steps: frames.len() - 1,
        evaluations: energies
            .iter()
            .map(|&energy| Evaluation {
                energy,
                scf_iterations: 6,
                seconds: 0.003,
            })
            .collect(),
        numerics: "f64".into(),
        energies,
        path: frames
            .iter()
            .flat_map(|f| f.as_flattened())
            .copied()
            .collect(),
    })))
}

fn edit(app: &mut App, o: u64) {
    let before = app.tab.doc.clone();
    let carbon = app.tab.doc.add_atom("C", Point::new(40., 0.));
    app.tab.doc.add_bond(o, carbon, 1, "plain");
    app.changed(before);
}

#[test]
fn a_finished_run_opens_the_palette_and_the_card_defers_to_it() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    assert!(app.xtb_palette_visible());
    let shown = app.xtb_shown().expect("the new result");
    assert!(!shown.stale);
    assert!(
        shown
            .method()
            .starts_with("GFN2-xTB · axtb 0.2.0 · gas phase · charge 0 · multiplicity 1 · "),
        "{}",
        shown.method()
    );
    let labels: Vec<_> = shown.rows().iter().map(|(label, _)| *label).collect();
    assert_eq!(
        &labels[..3],
        ["Total energy", "HOMO–LUMO gap", "Optimization"]
    );
    assert!(matches!(app.tab.xtb.outcome, Some(Outcome::Done { .. })));
    assert!(app.xtb_bottom_palette().is_some());

    palette(&mut app, Action::CopyTable);
    assert!(app.status.starts_with("Copied 6 rows"), "{}", app.status);

    palette(&mut app, Action::Close);
    assert!(!app.xtb_palette_visible());
    assert!(app.xtb_bottom_palette().is_none());
    palette(&mut app, Action::Show);
    assert!(app.xtb_palette_visible());

    // Docked in the inspector, the bottom dock is empty.
    palette(&mut app, Action::Dock);
    assert!(app.xtb_bottom_palette().is_none());
    assert!(app.xtb_inspector_palette().is_some());
    assert_eq!(app.inspector_tab, crate::app::InspectorTab::Predict);
}

#[test]
fn an_edit_greys_the_results_undo_restores_them_and_recalculate_reruns() {
    let (mut app, o) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    palette(&mut app, Action::Labels(true));
    assert!(
        app.xtb_label()
            .is_some_and(|label| label.text == "E −5.070176 Eh")
    );

    edit(&mut app, o);
    assert!(app.xtb_shown().is_some_and(|shown| shown.stale));
    assert!(app.xtb_label().is_none(), "stale results draw no labels");

    let _ = app.update(Message::Undo);
    assert!(app.xtb_shown().is_some_and(|shown| !shown.stale));
    assert!(app.xtb_label().is_some());

    let _ = app.update(Message::Redo);
    assert!(app.xtb_shown().is_some_and(|shown| shown.stale));
    app.xtb.settings.task = CalcTask::Energy;
    palette(&mut app, Action::Recalculate);
    let run = app.tab.xtb.run.as_ref().expect("Recalculate starts a run");
    assert_eq!(
        run.task,
        CalcTask::Optimize,
        "the result's task, not the card's"
    );
    assert_eq!((run.charge, run.unpaired, run.solvent), (0, 0, None));
    assert_eq!(run.atoms.len(), 2, "the molecule as it is now");
    assert!(
        app.tab.model_job.is_some(),
        "the changed molecule gets a new model first"
    );
}

#[test]
fn a_result_from_a_late_run_arrives_stale() {
    let (mut app, o) = water(CalcTask::Optimize);
    edit(&mut app, o);
    event(&mut app, optimized(&START, &MOVED));
    assert!(app.xtb_shown().is_some_and(|shown| shown.stale));
    palette(&mut app, Action::Step(0));
    assert!(app.xtb_step().is_none(), "stale steps do not preview");
}

#[test]
fn a_step_previews_on_the_model_and_use_this_geometry_is_one_undo_step() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(&mut app, path(&[&START, &MIDDLE, &MOVED]));
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
    let shown = app.xtb_shown().expect("result");
    assert_eq!(results::warning(shown.info), Some(state::NEAR_DEGENERATE));

    palette(&mut app, Action::View(View::Steps));
    palette(&mut app, Action::Step(1));
    assert_eq!(model(app.display_document()), f32s(&MIDDLE));
    assert_eq!(
        model(&app.tab.doc),
        f32s(&MOVED),
        "a preview is not an edit"
    );
    let _ = app.update(Message::Escape);
    assert_eq!(model(app.display_document()), f32s(&MOVED));

    palette(&mut app, Action::Step(1));
    palette(&mut app, Action::UseStep);
    assert_eq!(model(&app.tab.doc), f32s(&MIDDLE));
    assert_eq!(app.xtb_step(), None);
    assert!(
        app.status.starts_with("Applied xTB geometry of step 1"),
        "{}",
        app.status
    );
    assert!(
        app.xtb_shown().is_some_and(|shown| !shown.stale),
        "moving the model keeps the result current"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(model(&app.tab.doc), f32s(&MOVED));
}

#[test]
fn a_step_with_another_configuration_asks_first() {
    let (mut app, _, start) = halocarbon();
    let mirrored: Vec<[f64; 3]> = start.iter().map(|&[x, y, z]| [-x, y, z]).collect();
    event(&mut app, path(&[&start, &mirrored, &start]));
    palette(&mut app, Action::Step(1));
    palette(&mut app, Action::UseStep);
    assert!(
        app.tab.xtb.palette.confirm[0].starts_with("Step 1 changes the configuration at C"),
        "{:?}",
        app.tab.xtb.palette.confirm
    );
    assert_eq!(model(&app.tab.doc), f32s(&start));
    palette(&mut app, Action::UseStepAnyway);
    assert_eq!(model(&app.tab.doc), f32s(&mirrored));
}

#[test]
fn keep_full_results_grows_the_saved_size_and_undoes() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(&mut app, path(&[&START, &MIDDLE, &MOVED]));
    let (light, full) = {
        let shown = app.xtb_shown().expect("result");
        app.xtb_sizes(&shown)
    };
    assert!(full > 0, "steps and the gradient are full data");
    let saved = app.tab.doc.science.saved();
    let path = saved.results[0].result.trajectory.as_ref().expect("path");
    assert!(
        path.positions_angstrom.is_empty(),
        "light results keep no steps"
    );

    palette(&mut app, Action::KeepFull(true));
    let shown = app.xtb_shown().expect("result");
    assert!(shown.stored.keep_full);
    let (kept, _) = app.xtb_sizes(&shown);
    assert!(kept > light, "{kept} > {light}");
    let _ = app.update(Message::Undo);
    assert!(app.xtb_shown().is_some_and(|shown| !shown.stored.keep_full));
}

/// A 482-atom chain whose kept optimization path (1000 steps) makes the
/// document's results larger than 5 MB.
pub(in crate::app) fn large_results() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let carbons = 160;
    let ids: Vec<u64> = (0..carbons)
        .map(|i| {
            app.tab
                .doc
                .add_atom("C", Point::new(i as f32 * 25., (i % 2) as f32 * 14.))
        })
        .collect();
    for pair in ids.windows(2) {
        app.tab.doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    app.tab.selected = vec![ids[0]];
    app.xtb.settings.task = CalcTask::Optimize;
    send(&mut app, XtbAction::Run);
    let order = app.model_atoms();
    let mut start: Vec<[f64; 3]> = order
        .iter()
        .map(|id| {
            let i = ids.iter().position(|c| c == id).expect("carbon") as f64;
            [i * 1.27, (i % 2.) * 0.87, 0.]
        })
        .collect();
    let mut parents = vec![];
    for (index, carbon) in start.clone().iter().enumerate() {
        let ends = order[index] == ids[0] || order[index] == ids[carbons - 1];
        for k in 0..if ends { 3 } else { 2 } {
            let side = if k % 2 == 0 { 1. } else { -1. };
            start.push([carbon[0] + 0.3 * k as f64, carbon[1] - 0.6, side * 0.9]);
            parents.push(index);
        }
    }
    let points = start.iter().map(|&[x, y, z]| Point3 { x, y, z }).collect();
    let _ = app.finish_model(points, parents);
    // Small, varied moves, so that the compact encoding cannot shrink them.
    let mut seed = 1u64;
    let frames: Vec<Vec<[f64; 3]>> = (0..1000)
        .map(|_| {
            start
                .iter()
                .map(|p| {
                    p.map(|x| {
                        seed = seed
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        x + ((seed >> 40) as f64 / (1u64 << 24) as f64 - 0.5) * 0.02
                    })
                })
                .collect()
        })
        .collect();
    let mut frames: Vec<&[[f64; 3]]> = frames.iter().map(Vec::as_slice).collect();
    frames[0] = &start;
    event(&mut app, path(&frames));
    palette(&mut app, Action::KeepFull(true));
    app
}

#[test]
fn save_asks_once_about_large_results_and_make_lighter_drops_the_steps() {
    let mut app = large_results();
    let bytes = app.tab.doc.science.file_bytes();
    assert!(bytes > reshiki::science::LIGHTER_BYTES, "{bytes}");
    let _ = app.update(Message::Save);
    let Some(Prompt::LargeResults {
        bytes: asked,
        lighter: Some((1, after)),
        ..
    }) = app.prompt.clone()
    else {
        panic!("Save asks about large results: {:?}", app.prompt.is_some());
    };
    assert_eq!(asked, bytes);
    assert!(after * 20 < bytes, "{after} of {bytes}");
    assert!(!app.file_io.saving, "nothing is written before the answer");

    let then = app.prompt.take().expect("prompt").answer(Answer::Save);
    let _ = app.update(then);
    assert!(app.xtb_shown().is_some_and(|shown| !shown.stored.keep_full));
    assert!(app.tab.doc.science.file_bytes() < reshiki::science::LIGHTER_BYTES);

    // Asked once per document.
    palette(&mut app, Action::KeepFull(true));
    app.file_io.saving = false;
    let _ = app.update(Message::Save);
    assert!(app.prompt.is_none());
}

/// Writes the large-results document for the Save warning screenshots:
/// `RESHIKI_XTB_LARGE_FIXTURE=/path/large.rsk cargo test … -- --ignored`.
#[test]
#[ignore = "writes an evidence fixture"]
fn write_large_results_fixture() {
    let path = std::env::var("RESHIKI_XTB_LARGE_FIXTURE").expect("fixture path");
    let app = large_results();
    std::fs::write(path, app.tab.doc.file_json().expect("file")).expect("written");
}
