use super::*;
use iced::widget::canvas::Program;
fn bounds() -> Rectangle {
    Rectangle::with_size(iced::Size::new(36., 36.))
}
fn button(tool: Tool) -> ToolButton {
    ToolButton {
        tool,
        icon: Icon::Tool(tool),
        active: false,
        opens_on_click: tool == Tool::Wedge,
    }
}
fn message(action: Option<Action<Message>>) -> Option<Message> {
    action.and_then(|a| a.into_inner().0)
}
#[test]
fn basic_bonds_and_chains_select_directly_even_at_the_corner() {
    for tool in [
        Tool::Bond(1),
        Tool::Bond(2),
        Tool::Bond(3),
        Tool::Chain(reshiki::chains::ChainMode::Straight),
        Tool::Chain(reshiki::chains::ChainMode::Snaking),
    ] {
        let button = button(tool);
        let mut state = State::default();
        let cursor = mouse::Cursor::Available(Point::new(32., 32.));
        assert!(
            message(button.update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds(),
                cursor
            ))
            .is_none()
        );
        assert!(
            matches!(message(button.update(&mut state, &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)), bounds(), cursor)), Some(Message::Tool(t)) if t == tool)
        );
    }
}
#[test]
fn ring_click_selects_and_hold_opens_without_selecting_on_release() {
    let button = button(Tool::Ring);
    let cursor = mouse::Cursor::Available(Point::new(16., 16.));
    let mut state = State::default();
    let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    button.update(&mut state, &press, bounds(), cursor);
    assert!(matches!(
        message(button.update(&mut state, &release, bounds(), cursor)),
        Some(Message::Tool(Tool::Ring))
    ));
    button.update(&mut state, &press, bounds(), cursor);
    let now = state.pressed.unwrap() + HOLD;
    assert!(matches!(
        message(button.update(
            &mut state,
            &Event::Window(iced::window::Event::RedrawRequested(now)),
            bounds(),
            cursor
        )),
        Some(Message::Palette(palettes::Action::Open(Tool::Ring)))
    ));
    assert!(message(button.update(&mut state, &release, bounds(), cursor)).is_none());
}
#[test]
fn leaving_cancels_a_hold_and_the_corner_opens_immediately() {
    let button = button(Tool::Arrow);
    let mut state = State::default();
    let cursor = mouse::Cursor::Available(Point::new(16., 16.));
    button.update(
        &mut state,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds(),
        cursor,
    );
    button.update(
        &mut state,
        &Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(60., 60.),
        }),
        bounds(),
        mouse::Cursor::Available(Point::new(60., 60.)),
    );
    assert!(state.pressed.is_none());
    assert!(matches!(
        message(button.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds(),
            mouse::Cursor::Available(Point::new(32., 32.))
        )),
        Some(Message::Palette(palettes::Action::Open(Tool::Arrow)))
    ));
}
