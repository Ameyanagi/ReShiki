use super::*;
use crate::canvas::{Camera, Tool, World, tests::chain_canvas};
use iced::widget::canvas::Program;
use reshiki::{chains::ChainMode, document::Document};

fn edits(action: Option<Action<Edit>>) -> Vec<Edit> {
    match action.and_then(|action| action.into_inner().0) {
        Some(Edit::Touch(edits)) => edits,
        Some(edit) => vec![edit],
        None => vec![],
    }
}

fn send(
    canvas: &MoleculeCanvas<'_>,
    state: &mut State,
    bounds: Rectangle,
    event: touch::Event,
) -> Vec<Edit> {
    // Touch positions must work even when the mouse cursor is elsewhere.
    edits(canvas.update(
        state,
        &Event::Touch(event),
        bounds,
        mouse::Cursor::Available(Point::new(-100., -100.)),
    ))
}

#[test]
fn touch_taps_and_drags_match_mouse_edits_for_drawing_selection_and_erasing() {
    let mut doc = Document::default();
    doc.add_atom("C", World::new(-60., -30.));
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(500., 350.));
    for tool in [
        Tool::Bond(1),
        Tool::Chain(ChainMode::Straight),
        Tool::Atom,
        Tool::Select,
        Tool::Lasso,
        Tool::Erase,
    ] {
        for rulers in [false, true] {
            for zoom in [0.5, 2.] {
                let mut canvas = chain_canvas(&doc, ChainMode::Straight);
                canvas.tool = tool;
                canvas.guides.rulers = rulers;
                canvas.camera.zoom = zoom;
                let paper = canvas.guides.paper(bounds);
                let start = canvas.camera.screen(World::new(-60., -30.), paper)
                    + Vector::new(paper.x, paper.y);
                for delta in [Vector::ZERO, Vector::new(70., 20.)] {
                    let end = start + delta;
                    let mut mouse_state = State::default();
                    let mouse_edits: Vec<_> = [
                        mouse::Event::CursorMoved { position: start },
                        mouse::Event::ButtonPressed(mouse::Button::Left),
                        mouse::Event::CursorMoved { position: end },
                        mouse::Event::ButtonReleased(mouse::Button::Left),
                    ]
                    .into_iter()
                    .flat_map(|event| {
                        edits(canvas.update(
                            &mut mouse_state,
                            &Event::Mouse(event),
                            bounds,
                            mouse::Cursor::Unavailable,
                        ))
                    })
                    .filter(|edit| !edit.is_hover())
                    .collect();
                    let mut state = State::default();
                    let touch_edits: Vec<_> = [
                        touch::Event::FingerPressed {
                            id: touch::Finger(7),
                            position: start,
                        },
                        touch::Event::FingerMoved {
                            id: touch::Finger(7),
                            position: end,
                        },
                        touch::Event::FingerLifted {
                            id: touch::Finger(7),
                            position: end,
                        },
                    ]
                    .into_iter()
                    .flat_map(|event| send(&canvas, &mut state, bounds, event))
                    .filter(|edit| !edit.is_hover())
                    .collect();
                    // Erasing also sends the final segment on lift; it is a
                    // zero-length continuation when the last motion was delivered.
                    let touch_edits: Vec<_> = touch_edits
                        .into_iter()
                        .filter(|edit| !matches!(edit, Edit::EraseTo(a, b) if a == b))
                        .collect();
                    let mouse_edits: Vec<_> = mouse_edits
                        .into_iter()
                        .filter(|edit| !matches!(edit, Edit::EraseTo(a, b) if a == b))
                        .collect();
                    assert_eq!(
                        format!("{touch_edits:?}"),
                        format!("{mouse_edits:?}"),
                        "{tool:?}, rulers={rulers}, zoom={zoom}, delta={delta:?}"
                    );
                    assert!(state.gesture.is_none());
                    assert!(state.touch.fingers.is_empty());
                    assert!(state.cursor.is_none());
                }
            }
        }
    }
}

#[test]
fn touch_release_uses_its_own_position_and_lost_releases_a_started_drag() {
    let doc = Document::default();
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.tool = Tool::Bond(1);
    canvas.bond_drawing.fixed_length = false;
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    for lost in [false, true] {
        let mut state = State::default();
        send(
            &canvas,
            &mut state,
            bounds,
            touch::Event::FingerPressed {
                id: touch::Finger(1),
                position: Point::new(200., 150.),
            },
        );
        send(
            &canvas,
            &mut state,
            bounds,
            touch::Event::FingerMoved {
                id: touch::Finger(1),
                position: Point::new(230., 150.),
            },
        );
        let position = Point::new(260., 150.);
        let event = if lost {
            touch::Event::FingerLost {
                id: touch::Finger(1),
                position,
            }
        } else {
            touch::Event::FingerLifted {
                id: touch::Finger(1),
                position,
            }
        };
        let result = send(&canvas, &mut state, bounds, event);
        assert!(
            matches!(result.last(), Some(Edit::Bond(start, end, None, None)) if *start == World::default() && end.x > 50.)
        );
        assert!(state.gesture.is_none());
        assert!(state.touch.fingers.is_empty());
    }
}

#[test]
fn second_finger_cancels_drawing_and_remaining_finger_cannot_resume_it() {
    let doc = Document::default();
    let canvas = chain_canvas(&doc, ChainMode::Straight);
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    for move_first in [false, true] {
        for first_lifted in [1, 2] {
            let mut state = State::default();
            send(
                &canvas,
                &mut state,
                bounds,
                touch::Event::FingerPressed {
                    id: touch::Finger(1),
                    position: Point::new(120., 150.),
                },
            );
            if move_first {
                send(
                    &canvas,
                    &mut state,
                    bounds,
                    touch::Event::FingerMoved {
                        id: touch::Finger(1),
                        position: Point::new(150., 150.),
                    },
                );
                assert!(state.gesture.is_some());
            }
            let result = send(
                &canvas,
                &mut state,
                bounds,
                touch::Event::FingerPressed {
                    id: touch::Finger(2),
                    position: Point::new(250., 150.),
                },
            );
            assert!(result.iter().all(Edit::is_hover));
            assert!(state.gesture.is_none());
            assert!(state.touch.deadline().is_none());
            send(
                &canvas,
                &mut state,
                bounds,
                touch::Event::FingerLifted {
                    id: touch::Finger(first_lifted),
                    position: Point::new(250., 150.),
                },
            );
            let remaining = touch::Finger(3 - first_lifted);
            assert!(
                send(
                    &canvas,
                    &mut state,
                    bounds,
                    touch::Event::FingerMoved {
                        id: remaining,
                        position: Point::new(220., 170.)
                    }
                )
                .is_empty()
            );
            assert!(
                send(
                    &canvas,
                    &mut state,
                    bounds,
                    touch::Event::FingerLifted {
                        id: remaining,
                        position: Point::new(220., 170.)
                    }
                )
                .is_empty()
            );
            assert!(state.touch.fingers.is_empty());
        }
    }
}

#[test]
fn long_press_opens_context_without_drawing_and_movement_cancels_timer() {
    let doc = Document::default();
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(400., 300.));
    for tool in [Tool::Atom, Tool::Erase, Tool::Bond(1)] {
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = tool;
        canvas.guides.rulers = true;
        let position = Point::new(200., 180.);
        let mut state = State::default();
        assert!(
            send(
                &canvas,
                &mut state,
                bounds,
                touch::Event::FingerPressed {
                    id: touch::Finger(1),
                    position
                }
            )
            .is_empty()
        );
        let deadline = state.touch.deadline().unwrap();
        let early = canvas
            .update(
                &mut state,
                &Event::Window(iced::window::Event::RedrawRequested(
                    deadline - Duration::from_millis(1),
                )),
                bounds,
                mouse::Cursor::Unavailable,
            )
            .unwrap()
            .into_inner();
        assert!(early.0.is_none());
        assert_eq!(early.1, iced::window::RedrawRequest::At(deadline));
        let context = edits(canvas.update(
            &mut state,
            &Event::Window(iced::window::Event::RedrawRequested(deadline)),
            bounds,
            mouse::Cursor::Unavailable,
        ));
        assert!(
            matches!(context.as_slice(), [Edit::ContextMenu { position: p, .. }] if *p == Point::new(position.x - bounds.x, position.y - bounds.y))
        );
        assert!(state.gesture.is_none());
        assert!(
            send(
                &canvas,
                &mut state,
                bounds,
                touch::Event::FingerLifted {
                    id: touch::Finger(1),
                    position
                }
            )
            .is_empty()
        );
        let mut state = State::default();
        send(
            &canvas,
            &mut state,
            bounds,
            touch::Event::FingerPressed {
                id: touch::Finger(1),
                position,
            },
        );
        send(
            &canvas,
            &mut state,
            bounds,
            touch::Event::FingerMoved {
                id: touch::Finger(1),
                position: position + Vector::new(10., 0.),
            },
        );
        assert!(state.touch.deadline().is_none());
        let result = edits(canvas.update(
            &mut state,
            &Event::Window(iced::window::Event::RedrawRequested(deadline + LONG_PRESS)),
            bounds,
            mouse::Cursor::Unavailable,
        ));
        assert!(result.is_empty());
    }
}

#[test]
fn outside_unowned_and_cancelled_touches_do_not_edit() {
    let doc = Document::default();
    let canvas = chain_canvas(&doc, ChainMode::Straight);
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(400., 300.));
    let mut state = State::default();
    for event in [
        touch::Event::FingerPressed {
            id: touch::Finger(1),
            position: Point::ORIGIN,
        },
        touch::Event::FingerMoved {
            id: touch::Finger(9),
            position: Point::new(200., 180.),
        },
        touch::Event::FingerLifted {
            id: touch::Finger(9),
            position: Point::new(200., 180.),
        },
    ] {
        assert!(
            canvas
                .update(
                    &mut state,
                    &Event::Touch(event),
                    bounds,
                    mouse::Cursor::Unavailable
                )
                .is_none()
        );
    }
    let position = Point::new(200., 180.);
    send(
        &canvas,
        &mut state,
        bounds,
        touch::Event::FingerPressed {
            id: touch::Finger(1),
            position,
        },
    );
    assert!(
        send(
            &canvas,
            &mut state,
            bounds,
            touch::Event::FingerLost {
                id: touch::Finger(1),
                position
            }
        )
        .is_empty()
    );
    assert!(state.touch.deadline().is_none());
    send(
        &canvas,
        &mut state,
        bounds,
        touch::Event::FingerPressed {
            id: touch::Finger(1),
            position,
        },
    );
    canvas.update(
        &mut state,
        &Event::Window(iced::window::Event::Unfocused),
        bounds,
        mouse::Cursor::Unavailable,
    );
    assert!(state.touch.fingers.is_empty());
    assert!(state.touch.deadline().is_none());
}

#[test]
fn pinch_and_pan_keep_the_world_anchor_under_the_moving_midpoint_even_at_zoom_limits() {
    let bounds = Rectangle::with_size(iced::Size::new(500., 350.));
    let before = [Point::new(80., 100.), Point::new(200., 100.)];
    for after in [
        [Point::new(60., 100.), Point::new(220., 100.)],
        [Point::new(120., 100.), Point::new(160., 100.)],
        [Point::new(110., 80.), Point::new(230., 80.)],
        [Point::new(50., 70.), Point::new(250., 110.)],
    ] {
        let (navigation, midpoint) = pinch(before, after);
        let offset = Vector::new(
            midpoint.x - bounds.width / 2.,
            midpoint.y - bounds.height / 2.,
        );
        let new_midpoint = Point::new(
            (after[0].x + after[1].x) / 2.,
            (after[0].y + after[1].y) / 2.,
        );
        for zoom in [0.005, 0.5, 2., 5.] {
            let mut camera = Camera {
                center: World::new(1000., -2000.),
                zoom,
            };
            let anchor = camera.world(midpoint, bounds);
            let pointer = camera.navigate(navigation, offset, 0.005);
            let factor = after[0].distance(after[1]) / before[0].distance(before[1]);
            assert!((camera.zoom - (zoom * factor).clamp(0.005, 5.)).abs() < 0.00001);
            assert!(camera.screen(anchor, bounds).distance(new_midpoint) < 0.002);
            assert!(pointer.distance(anchor) < 0.002);
        }
    }
}

#[test]
fn optimization_preview_preserves_long_press_timer_and_forgets_touches_on_blur() {
    let doc = Document::default();
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.optimizer = Some(optimization::Context {
        session: 1,
        atoms: &[],
        pins: &[],
        interactive: true,
    });
    let bounds = Rectangle::with_size(iced::Size::new(400., 300.));
    let mut state = State::default();
    send(
        &canvas,
        &mut state,
        bounds,
        touch::Event::FingerPressed {
            id: touch::Finger(1),
            position: Point::new(200., 150.),
        },
    );
    let deadline = state.touch.deadline().unwrap();
    let early = canvas
        .update(
            &mut state,
            &Event::Window(iced::window::Event::RedrawRequested(
                deadline - Duration::from_millis(1),
            )),
            bounds,
            mouse::Cursor::Unavailable,
        )
        .unwrap()
        .into_inner();
    assert_eq!(early.1, iced::window::RedrawRequest::At(deadline));
    canvas.update(
        &mut state,
        &Event::Window(iced::window::Event::Unfocused),
        bounds,
        mouse::Cursor::Unavailable,
    );
    assert!(state.touch.fingers.is_empty());
    assert!(state.touch.deadline().is_none());
}

#[test]
fn batched_two_finger_events_use_current_camera_and_ignore_third_finger() {
    let doc = Document::default();
    let mut canvas = chain_canvas(&doc, ChainMode::Straight);
    canvas.guides.rulers = true;
    let bounds = Rectangle::new(Point::new(20., 50.), iced::Size::new(500., 350.));
    let paper = canvas.guides.paper(bounds);
    let mut state = State::default();
    let original = [Point::new(100., 160.), Point::new(220., 160.)];
    for (id, position) in [
        (1, original[0]),
        (2, original[1]),
        (3, Point::new(300., 200.)),
    ] {
        send(
            &canvas,
            &mut state,
            bounds,
            touch::Event::FingerPressed {
                id: touch::Finger(id),
                position,
            },
        );
    }
    assert!(
        send(
            &canvas,
            &mut state,
            bounds,
            touch::Event::FingerMoved {
                id: touch::Finger(3),
                position: Point::new(350., 250.)
            }
        )
        .is_empty()
    );
    // Emit all packets without rebuilding the canvas view, then apply them.
    let mut packets = Vec::new();
    for (id, position) in [
        (1, Point::new(80., 180.)),
        (2, Point::new(250., 180.)),
        (1, Point::new(100., 160.)),
        (2, Point::new(220., 160.)),
    ] {
        packets.extend(send(
            &canvas,
            &mut state,
            bounds,
            touch::Event::FingerMoved {
                id: touch::Finger(id),
                position,
            },
        ));
    }
    let original_camera = Camera {
        center: World::new(1000., -2000.),
        zoom: 0.5,
    };
    let mut camera = original_camera;
    for edit in packets {
        let Edit::Navigate(navigation, offset) = edit else {
            panic!("touch navigation")
        };
        camera.navigate(navigation, offset, 0.005);
    }
    assert!((camera.zoom - original_camera.zoom).abs() < 0.00001);
    assert!(camera.center.distance(original_camera.center) < 0.001);
    // No local/global origin leaks into the pinch anchor.
    let local = Point::new(original[0].x - paper.x, original[0].y - paper.y);
    assert!(
        camera
            .world(local, paper)
            .distance(original_camera.world(local, paper))
            < 0.001
    );
}
