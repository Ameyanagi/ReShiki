//! Pointer ownership for a rigid-tube bend preview. Selection and ordinary
//! equipment handles stay inactive until the user applies or cancels it.
use crate::canvas::{Edit, Gesture, MoleculeCanvas, State};
use iced::{Event, Point, Rectangle, mouse, widget::canvas::Action};

#[derive(Clone, Copy, Debug)]
pub(in crate::canvas) struct Capture {
    pub(in crate::canvas) token: (u64, u64),
    pivot: crate::canvas::World,
    pressed: crate::canvas::World,
    angle_offset: f64,
    dragged: bool,
}

impl Capture {
    fn new(bend: crate::canvas::TubeBend, pressed: crate::canvas::World) -> Option<Self> {
        let [a, b] = bend.leg?;
        let angle = f64::from(b.y - a.y).atan2(f64::from(b.x - a.x));
        let pointer =
            f64::from(pressed.y - bend.pivot.y).atan2(f64::from(pressed.x - bend.pivot.x));
        Some(Self {
            token: bend.token,
            pivot: bend.pivot,
            pressed,
            angle_offset: angle - pointer,
            dragged: false,
        })
    }

    /// Keep the angular grab offset, rather than snapping an off-centre press
    /// onto the centreline. The app still receives its usual world target.
    fn corrected(
        &mut self,
        at: crate::canvas::World,
        threshold: f32,
    ) -> Option<crate::canvas::World> {
        if !self.dragged && at.distance(self.pressed) < threshold {
            return None;
        }
        let x = f64::from(at.x - self.pivot.x);
        let y = f64::from(at.y - self.pivot.y);
        if !x.is_finite() || !y.is_finite() || x.hypot(y) < 1e-9 {
            return None;
        }
        self.dragged = true;
        let (sin, cos) = self.angle_offset.sin_cos();
        Some(
            self.pivot
                .offset((cos * x - sin * y) as f32, (sin * x + cos * y) as f32),
        )
    }

    fn ended(self) -> Action<Edit> {
        if self.dragged {
            Action::publish(Edit::TubeBendDrag {
                token: self.token,
                at: None,
            })
            .and_capture()
        } else {
            Action::capture()
        }
    }
}

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
            .is_some_and(|capture| capture.token != bend.token || !bend.can_rotate)
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
                state.tube_bend_capture = None;
                if inside && let Some(point) = point {
                    let at = self.camera.world(point, bounds);
                    if bend.hit(at, self.camera.world_len(10.)) {
                        state.tube_bend_capture = Capture::new(bend, at);
                    }
                }
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.tube_bend_capture.is_some() => {
                if !inside {
                    return Some(state.tube_bend_capture.take()?.ended());
                }
                let threshold = self.camera.world_len(crate::canvas::pick::DRAG_SLOP_PX);
                let at = self.camera.world(point?, bounds);
                let at = state.tube_bend_capture.as_mut()?.corrected(at, threshold);
                Some(at.map_or_else(Action::capture, |at| {
                    Action::publish(Edit::TubeBendDrag {
                        token: bend.token,
                        at: Some(at),
                    })
                    .and_capture()
                }))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let Some(mut capture) = state.tube_bend_capture.take() else {
                    return Some(Action::capture());
                };
                let at = point
                    .filter(|_| inside)
                    .map(|point| self.camera.world(point, bounds));
                let threshold = self.camera.world_len(crate::canvas::pick::DRAG_SLOP_PX);
                Some(
                    if let Some(at) = at.and_then(|at| capture.corrected(at, threshold)) {
                        Action::publish(Edit::TubeBendDrag {
                            token: capture.token,
                            at: Some(at),
                        })
                        .and_capture()
                    } else {
                        capture.ended()
                    },
                )
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
                state.cursor = None;
                state.hovering = false;
                state.bond_text_hover = None;
                Some(state.tube_bend_capture.take()?.ended())
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
            can_rotate: true,
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
        assert_eq!(
            state.tube_bend_capture.map(|capture| capture.token),
            Some((7, 2))
        );
        assert!(state.gesture.is_none());
        let click = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor,
            )
            .unwrap();
        assert!(
            click.into_inner().0.is_none(),
            "a click must not set the bend angle"
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
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

    #[test]
    fn rotation_preserves_the_angular_grab_offset_and_ignores_small_movements() {
        let pivot = World::new(20., 30.);
        let radial = |degrees: f64, radius: f64| {
            pivot.offset(
                (degrees.to_radians().cos() * radius) as f32,
                (degrees.to_radians().sin() * radius) as f32,
            )
        };
        let bend = TubeBend {
            token: (3, 9),
            pivot,
            leg: Some([radial(30., 10.), radial(30., 80.)]),
            can_rotate: true,
        };
        // Grab 4 degrees off the leg, rather than exactly on its centreline.
        let pressed = radial(34., 40.);
        assert!(bend.hit(pressed, 10.));
        let mut capture = Capture::new(bend, pressed).unwrap();
        assert!(capture.corrected(pressed, 3.).is_none());
        assert!(capture.corrected(pressed.offset(0.5, 0.5), 3.).is_none());
        assert!(!capture.dragged);
        // A 25-degree pointer turn makes a 25-degree leg turn, without
        // including the initial 4-degree offset or changing the pivot.
        let corrected = capture.corrected(radial(59., 55.), 3.).unwrap();
        let actual = f64::from(corrected.y - pivot.y)
            .atan2(f64::from(corrected.x - pivot.x))
            .to_degrees();
        assert!((actual - 55.).abs() < 1e-4, "{actual}");
        assert!((corrected.distance(pivot) - 55.).abs() < 1e-4);
        assert!(capture.dragged);
    }

    #[test]
    fn only_the_rotating_side_captures_and_locks_remove_the_grab_affordance() {
        let doc = Document::default();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = crate::canvas::Tool::Select;
        let bend = TubeBend {
            token: (2, 4),
            pivot: World::default(),
            leg: Some([World::new(15., 0.), World::new(80., 0.)]),
            can_rotate: true,
        };
        canvas.tube_bend = Some(bend);
        let bounds = Rectangle::with_size(Size::new(400., 300.));
        let mut state = State::default();
        for world in [World::default(), World::new(4., 0.), World::new(-30., 0.)] {
            assert!(!bend.hit(world, 10.));
            let point = canvas.camera.screen(world, bounds);
            canvas.tube_bend_event(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                Some(point),
                true,
            );
            assert!(state.tube_bend_capture.is_none());
            assert!(state.gesture.is_none());
            assert_eq!(
                canvas.pointer_interaction(&state, bounds, mouse::Cursor::Available(point)),
                mouse::Interaction::default()
            );
        }
        let point = canvas.camera.screen(World::new(40., 0.), bounds);
        assert_eq!(
            canvas.pointer_interaction(&state, bounds, mouse::Cursor::Available(point)),
            mouse::Interaction::Grab
        );
        canvas.tube_bend = Some(TubeBend {
            can_rotate: false,
            ..bend
        });
        canvas.tube_bend_event(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            Some(point),
            true,
        );
        assert!(state.tube_bend_capture.is_none());
        assert_eq!(
            canvas.pointer_interaction(&state, bounds, mouse::Cursor::Available(point)),
            mouse::Interaction::default()
        );
    }

    #[test]
    fn losing_capture_or_starting_a_pan_cannot_continue_a_rotation() {
        let doc = Document::default();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = crate::canvas::Tool::Select;
        canvas.tube_bend = Some(TubeBend {
            token: (8, 3),
            pivot: World::default(),
            leg: Some([World::new(20., 0.), World::new(80., 0.)]),
            can_rotate: true,
        });
        let bounds = Rectangle::with_size(Size::new(400., 300.));
        let point = canvas.camera.screen(World::new(40., 0.), bounds);
        let cursor = mouse::Cursor::Available(point);
        let mut state = State::default();
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        assert!(state.tube_bend_capture.is_some());
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)),
            bounds,
            cursor,
        );
        assert!(state.tube_bend_capture.is_none());
        assert!(matches!(state.gesture, Some(Gesture::Pan { .. })));
        canvas.update(
            &mut state,
            &Event::Window(iced::window::Event::Unfocused),
            bounds,
            mouse::Cursor::Unavailable,
        );
        assert!(state.gesture.is_none());
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            bounds,
            cursor,
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        );
        assert!(state.tube_bend_capture.is_some());
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorLeft),
            bounds,
            mouse::Cursor::Unavailable,
        );
        assert!(state.tube_bend_capture.is_none());
        assert!(state.cursor.is_none());
    }
}
