use super::*;
use crate::appearance::EnergyUnit;
use reshiki::document::Point;
use reshiki::graphics::{
    BracketSides, EnergyProfileModels, EnergyProfileStateKind as Kind, Graphic, GraphicKind,
};
use reshiki::scene;

fn place(app: &mut App, arrow: u64) -> u64 {
    send(app, XtbAction::Ts(Action::PlaceEnergyProfile(Some(arrow))));
    let [id] = app.tab.selected.as_slice() else {
        panic!("{}", app.status);
    };
    assert!(
        app.tab
            .doc
            .graphics
            .iter()
            .any(|g| g.id == *id && g.energy_profile.is_some()),
        "{}",
        app.status
    );
    *id
}

#[test]
fn placed_profile_sits_below_the_model_row_and_skips_an_obstacle() {
    let (mut app, arrow) = irc_profile::finished();
    let doc = &mut app.tab.doc;
    let row_bottom = doc
        .models
        .iter()
        .filter_map(|model| reshiki::models3d::frame(doc, model).map(|(_, hi)| hi.y))
        .reduce(f32::max)
        .unwrap();
    let obstacle = doc.next_id();
    doc.graphics.push(Graphic::dragged(
        obstacle,
        GraphicKind::Rectangle,
        Point::new(-100., row_bottom + 50.),
        Point::new(180., row_bottom + 100.),
        Default::default(),
        BracketSides::Both,
        false,
    ));
    let before = app.tab.doc.clone();
    let id = place(&mut app, arrow);
    let doc = &app.tab.doc;
    let (lo, _) = scene::selection_bounds(doc, &[id]).unwrap();
    let (_, obstacle_hi) = scene::selection_bounds(doc, &[obstacle]).unwrap();
    assert!(lo.y > row_bottom && lo.y > obstacle_hi.y);
    let p = doc
        .graphics
        .last()
        .unwrap()
        .energy_profile
        .as_ref()
        .unwrap();
    assert_eq!(
        p.states.iter().map(|s| s.kind).collect::<Vec<_>>(),
        [Kind::Reactant, Kind::TransitionState, Kind::Product]
    );
    assert_eq!(p.states[0].label, "Allyl vinyl ether");
    assert_eq!(p.states[1].label, "TS1‡");
    assert!(p.states.iter().all(|s| s.geometry.is_some()));
    assert_eq!(p.steps[0].irc.len(), 5);
    assert_eq!(p.steps[0].irc[2].energy_kcal, p.states[1].energy_kcal);

    select(&mut app, arrow);
    send(&mut app, XtbAction::Ts(Action::PlaceEnergyProfile(None)));
    assert_eq!(app.tab.selected, [id]);
    assert_eq!(app.tab.doc.graphics.len(), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn rerun_updates_the_profile_with_one_undo_and_keeps_its_frame_and_style() {
    let (mut app, arrow) = irc_profile::finished();
    let id = place(&mut app, arrow);
    send(
        &mut app,
        XtbAction::Ts(Action::EditEnergyProfile(
            id,
            profile::Edit::Models(EnergyProfileModels::RowAbove),
        )),
    );
    let before = app.tab.doc.clone();
    let g = app.tab.doc.graphics.last_mut().unwrap();
    g.origin = g.origin.offset(30., 15.);
    g.axis_x.x *= 1.2;
    g.axis_y.y *= 0.9;
    g.style.width_pt = 1.4;
    app.changed(before);
    select(&mut app, arrow);
    let before = app.tab.doc.clone();
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    if app.tab.model_job.is_some() {
        finish_model(&mut app);
    }
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = Arc::unwrap_or_clone(example_report(run.launch.as_ref().unwrap()));
    report.result.energy_hartree = Some(-4.93);
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(Arc::new(report)))),
    );

    let g = app.tab.doc.graphics.last().unwrap();
    let previous = before.graphics.last().unwrap();
    let p = g.energy_profile.as_ref().unwrap();
    assert_eq!(g.id, id);
    assert_eq!(
        (g.origin, g.axis_x, g.axis_y, &g.style),
        (
            previous.origin,
            previous.axis_x,
            previous.axis_y,
            &previous.style
        )
    );
    assert_eq!(p.models, EnergyProfileModels::RowAbove);
    assert!((p.states[1].energy_kcal - 0.07 * reshiki::qcdata::KCAL_PER_HARTREE).abs() < 1e-8);
    assert!(p.steps[0].irc.is_empty());
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn units_follow_the_live_link_and_source_changes_leave_the_snapshot() {
    let (mut app, arrow) = irc_profile::finished();
    app.appearance.energy_unit = EnergyUnit::KcalPerMol;
    let id = place(&mut app, arrow);
    let original = app.tab.doc.clone();
    app.update_energy_profile_units(EnergyUnit::KjPerMol);
    let current = app.tab.doc.graphics.last().unwrap();
    assert_eq!(
        current.energy_profile.as_ref().unwrap().unit,
        EnergyUnit::KjPerMol
    );
    let older = original.graphics.last().unwrap();
    assert_eq!(
        (current.origin, current.axis_x, current.axis_y),
        (older.origin, older.axis_x, older.axis_y)
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);

    let product = app.tab.doc.reactions[0].products[0].atoms[0];
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(product).unwrap().charge = 1;
    app.changed(before);
    let p = app
        .tab
        .doc
        .graphics
        .last()
        .unwrap()
        .energy_profile
        .as_ref()
        .unwrap();
    assert!(p.linked);
    assert!(!profile::source_current(&app.tab.doc, p));
    let before = app.tab.doc.clone();
    app.update_energy_profile_units(EnergyUnit::Ev);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Undo);

    select(&mut app, arrow);
    let _ = app.update(Message::Delete);
    let p = app
        .tab
        .doc
        .graphics
        .last()
        .unwrap()
        .energy_profile
        .as_ref()
        .unwrap();
    assert!(p.linked);
    assert!(!profile::source_current(&app.tab.doc, p));
    assert_eq!(
        p.states,
        original
            .graphics
            .last()
            .unwrap()
            .energy_profile
            .as_ref()
            .unwrap()
            .states
    );
    let _ = app.update(Message::Undo);

    send(
        &mut app,
        XtbAction::Ts(Action::EditEnergyProfile(id, profile::Edit::Unlink)),
    );
    let frozen = app.tab.doc.graphics.last().unwrap().clone();
    assert!(!frozen.energy_profile.as_ref().unwrap().linked);
    app.update_energy_profile_units(EnergyUnit::Ev);
    select(&mut app, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    if app.tab.model_job.is_some() {
        finish_model(&mut app);
    }
    complete(&mut app);
    assert_eq!(app.tab.doc.graphics.last(), Some(&frozen));
}
