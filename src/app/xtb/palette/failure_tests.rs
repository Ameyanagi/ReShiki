//! Failures keep their molecule identity independently of selection and retry.
use super::super::super::tests::{MOVED, START, event, optimized, send, water};
use super::super::super::{CalcTask, Outcome, RunEvent as Event};
use super::*;
use reshiki::document::Point;
use reshiki::geometry::Point3;
use reshiki::qcdata::Status;
use std::sync::Arc;

fn add_molecule(app: &mut App) -> u64 {
    let before = app.tab.doc.clone();
    let atom = app.tab.doc.add_atom("O", Point::new(200., 0.));
    app.changed(before);
    atom
}

#[test]
fn a_validation_failure_does_not_hide_another_molecules_results() {
    let (mut app, first) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    let result = Arc::clone(&app.xtb_shown().unwrap().stored.result);
    let other = add_molecule(&mut app);
    app.tab.selected = vec![other];
    app.xtb.settings.iterations = "invalid".into();
    send(&mut app, XtbAction::Run);
    let failed = app.xtb_failed().expect("the invalid target's failure");
    assert_eq!(failed.atoms, vec![other]);
    assert!(failed.request.is_none());

    app.tab.selected = vec![first];
    assert!(app.xtb_outcome().is_none());
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &result
    ));
    send(&mut app, XtbAction::Palette(Action::CopyTable));
    assert!(app.status.starts_with("Copied "), "{}", app.status);
}

#[test]
fn a_running_failure_keeps_its_target_after_selection_changes() {
    let (mut app, first) = water(CalcTask::Optimize);
    let other = add_molecule(&mut app);
    app.tab.selected = vec![other];
    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf("250 iterations".into()))),
    );
    let Some(Outcome::Failed(failed)) = &app.tab.xtb.outcome else {
        panic!("a failed run");
    };
    assert_eq!(failed.atoms, vec![first]);
    assert_eq!(failed.request.as_ref().unwrap().atoms, vec![first]);
    assert!(app.xtb_outcome().is_none());
    app.tab.selected = vec![first];
    assert!(app.xtb_failed().is_some());
}

#[test]
fn clearing_selection_does_not_disclose_a_different_molecules_pinned_result() {
    let (mut app, _) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    let pinned = Arc::clone(&app.xtb_shown().unwrap().stored.result);
    let other = add_molecule(&mut app);
    app.tab.selected = vec![other];
    send(&mut app, XtbAction::Run);
    let positions = START.map(|[x, y, z]| Point3 { x, y, z }).to_vec();
    let _ = app.finish_model(positions, vec![0, 0]);
    event(
        &mut app,
        Event::Done(Err(state::Failure::Scf("250 iterations".into()))),
    );
    app.tab.selected.clear();
    app.tab.xtb.palette.previous = true;
    assert!(app.xtb_target().is_none());
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &pinned
    ));
    let failed = app.xtb_failed().expect("the most recent run failed");
    assert_eq!(failed.atoms, vec![other]);
    assert!(app.xtb_last_successful(failed).is_none());
    assert!(app.xtb_previous_result(failed, false).is_none());
}

#[test]
fn the_previous_run_is_the_newest_matching_converged_result() {
    let (mut app, first) = water(CalcTask::Optimize);
    event(&mut app, optimized(&START, &MOVED));
    let mut newest = app.tab.doc.science.results[0].clone();
    Arc::make_mut(&mut newest.result).energy_hartree = Some(-7.5);
    let expected = Arc::clone(&newest.result);
    app.tab.doc.science.results.push(newest.clone());
    for status in [
        Status::NotConverged {
            reason: "stopped".into(),
        },
        Status::Failed {
            reason: "failed".into(),
        },
    ] {
        let mut stopped = newest.clone();
        Arc::make_mut(&mut stopped.result).status = status;
        app.tab.doc.science.results.push(stopped);
    }
    let other = add_molecule(&mut app);
    let mut attachment = app.tab.doc.science.attachments[0].clone();
    attachment.id += 1;
    attachment.atoms = vec![Some(other), None, None];
    let mut unrelated = newest;
    unrelated.attachment = attachment.id;
    Arc::make_mut(&mut unrelated.result).energy_hartree = Some(-12.);
    app.tab.doc.science.attachments.push(attachment);
    app.tab.xtb.palette.show(&unrelated.result);
    app.tab.doc.science.results.push(unrelated);

    app.tab.selected = vec![first];
    app.xtb_fail(state::Failure::Other("invalid settings".into()));
    app.tab.selected.clear();
    let failed = app.xtb_failed().unwrap();
    let shown = app.xtb_last_successful(failed).unwrap();
    assert_eq!(shown.index, 1);
    assert!(Arc::ptr_eq(&shown.stored.result, &expected));

    for stored in &mut app.tab.doc.science.results[..2] {
        Arc::make_mut(&mut stored.result).status = Status::NotConverged {
            reason: "stopped".into(),
        };
    }
    assert!(app.xtb_last_successful(app.xtb_failed().unwrap()).is_none());
}

#[test]
fn a_failure_without_a_target_only_shows_when_no_molecule_is_targeted() {
    let (mut app, _) = App::new();
    let first = app.tab.doc.add_atom("O", Point::default());
    app.tab.doc.add_atom("O", Point::new(200., 0.));
    app.xtb_fail(state::Failure::Other("invalid settings".into()));
    let failed = app.xtb_failed().expect("no molecule is targeted");
    assert!(failed.atoms.is_empty());
    assert!(app.xtb_last_successful(failed).is_none());
    app.tab.selected = vec![first];
    assert!(app.xtb_outcome().is_none());
}
