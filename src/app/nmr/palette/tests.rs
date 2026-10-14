use super::*;

#[test]
fn pointer_history_uses_ordered_motion_and_never_resyncs_from_redraw() {
    let mut state = State::default();
    let start = Point::new(500., 220.);
    let end = Point::new(400., 170.);
    let final_cursor = mouse::Cursor::Available(end);
    assert_eq!(
        state.event_cursor(
            &Event::Mouse(mouse::Event::CursorMoved { position: start }),
            final_cursor,
        ),
        mouse::Cursor::Available(start)
    );
    assert_eq!(
        state.event_cursor(
            &Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now()
            )),
            final_cursor,
        ),
        final_cursor
    );
    for event in [
        mouse::Event::ButtonPressed(mouse::Button::Left),
        mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0., y: 1. },
        },
    ] {
        assert_eq!(
            state.event_cursor(&Event::Mouse(event), final_cursor),
            mouse::Cursor::Available(start)
        );
    }
    assert_eq!(
        state.event_cursor(
            &Event::Mouse(mouse::Event::CursorMoved { position: end }),
            final_cursor,
        ),
        final_cursor
    );
    assert_eq!(
        state.event_cursor(
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            final_cursor,
        ),
        final_cursor
    );
}

#[test]
fn pointer_history_preserves_masks_and_invalidates_left_entry_and_unfocus() {
    let start = Point::new(500., 220.);
    let end = Point::new(400., 170.);
    let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    for masked in [mouse::Cursor::Unavailable, mouse::Cursor::Levitating(end)] {
        let mut state = State {
            pointer: Some(start),
            ..State::default()
        };
        assert_eq!(state.event_cursor(&press, masked), masked);
        assert!(state.pointer.is_none());
        assert_eq!(
            state.event_cursor(&press, mouse::Cursor::Available(end)),
            mouse::Cursor::Available(end)
        );
    }
    for cancel in [
        Event::Mouse(mouse::Event::CursorLeft),
        Event::Mouse(mouse::Event::CursorEntered),
        Event::Window(iced::window::Event::Unfocused),
    ] {
        let mut state = State {
            pointer: Some(start),
            ..State::default()
        };
        let _ = state.event_cursor(&cancel, mouse::Cursor::Available(end));
        assert!(state.pointer.is_none());
        assert_eq!(
            state.event_cursor(&press, mouse::Cursor::Available(end)),
            mouse::Cursor::Available(end)
        );
    }
}
