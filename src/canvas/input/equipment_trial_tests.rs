use super::*;
use crate::canvas::{Camera, EquipmentAuthoring, Navigation, tests::chain_canvas};
use iced::widget::canvas::Program;
use reshiki::{
    chains::ChainMode,
    document::Document,
    graphics::{Graphic, GraphicKind},
};

fn drawing() -> Document {
    let mut doc = Document::default();
    doc.graphics.push(Graphic::dragged(
        1,
        GraphicKind::Rectangle,
        World::new(-40., -30.),
        World::new(40., 30.),
        Default::default(),
        Default::default(),
        false,
    ));
    doc
}

fn send(
    canvas: &MoleculeCanvas<'_>,
    state: &mut State,
    bounds: Rectangle,
    event: Event,
    pointer: Point,
) -> Option<Edit> {
    canvas
        .update(state, &event, bounds, mouse::Cursor::Available(pointer))
        .and_then(|a| a.into_inner().0)
}

fn cancelled(edit: Option<Edit>, epoch: u64) {
    let Some(Edit::Touch(edits)) = edit else {
        panic!("Expected cancellation")
    };
    assert!(matches!(edits.as_slice(), [Edit::EquipmentAuthoringTest {
        at, epoch: e, finish: false, .. }, Edit::Hover(None)] if *e == epoch && *at == World::default()));
}

#[test]
fn equipment_trial_mouse_owns_the_test_piece_and_preserves_grab_offset_at_each_zoom() {
    let doc = drawing();
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(600., 400.));
    for rulers in [false, true] {
        for zoom in [0.5, 2.] {
            let mut canvas = chain_canvas(&doc, ChainMode::Straight);
            canvas.guides.rulers = rulers;
            canvas.camera = Camera {
                zoom,
                center: World::new(15., -20.),
            };
            canvas.equipment_authoring = Some(EquipmentAuthoring {
                trial: Some((1, World::default(), 23)),
                ..Default::default()
            });
            let paper = canvas.guides.paper(bounds);
            let pointer = canvas.camera.screen(World::new(40., 10.), paper)
                + iced::Vector::new(paper.x, paper.y);
            let end = pointer + iced::Vector::new(30., -16.);
            let mut state = State::default();
            assert!(matches!(
                send(
                    &canvas,
                    &mut state,
                    bounds,
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    pointer
                ),
                Some(Edit::EquipmentAuthoringTestStart(23))
            ));
            assert!(state.gesture.is_none());
            let expected = World::new(30. / zoom, -16. / zoom);
            for (event, finish) in [
                (mouse::Event::CursorMoved { position: end }, false),
                (mouse::Event::ButtonReleased(mouse::Button::Left), true),
            ] {
                let Some(Edit::EquipmentAuthoringTest {
                    at,
                    epoch,
                    reach,
                    finish: actual,
                }) = send(&canvas, &mut state, bounds, Event::Mouse(event), end)
                else {
                    panic!("Test drag owns its input")
                };
                assert!(at.distance(expected) < 0.001, "{at:?}, {expected:?}");
                assert_eq!(epoch, 23);
                assert!((reach - 14. / zoom).abs() < 0.001);
                assert_eq!(actual, finish);
            }
            assert!(state.equipment_trial.is_none());
            let away = Point::new(paper.x + 10., paper.y + 10.);
            state.cursor = None;
            assert!(
                send(
                    &canvas,
                    &mut state,
                    bounds,
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    away
                )
                .is_none()
            );
            assert!(state.equipment_trial.is_none());
        }
    }
}

#[test]
fn equipment_trial_mouse_cancellation_and_navigation_keep_separate_ownership() {
    let doc = drawing();
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(600., 400.));
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.equipment_authoring = Some(EquipmentAuthoring {
        trial: Some((1, World::default(), 23)),
        ..Default::default()
    });
    let p =
        canvas.camera.screen(World::new(40., 10.), bounds) + iced::Vector::new(bounds.x, bounds.y);
    for event in [
        Event::Mouse(mouse::Event::CursorLeft),
        Event::Window(iced::window::Event::Unfocused),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)),
    ] {
        let mut state = State::default();
        send(
            &canvas,
            &mut state,
            bounds,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            p,
        );
        for neutral in [
            mouse::Event::ButtonReleased(mouse::Button::Right),
            mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0., y: 1. },
            },
        ] {
            assert!(send(&canvas, &mut state, bounds, Event::Mouse(neutral), p).is_none());
            assert!(state.equipment_trial.is_some());
        }
        let pan = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle))
        );
        cancelled(send(&canvas, &mut state, bounds, event, p), 23);
        assert!(state.equipment_trial.is_none());
        assert_eq!(matches!(state.gesture, Some(Gesture::Pan { .. })), pan);
    }
    let mut state = State::default();
    assert!(matches!(
        send(
            &canvas,
            &mut state,
            bounds,
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0., y: 1. }
            }),
            p
        ),
        Some(Edit::Navigate(Navigation::Pan(_), _))
    ));
    send(
        &canvas,
        &mut state,
        bounds,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        p,
    );
    // A replaced draft cannot receive the rest of an earlier drag.
    canvas
        .equipment_authoring
        .as_mut()
        .unwrap()
        .trial
        .as_mut()
        .unwrap()
        .2 = 24;
    send(
        &canvas,
        &mut state,
        bounds,
        Event::Mouse(mouse::Event::CursorMoved { position: p }),
        p,
    );
    assert!(state.equipment_trial.is_none());
    send(
        &canvas,
        &mut state,
        bounds,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        p,
    );
    let outside = Point::new(0., 0.);
    send(
        &canvas,
        &mut state,
        bounds,
        Event::Mouse(mouse::Event::CursorMoved { position: outside }),
        outside,
    );
    cancelled(
        send(
            &canvas,
            &mut state,
            bounds,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            outside,
        ),
        24,
    );
}
