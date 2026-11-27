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
    assert_eq!(p.states[1].label, "TS‡");
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
fn reassigned_reactant_keeps_the_finished_rerun_off_the_card_and_profile() {
    let (mut app, arrow) = irc_profile::finished();
    let id = place(&mut app, arrow);
    let placed = app.tab.doc.graphics.last().unwrap().clone();
    select(&mut app, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    if app.tab.model_job.is_some() {
        finish_model(&mut app);
    }

    let before = app.tab.doc.clone();
    let original = &before.reactions[0].reactants[0].atoms;
    let mut copy = reshiki::editing::selection(&before, original);
    copy.models.clear();
    copy.science = Default::default();
    let copies: Vec<_> =
        reshiki::editing::append_preserving_layout(&mut app.tab.doc, &copy, Point::new(0., -200.))
            .into_iter()
            .filter(|id| app.tab.doc.atom(*id).is_some())
            .collect();
    assert_eq!(copies.len(), original.len());
    app.tab.doc.reactions[0].reactants[0].atoms = copies;
    app.changed(before);
    complete(&mut app);

    let doc = &app.tab.doc;
    assert_eq!(doc.science.results.len(), 2);
    let stored = doc.science.results.last().unwrap();
    let attached = doc.science.attachment(stored.attachment).unwrap();
    assert_eq!(
        attached.freshness(doc),
        reshiki::science::Freshness::Current
    );
    assert_eq!(placement_arrow(doc, &stored.result), Some(arrow));
    assert!(app.xtb_resolve_reaction(doc, arrow, None).result.is_none());
    assert_eq!(doc.graphics.iter().find(|g| g.id == id), Some(&placed));
}

#[test]
fn equivalent_map_relabeling_keeps_the_link_but_changed_pairs_do_not() {
    let (mut app, arrow) = irc_profile::finished();
    let id = place(&mut app, arrow);
    let original = app.tab.doc.graphics.last().unwrap().clone();
    let before = app.tab.doc.clone();
    for atom in &mut app.tab.doc.atoms {
        if atom.map_num == 1 {
            atom.map_num = 17;
        }
    }
    app.changed(before);

    assert!(profile::source_current(
        &app.tab.doc,
        original.energy_profile.as_ref().unwrap()
    ));
    assert_eq!(place(&mut app, arrow), id);
    select(&mut app, arrow);
    send(&mut app, XtbAction::Ts(Action::Calculate(arrow)));
    let run = app.tab.xtb.run.as_ref().unwrap();
    let serial = run.serial;
    let mut report = Arc::unwrap_or_clone(example_report(run.launch.as_ref().unwrap()));
    report.result.energy_hartree = Some(-4.93);
    send(
        &mut app,
        XtbAction::TsEvent(serial, Event::Done(Ok(Arc::new(report)))),
    );
    let current = app.tab.doc.graphics.last().unwrap().clone();
    assert_eq!(current.id, id);
    assert_ne!(current, original);
    assert_eq!(place(&mut app, arrow), id);
    assert_eq!(app.tab.doc.graphics.len(), 1);

    let products = app.tab.doc.reactions[0].products[0].atoms.clone();
    for atom in &mut app.tab.doc.atoms {
        if products.contains(&atom.id) {
            atom.map_num = match atom.map_num {
                17 => 2,
                2 => 17,
                number => number,
            };
        }
    }
    assert!(!profile::source_current(
        &app.tab.doc,
        current.energy_profile.as_ref().unwrap()
    ));
}

#[test]
fn placed_profile_clears_a_zero_order_bond_from_the_scheme() {
    let (mut app, arrow) = irc_profile::finished();
    let id = place(&mut app, arrow);
    let (lo, hi) = scene::selection_bounds(&app.tab.doc, &[id]).unwrap();
    let _ = app.update(Message::Undo);
    let doc = &mut app.tab.doc;
    let product = *doc.reactions[0].products[0].atoms.last().unwrap();
    // Contacts on a calculated endpoint make the saved search stale.
    let a = doc.atom(product).unwrap().position.offset(0., -30.);
    let agent = doc.add_atom("N", a);
    doc.reactions[0]
        .agents
        .push(reshiki::reactions::Participant {
            atoms: vec![agent],
            coefficient: 1,
        });
    let y = (lo.y + hi.y) / 2.;
    let x = (lo.x + hi.x) / 2.;
    let rise = 3. * (hi.y - a.y);
    let b = Point::new(a.x + (x - a.x) * rise / (y - a.y), a.y + rise);
    assert!(b.x < lo.x || b.x > hi.x);
    let outside = doc.add_atom("O", b);
    doc.add_bond(agent, outside, 0, "dotted");

    let id = place(&mut app, arrow);
    let (placed, _) = scene::selection_bounds(&app.tab.doc, &[id]).unwrap();
    assert!(placed.y > b.y);
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

#[test]
fn smooth_profile_uses_saved_frames_and_includes_each_state_in_the_movie() {
    use crate::app::{
        animation::PlaybackMode,
        profile_animation::{Action as Play, Motion, Timeline},
    };
    let (mut app, arrow) = irc_profile::finished();
    let id = place(&mut app, arrow);
    let graphic = app.selected_energy_profile().unwrap();
    let profile = graphic.energy_profile.as_deref().unwrap();
    let paths = app.energy_profile_paths(profile);
    assert!(paths[0].is_some());
    let timeline = Timeline::new(profile, &paths, Motion::Smooth);
    let traced = timeline.frame(1).unwrap();
    assert!(timeline.geometry(traced).is_some());
    assert!(traced.sample().unwrap().1.x < 0.5);
    let first_pass = timeline.sequence(20, PlaybackMode::Once, 1.);
    assert_eq!(first_pass.len(), 37);
    assert_eq!(first_pass.first(), Some(&0));
    assert_eq!(first_pass.last(), Some(&4));
    let state_indices: Vec<_> = first_pass
        .iter()
        .map(|&index| timeline.frame(index).unwrap().state)
        .collect();
    assert!(state_indices.contains(&0) && state_indices.contains(&1) && state_indices.contains(&2));
    assert_eq!(timeline.sequence(20, PlaybackMode::Loop, 1.), first_pass);
    assert_eq!(
        timeline.sequence(20, PlaybackMode::BackAndForth, 1.).len(),
        42
    );

    let mut multistep = profile.clone();
    let mut second_ts = profile.states[1].clone();
    second_ts.label = "TS2‡".into();
    multistep.states[2].kind = Kind::Intermediate;
    multistep
        .states
        .extend([second_ts, profile.states[2].clone()]);
    multistep.steps.push(reshiki::graphics::EnergyProfileStep {
        from: 2,
        to: 4,
        transition: Some(3),
        source: None,
        source_key: None,
        irc: vec![],
    });
    let timeline = Timeline::new(&multistep, &paths, Motion::Smooth);
    for repeat in PlaybackMode::ALL {
        let mut states: Vec<_> = timeline
            .sequence(20, repeat, 1.)
            .iter()
            .map(|&index| timeline.frame(index).unwrap().state)
            .collect();
        states.dedup();
        if repeat == PlaybackMode::BackAndForth {
            assert_eq!(states, [0, 1, 2, 3, 4, 3, 2, 1, 0]);
        } else {
            assert_eq!(states, [0, 1, 2, 3, 4]);
        }
    }

    let (doc, revision, history) = (
        app.tab.doc.clone(),
        app.tab.revision,
        app.tab.history.can_undo(),
    );
    let _ = app.update(Message::ProfileAnimation(Play::Motion(Motion::Smooth)));
    let _ = app.update(Message::ProfileAnimation(Play::Toggle));
    crate::app::profile_animation::tests::finish_hold(&mut app);
    assert_eq!(
        app.profile_animation_overlay().unwrap().hidden_picture,
        Some((id, 1))
    );
    assert_eq!(app.tab.doc, doc);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.history.can_undo(), history);

    let mut full = app.tab.doc.clone();
    Arc::make_mut(&mut full.science.results[0].result).keep_full = true;
    let mut reopened = reaction_app(
        Document::from_native_file(&full.file_json().unwrap()).unwrap(),
        arrow,
    );
    reopened.set_selection(vec![id]);
    let profile = reopened
        .selected_energy_profile()
        .unwrap()
        .energy_profile
        .as_deref()
        .unwrap();
    assert!(reopened.energy_profile_paths(profile)[0].is_some());

    let result = Arc::make_mut(&mut app.tab.doc.science.results[0].result);
    result
        .run
        .as_mut()
        .unwrap()
        .transition_state
        .as_mut()
        .unwrap()
        .irc
        .as_mut()
        .unwrap()
        .trajectory
        .positions_angstrom
        .clear();
    let profile = app
        .selected_energy_profile()
        .unwrap()
        .energy_profile
        .as_deref()
        .unwrap();
    assert!(
        app.energy_profile_paths(profile)
            .iter()
            .all(Option::is_none)
    );
}
