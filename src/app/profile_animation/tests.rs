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
fn readout_uses_the_authored_state_and_the_physical_irc_coordinate() {
    let data = profile(&[Kind::Reactant, Kind::TransitionState, Kind::Product]);
    assert_eq!(
        timeline::readout(&data, 0, None, None),
        "Reactant · 0.0 kcal/mol"
    );
    assert_eq!(
        timeline::readout(&data, 1, None, None),
        "TS1‡ · 27.3 kcal/mol"
    );
    assert_eq!(
        timeline::readout(&data, 1, Some(12.34), Some(-2.21)),
        "TS1‡ · 12.3 kcal/mol · s = −2.2 amu½·bohr"
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
    let _ = app.update(Message::ProfileAnimation(Action::Tick));
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
