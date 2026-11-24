//! Pointer ownership for a rigid-tube bend preview. Selection and ordinary
//! equipment handles stay inactive until the user applies or cancels it.
use crate::canvas::{Edit, Gesture, MoleculeCanvas, State};
use iced::{Event, Point, Rectangle, mouse, widget::canvas::Action};

impl MoleculeCanvas<'_> {
    pub(super) fn tube_bend_event(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        point: Option<Point>,
        inside: bool,
    ) -> Option<Action<Edit>> {
        let Some(bend) = self.tube_bend else {
            state.tube_bend_capture = None;
            return None;
        };
        if state
            .tube_bend_capture
            .is_some_and(|token| token != bend.token)
        {
            state.tube_bend_capture = None;
        }
        if matches!(state.gesture, Some(Gesture::Pan { .. })) {
            return None;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.end_gesture();
                state.last_click = None;
                if inside && let Some(point) = point {
                    let at = self.camera.world(point, bounds);
                    if bend.hit(at, self.camera.world_len(10.)) {
                        state.tube_bend_capture = Some(bend.token);
                    }
                }
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.tube_bend_capture.is_some() => {
                let at = inside
                    .then(|| point.map(|point| self.camera.world(point, bounds)))
                    .flatten();
                if !inside {
                    state.tube_bend_capture = None;
                }
                Some(
                    Action::publish(Edit::TubeBendDrag {
                        token: bend.token,
                        at,
                    })
                    .and_capture(),
                )
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let captured = state.tube_bend_capture.take().is_some();
                Some(if captured {
                    let at = inside
                        .then(|| point.map(|point| self.camera.world(point, bounds)))
                        .flatten();
                    Action::publish(Edit::TubeBendDrag {
                        token: bend.token,
                        at,
                    })
                    .and_capture()
                } else {
                    Action::capture()
                })
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                state.tube_bend_capture = None;
                None
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
                state.tube_bend_capture.map(|_| Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                state.tube_bend_capture = None;
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorLeft)
            | Event::Window(iced::window::Event::Unfocused) => {
                let token = state.tube_bend_capture.take()?;
                Some(Action::publish(Edit::TubeBendDrag { token, at: None }).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => Some(Action::request_redraw()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{TubeBend, World, guides, tests::chain_canvas};
    use iced::{Size, widget::canvas::Program};
    use reshiki::{chains::ChainMode, document::Document};

    #[test]
    fn bend_capture_uses_world_coordinates_with_inset_rulers_and_never_starts_selection() {
        let doc = Document::default();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = crate::canvas::Tool::Select;
        canvas.guides = guides::Guides {
            rulers: true,
            ..Default::default()
        };
        canvas.camera.center = World::new(50., 40.);
        canvas.camera.zoom = 2.;
        canvas.tube_bend = Some(TubeBend {
            token: (7, 2),
            pivot: World::new(50., 40.),
            leg: Some([World::new(70., 40.), World::new(90., 40.)]),
        });
        let bounds = Rectangle::new(Point::new(20., 30.), Size::new(500., 400.));
        let paper = canvas.guides.paper(bounds);
        let screen =
            |world| canvas.camera.screen(world, paper) + iced::Vector::new(paper.x, paper.y);
        let mut state = State::default();
        // Start well away from the pivot: the actual outgoing leg owns this drag.
        let start = screen(World::new(80., 40.));
        let cursor = mouse::Cursor::Available(start);
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: start }),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        assert_eq!(state.tube_bend_capture, Some((7, 2)));
        assert!(state.gesture.is_none());
        let to = World::new(65., 55.);
        let motion = screen(to);
        let edit = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorMoved { position: motion }),
                bounds,
                mouse::Cursor::Available(motion),
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        assert!(matches!(edit, Edit::TubeBendDrag { token: (7, 2), at: Some(at) } if at == to));
        assert!(state.gesture.is_none());
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(motion),
        );
        assert!(state.tube_bend_capture.is_none());
        // A press elsewhere is consumed, not a new selection or tube move.
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(motion),
        );
        assert!(state.gesture.is_none());
        assert!(state.tube_bend_capture.is_none());
    }
}
