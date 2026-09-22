//! Direct selection with a delayed flyout and an explicit corner target.
use super::{Message, icons::Glyph, icons::Icon, palettes};
use crate::canvas::Tool;
use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke};
use iced::{Color, Event, Point, Rectangle, Renderer, Theme, Vector, mouse};
use std::time::{Duration, Instant};

const HOLD: Duration = Duration::from_millis(450);

pub(super) struct ToolButton {
    pub tool: Tool,
    pub icon: Icon,
    pub active: bool,
    pub opens_on_click: bool,
}
#[derive(Default)]
pub(super) struct State {
    pressed: Option<Instant>,
    expanded: bool,
}
impl canvas::Program<Message> for ToolButton {
    type State = State;
    fn update(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let family = palettes::family(self.tool);
        let inside = cursor.is_over(bounds);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside => {
                state.expanded = false;
                let corner = cursor
                    .position_in(bounds)
                    .is_some_and(|p| p.x >= bounds.width - 12. && p.y >= bounds.height - 12.);
                if family.is_some() && (corner || self.opens_on_click) {
                    state.expanded = true;
                    return Some(
                        Action::publish(Message::Palette(palettes::Action::Open(self.tool)))
                            .and_capture(),
                    );
                }
                state.pressed = Some(Instant::now());
                Some(Action::request_redraw().and_capture())
            }
            Event::Window(iced::window::Event::RedrawRequested(now))
                if family.is_some() && state.pressed.is_some() =>
            {
                let deadline = state.pressed? + HOLD;
                if *now >= deadline && inside {
                    state.pressed = None;
                    state.expanded = true;
                    Some(Action::publish(Message::Palette(palettes::Action::Open(
                        self.tool,
                    ))))
                } else {
                    Some(Action::request_redraw_at(deadline))
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let pressed = state.pressed.take().is_some();
                if state.expanded {
                    state.expanded = false;
                    return Some(Action::capture());
                }
                if pressed && inside {
                    Some(Action::publish(Message::Tool(self.tool)).and_capture())
                } else if pressed {
                    Some(Action::request_redraw())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if inside && family.is_some() =>
            {
                state.pressed = None;
                Some(
                    Action::publish(Message::Palette(palettes::Action::Open(self.tool)))
                        .and_capture(),
                )
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if !inside => {
                state.pressed = None;
                Some(Action::request_redraw())
            }
            Event::Window(iced::window::Event::Unfocused)
            | Event::Mouse(mouse::Event::CursorLeft)
            | Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => {
                *state = State::default();
                Some(Action::request_redraw())
            }
            _ => None,
        }
    }
    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let hovered = cursor.is_over(bounds);
        if self.active || hovered || state.pressed.is_some() {
            let path = Path::rounded_rectangle(
                Point::new(0.5, 0.5),
                iced::Size::new(bounds.width - 1., bounds.height - 1.),
                5.0.into(),
            );
            frame.fill(
                &path,
                if self.active {
                    Color::from_rgb8(224, 239, 233)
                } else {
                    Color::from_rgb8(235, 239, 243)
                },
            );
            if self.active {
                frame.stroke(
                    &path,
                    Stroke::default()
                        .with_width(1.)
                        .with_color(Color::from_rgb8(121, 176, 159)),
                );
            }
        }
        frame.with_save(|frame| {
            frame.translate(Vector::new(6., 6.));
            Glyph(self.icon, true).paint(frame);
        });
        if palettes::family(self.tool).is_some() {
            let triangle = Path::new(|p| {
                p.move_to(Point::new(29., 33.));
                p.line_to(Point::new(33., 29.));
                p.line_to(Point::new(33., 33.));
                p.close();
            });
            frame.fill(&triangle, Color::from_rgb8(51, 62, 72));
        }
        vec![frame.into_geometry()]
    }
    fn mouse_interaction(
        &self,
        _: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn basic_bond_clicks_select_directly_even_at_the_corner() {
        for tool in [Tool::Bond(1), Tool::Bond(2), Tool::Bond(3)] {
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
}
