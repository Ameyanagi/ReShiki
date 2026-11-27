use super::*;
use reshiki::arrows::Preset;
use reshiki::document::{Arrow, Point};
use reshiki::graphics::{
    EnergyProfileGeometry, EnergyProfileState, EnergyProfileStateKind as Kind, EnergyProfileStep,
};
use reshiki::qcdata::Frame;
use reshiki::reactions::{Participant, Reaction};
use std::sync::Arc;

fn profile(kinds: &[Kind]) -> EnergyProfile {
    let states: Vec<_> = kinds
        .iter()
        .enumerate()
        .map(|(i, &kind)| {
            let (label, energy) = match kind {
                Kind::Reactant => ("Reactant", 0.),
                Kind::TransitionState if i == 1 => ("TS1‡", 27.3),
                Kind::TransitionState => ("TS2‡", 18.5),
                Kind::Intermediate => ("Intermediate", 4.2),
                Kind::Product => ("Product", -8.4),
            };
            EnergyProfileState {
                kind,
                label: label.into(),
                energy_kcal: energy,
                geometry: None,
            }
        })
        .collect();
    let minima: Vec<_> = kinds
        .iter()
        .enumerate()
        .filter_map(|(i, kind)| (*kind != Kind::TransitionState).then_some(i))
        .collect();
    let steps = minima
        .windows(2)
        .filter_map(|pair| {
            let &[from, to] = pair else {
                return None;
            };
            Some(EnergyProfileStep {
                source_key: None,
                from,
                to,
                transition: (to == from + 2).then_some(from + 1),
                irc: vec![],
            })
        })
        .collect();
    EnergyProfile::new(1, "a".repeat(64), "GFN2-xTB".into(), states, steps)
}

pub(in crate::app) fn claisen() -> (Document, u64) {
    // Atom order C1, C2, O3, C4, C5, C6 follows the scheme's map numbers.
    let mut doc = Document::default();
    let left = [
        [145., 38.],
        [116., 55.],
        [87., 38.],
        [58., 55.],
        [29., 38.],
        [0., 55.],
    ];
    let right = [
        [353., 38.],
        [382., 55.],
        [411., 38.],
        [266., 55.],
        [295., 38.],
        [324., 55.],
    ];
    let r_bonds = [[0, 1], [1, 2], [2, 3], [3, 4], [4, 5]];
    let p_bonds = [[0, 1], [1, 2], [0, 5], [3, 4], [4, 5]];
    let mut draw = |points: [[f32; 2]; 6], bonds: [[usize; 2]; 5], doubles| {
        let ids: Vec<_> = points
            .into_iter()
            .enumerate()
            .map(|(i, [x, y])| {
                let id = doc.add_atom(if i == 2 { "O" } else { "C" }, Point::new(x, y));
                doc.atom_mut(id).unwrap().map_num = i as u32 + 1;
                id
            })
            .collect();
        for (i, [a, b]) in bonds.into_iter().enumerate() {
            doc.add_bond(
                ids[a],
                ids[b],
                if doubles & (1 << i) == 0 { 1 } else { 2 },
                "plain",
            );
        }
        ids
    };
    let reactants = draw(left, r_bonds, 0b10001);
    let products = draw(right, p_bonds, 0b01010);
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(168., 48.),
        Point::new(238., 48.),
        Preset::Forward,
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(Participant {
        atoms: reactants.clone(),
        coefficient: 1,
    });
    reaction.products.push(Participant {
        atoms: products,
        coefficient: 1,
    });
    let mut data = profile(&[Kind::Reactant, Kind::TransitionState, Kind::Product]);
    data.source = Some(arrow);
    data.source_key = super::super::xtb::ts::profile::source_key(&doc, &reaction);
    doc.reactions.push(reaction);
    let coords = |points: [[f32; 2]; 6]| {
        points
            .into_iter()
            .map(|[x, y]| [f64::from(x) * 0.045, f64::from(y) * 0.045, 0.])
            .collect()
    };
    let mut saddle = Frame {
        numbers: vec![6, 6, 8, 6, 6, 6],
        positions_angstrom: (0..6)
            .map(|i| {
                let angle = f64::from(i) * std::f64::consts::TAU / 6.;
                [
                    1.55 * angle.cos(),
                    1.55 * angle.sin(),
                    if i % 2 == 0 { 0.4 } else { 0. },
                ]
            })
            .collect(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: reactants.into_iter().map(Some).collect(),
    };
    doc.science.frames.push(Arc::new(saddle.clone()));
    saddle.drawing_atoms.clear();
    let geometry = |coordinates, bonds: Vec<[usize; 2]>| EnergyProfileGeometry {
        frame: Frame {
            positions_angstrom: coordinates,
            ..saddle.clone()
        },
        bonds,
        forming: vec![],
        breaking: vec![],
    };
    data.states[0].geometry = Some(geometry(coords(left), r_bonds.to_vec()));
    data.states[1].geometry = Some(EnergyProfileGeometry {
        forming: vec![[0, 5]],
        breaking: vec![[2, 3]],
        ..geometry(
            saddle.positions_angstrom.clone(),
            vec![[0, 1], [1, 2], [3, 4], [4, 5]],
        )
    });
    data.states[2].geometry = Some(geometry(coords(right), p_bonds.to_vec()));
    let id = doc.next_id();
    let (w, h) = data.natural_size(&doc.drawing_style);
    doc.graphics.push(Graphic::energy_profile(
        id,
        Point::new((411. - w) * 0.5, 96.),
        Point::new(w, h),
        data,
        Default::default(),
    ));
    (doc, id)
}

pub(in crate::app) fn animation_app() -> (App, u64) {
    let (mut doc, id) = claisen();
    let before = doc.clone();
    let _ = reshiki::transaction::reconcile(&mut doc, before).unwrap();
    let (mut app, _) = App::new();
    app.tab.doc = doc.clone();
    app.tab.saved = doc;
    app.appearance.playback_mode = PlaybackMode::Loop;
    app.appearance.playback_speed = PlaybackSpeed::Normal;
    app.set_selection(vec![id]);
    (app, id)
}

fn tick(app: &mut App, deadline: Instant, now: Instant) {
    let _ = app.update(Message::ProfileAnimation(Action::Tick { deadline, now }));
}

pub(in crate::app) fn finish_hold(app: &mut App) {
    let deadline = app.profile_animation.as_ref().unwrap().deadline;
    tick(app, deadline, deadline);
}

fn timer_id(app: &App) -> u64 {
    use iced::advanced::subscription::{Hasher, into_recipes};
    use std::hash::Hasher as _;
    let mut hash = Hasher::default();
    let recipes = into_recipes(app.profile_animation_subscription());
    assert_eq!(recipes.len(), 1);
    recipes[0].hash(&mut hash);
    hash.finish()
}

#[test]
fn start_resume_and_seek_hold_the_current_state_for_the_full_speed_adjusted_duration() {
    for speed in PlaybackSpeed::ALL {
        let (mut app, _) = animation_app();
        app.appearance.playback_speed = speed;
        let hold = Duration::from_secs_f32(0.8 / speed.factor());
        for frame in [0, 1] {
            let started = Instant::now();
            let _ = app.update(Message::ProfileAnimation(Action::Toggle));
            let session = app.profile_animation.as_ref().unwrap();
            let deadline = session.deadline;
            assert_eq!(session.position.frame, frame);
            assert_eq!(session.hold(), hold);
            assert!(deadline >= started + hold);
            // The WASM interval's initial tick and an almost-due tick stay put.
            tick(&mut app, deadline, started);
            tick(&mut app, deadline, deadline - Duration::from_nanos(1));
            assert_eq!(
                app.profile_animation.as_ref().unwrap().position.frame,
                frame
            );

            let _ = app.update(Message::ProfileAnimation(Action::Toggle));
            if frame == 0 {
                let _ = app.update(Message::ProfileAnimation(Action::Step(1)));
            }
        }
        let started = Instant::now();
        let _ = app.update(Message::ProfileAnimation(Action::Toggle));
        let deadline = app.profile_animation.as_ref().unwrap().deadline;
        assert!(deadline >= started + hold);
        tick(&mut app, deadline, deadline);
        let session = app.profile_animation.as_ref().unwrap();
        assert_eq!(session.position.frame, 2);
        assert!(session.deadline >= deadline + hold);
    }
}

#[test]
fn queued_ticks_cannot_advance_a_new_hold_or_a_replacement_session() {
    let (mut app, id) = animation_app();
    let _ = app.update(Message::ProfileAnimation(Action::Toggle));
    let old = Instant::now() + Duration::from_millis(1);
    app.profile_animation.as_mut().unwrap().deadline = old;
    let old_timer = timer_id(&app);
    let _ = app.update(Message::ProfileAnimation(Action::Toggle));
    let _ = app.update(Message::ProfileAnimation(Action::Toggle));
    let current = app.profile_animation.as_ref().unwrap().deadline;
    assert_ne!(timer_id(&app), old_timer);
    tick(&mut app, old, current + Duration::from_secs(10));
    assert_eq!(app.profile_animation.as_ref().unwrap().position.frame, 0);

    tick(&mut app, current, current);
    let next = app.profile_animation.as_ref().unwrap().deadline;
    tick(&mut app, current, next + Duration::from_secs(10));
    assert_eq!(app.profile_animation.as_ref().unwrap().position.frame, 1);

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![])));
    app.set_selection(vec![id]);
    let _ = app.update(Message::ProfileAnimation(Action::Toggle));
    let replacement = app.profile_animation.as_ref().unwrap().deadline;
    tick(&mut app, next, replacement + Duration::from_secs(10));
    assert_eq!(app.profile_animation.as_ref().unwrap().position.frame, 0);
}

#[test]
fn changing_speed_restarts_the_current_state_hold() {
    let (mut app, _) = animation_app();
    let _ = app.update(Message::ProfileAnimation(Action::Toggle));
    let deadline = app.profile_animation.as_ref().unwrap().deadline;
    tick(&mut app, deadline, deadline);
    for speed in [PlaybackSpeed::Half, PlaybackSpeed::Double] {
        let previous = app.profile_animation.as_ref().unwrap().deadline;
        let started = Instant::now();
        app.appearance.playback_speed = speed;
        app.sync_profile_animation();
        let deadline = app.profile_animation.as_ref().unwrap().deadline;
        assert!(deadline >= started + Duration::from_secs_f32(0.8 / speed.factor()));
        tick(&mut app, previous, deadline + Duration::from_secs(10));
        tick(&mut app, deadline, deadline - Duration::from_nanos(1));
        assert_eq!(app.profile_animation.as_ref().unwrap().position.frame, 1);
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn switching_steps_and_smooth_restarts_equal_duration_state_holds() {
    let (mut app, arrow) = ts::tests::finished_irc();
    let _ = app.update(Message::Xtb(xtb::Action::Ts(
        ts::Action::PlaceEnergyProfile(Some(arrow)),
    )));
    app.appearance.playback_speed = PlaybackSpeed::Normal;
    app.appearance.playback_mode = PlaybackMode::Loop;
    let _ = app.update(Message::ProfileAnimation(Action::Toggle));
    assert!(
        app.profile_animation
            .as_ref()
            .unwrap()
            .paths
            .iter()
            .any(Option::is_some)
    );
    for motion in [Motion::Smooth, Motion::Steps] {
        let old = Instant::now() + Duration::from_millis(1);
        app.profile_animation.as_mut().unwrap().deadline = old;
        let old_timer = timer_id(&app);
        let started = Instant::now();
        let _ = app.update(Message::ProfileAnimation(Action::Motion(motion)));
        let session = app.profile_animation.as_ref().unwrap();
        let deadline = session.deadline;
        assert_eq!(session.motion, motion);
        assert!(session.position.playing);
        assert_eq!(session.position.frame, 0);
        assert!(deadline >= started + Duration::from_secs_f32(0.8));
        assert_ne!(timer_id(&app), old_timer);
        tick(&mut app, old, deadline + Duration::from_secs(10));
        tick(&mut app, deadline, deadline - Duration::from_nanos(1));
        assert_eq!(app.profile_animation.as_ref().unwrap().position.frame, 0);
        tick(&mut app, deadline, deadline);
        assert_eq!(app.profile_animation.as_ref().unwrap().position.frame, 1);
    }
}

#[test]
fn no_op_canvas_move_and_resize_pause_playback_without_desynchronizing_the_overlay() {
    for resize in [false, true] {
        let (mut app, id) = animation_app();
        let _ = app.update(Message::ProfileAnimation(Action::Toggle));
        let deadline = app.profile_animation.as_ref().unwrap().deadline;
        tick(&mut app, deadline, deadline);
        let session = app.profile_animation.as_ref().unwrap();
        let frame = session.position.frame;
        let deadline = session.deadline;
        let ink = format!("{:?}", app.profile_animation_overlay().unwrap().ink);
        let doc = app.tab.doc.clone();
        let revision = app.tab.revision;
        let groups = crate::canvas::profile_animation_gesture(
            &app.tab.doc,
            id,
            app.profile_animation_overlay().unwrap(),
            resize,
            false,
        );
        assert!(
            groups[0]
                .iter()
                .any(|edit| matches!(edit, crate::canvas::Edit::PauseProfileAnimation))
        );
        for (phase, edits) in groups.into_iter().enumerate() {
            for edit in edits {
                let _ = app.update(Message::Canvas(edit));
            }
            tick(&mut app, deadline, deadline + Duration::from_secs(10));
            let session = app.profile_animation.as_ref().unwrap_or_else(|| {
                panic!("Playback session ended at gesture phase {phase}, resize: {resize}")
            });
            assert!(!session.position.playing);
            assert_eq!(session.position.frame, frame);
            assert_eq!(
                format!("{:?}", app.profile_animation_overlay().unwrap().ink),
                ink
            );
        }
        assert_eq!(app.tab.doc, doc);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
        assert!(!app.dirty());
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(windows), feature = "xtb"))]
#[test]
fn moving_and_resizing_a_profile_preserves_the_smooth_session() {
    let (mut app, arrow) = ts::tests::finished_irc();
    let _ = app.update(Message::Xtb(xtb::Action::Ts(
        ts::Action::PlaceEnergyProfile(Some(arrow)),
    )));
    let id = app.selected_energy_profile().unwrap().id;
    let play = || Message::ProfileAnimation(Action::Toggle);
    let _ = app.update(Message::ProfileAnimation(Action::Motion(Motion::Smooth)));
    let _ = app.update(play());
    finish_hold(&mut app);
    let session = app.current_profile_animation().unwrap();
    let frame = session.position.frame;
    let sample = session.timeline.frame(frame).unwrap().sample().unwrap();

    for resize in [false, true] {
        let before = app.selected_energy_profile().unwrap().clone();
        let deadline = app.current_profile_animation().unwrap().deadline;
        let revision = app.tab.revision;
        let animation = app.profile_animation_overlay().unwrap();
        let ink = format!("{:?}", animation.ink);
        let gestures =
            crate::canvas::profile_animation_gesture(&app.tab.doc, id, animation, resize, true);
        for edits in gestures {
            for edit in edits {
                let _ = app.update(Message::Canvas(edit));
            }
            tick(&mut app, deadline, deadline + Duration::from_secs(10));
            let session = app.current_profile_animation().unwrap();
            assert!(!session.position.playing);
            assert_eq!(session.motion, Motion::Smooth);
            assert_eq!(session.position.frame, frame);
            assert_eq!(
                session.timeline.frame(frame).unwrap().sample(),
                Some(sample)
            );
        }
        let graphic = app.selected_energy_profile().unwrap();
        if resize {
            assert_ne!(graphic.axis_x, before.axis_x);
        } else {
            assert_ne!(graphic.origin, before.origin);
        }
        assert_eq!(app.tab.revision, revision + 1);
        assert_ne!(
            format!("{:?}", app.profile_animation_overlay().unwrap().ink),
            ink
        );
        let _ = app.update(play());
        let position = &app.current_profile_animation().unwrap().position;
        assert!(position.playing);
        assert_eq!(position.frame, frame);
    }
    finish_hold(&mut app);
    assert_eq!(
        app.current_profile_animation().unwrap().position.frame,
        frame + 1
    );

    let _ = app.update(ts::profile::edit_message(
        id,
        ts::profile::Edit::Models(reshiki::graphics::EnergyProfileModels::Off),
    ));
    assert!(app.profile_animation.is_none());
    let _ = app.update(play());
    assert_eq!(
        app.current_profile_animation().unwrap().motion,
        Motion::Smooth
    );
}

#[test]
fn steps_play_the_whole_profile_in_each_repeat_mode() {
    use Kind::{Intermediate as I, Product as P, Reactant as R, TransitionState as Ts};
    for kinds in [&[R][..], &[R, Ts, P], &[R, Ts, I, Ts, P]] {
        let data = profile(kinds);
        let timeline = Timeline::new(&data, &[], Motion::Steps);
        for repeat in PlaybackMode::ALL {
            let len = kinds.len();
            let mut playback = Playback::default();
            let mut first_pass = vec![];
            for state in 0..len {
                let frame = timeline.frame(playback.frame).unwrap();
                assert_eq!(frame.seconds, 0.8);
                first_pass.push(frame.state);
                if state + 1 < len {
                    assert!(playback.advance(repeat, len));
                }
            }
            assert_eq!(first_pass, (0..len).collect::<Vec<_>>());
            playback.advance(repeat, len);
            assert_eq!(
                playback.frame,
                match repeat {
                    PlaybackMode::Loop => 0,
                    PlaybackMode::BackAndForth => len.saturating_sub(2),
                    PlaybackMode::Once => len - 1,
                }
            );
            assert_eq!(playback.playing, len > 1 && repeat != PlaybackMode::Once);
        }
    }
}

#[test]
fn readout_uses_an_unnumbered_label_for_a_single_ts_and_the_physical_irc_coordinate() {
    let data = profile(&[Kind::Reactant, Kind::TransitionState, Kind::Product]);
    assert_eq!(
        timeline::readout(&data, 0, None, None),
        "Reactant · 0.0 kcal/mol"
    );
    assert_eq!(
        timeline::readout(&data, 1, None, None),
        "TS‡ · 27.3 kcal/mol"
    );
    assert_eq!(
        timeline::readout(&data, 1, Some(12.34), Some(-2.21)),
        "TS‡ · 12.3 kcal/mol · s = −2.2 amu½·bohr"
    );
}

#[test]
fn readout_numbers_multiple_ts_with_unnumbered_authored_labels() {
    use Kind::{Intermediate as I, Product as P, Reactant as R, TransitionState as Ts};
    let mut data = profile(&[R, Ts, I, Ts, P]);
    for state in data.states.iter_mut().filter(|state| state.kind == Ts) {
        state.label = "TS‡".into();
    }
    assert_eq!(
        timeline::readout(&data, 1, None, None),
        "TS1‡ · 27.3 kcal/mol"
    );
    assert_eq!(
        timeline::readout(&data, 3, None, None),
        "TS2‡ · 18.5 kcal/mol"
    );
}

#[test]
fn play_and_pause_are_view_only_and_a_deselection_edit_or_tab_change_stops_them() {
    let (doc, id) = claisen();
    let (mut app, _) = App::new();
    app.tab.doc = doc.clone();
    app.tab.saved = doc.clone();
    app.set_selection(vec![id]);
    let before = app.tab.revision;
    let play = || Message::ProfileAnimation(Action::Toggle);
    let _ = app.update(play());
    finish_hold(&mut app);
    assert_eq!(app.profile_animation.as_ref().unwrap().position.frame, 1);
    assert!(app.profile_animation_overlay().is_some());
    let _ = app.update(play());
    let _ = app.update(Message::ProfileAnimation(Action::Step(1)));
    assert_eq!(app.profile_animation.as_ref().unwrap().position.frame, 2);
    assert_eq!(app.tab.doc, doc);
    assert_eq!(app.tab.revision, before);
    assert!(!app.tab.history.can_undo());
    assert!(!app.dirty());

    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(vec![])));
    assert!(app.profile_animation.is_none());

    app.set_selection(vec![id]);
    let _ = app.update(play());
    let _ = app.update(super::super::xtb::ts::profile::edit_message(
        id,
        super::super::xtb::ts::profile::Edit::Models(reshiki::graphics::EnergyProfileModels::Off),
    ));
    assert!(app.profile_animation.is_none());
    let _ = app.update(play());
    let _ = app.new_document();
    assert!(app.profile_animation.is_none());
}
