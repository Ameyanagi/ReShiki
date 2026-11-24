//! Pointer ownership only while the rotating leg is being dragged. Other
//! canvas presses and context menus retain their ordinary behavior.
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

    fn abort(self) -> Action<Edit> {
        Action::publish(Edit::TubeBendAbort { token: self.token }).and_capture()
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
        if state.tube_bend_capture.is_none() && state.gesture.is_some() {
            return None;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(capture) = state.tube_bend_capture.take() {
                    return Some(capture.abort());
                }
                let at = point
                    .filter(|_| inside)
                    .map(|point| self.camera.world(point, bounds))?;
                if !bend.hit(at, self.camera.world_len(10.)) {
                    return None;
                }
                let capture = Capture::new(bend, at)?;
                state.end_gesture();
                state.last_click = None;
                state.tube_bend_capture = Some(capture);
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.tube_bend_capture.is_some() => {
                let threshold = self.camera.world_len(crate::canvas::pick::DRAG_SLOP_PX);
                let Some(point) = point else {
                    return Some(Action::capture());
                };
                let at = self.camera.world(point, bounds);
                let at = state.tube_bend_capture.as_mut()?.corrected(at, threshold);
                Some(at.map_or_else(Action::capture, |at| {
                    Action::publish(Edit::TubeBendDrag {
                        token: bend.token,
                        at,
                    })
                    .and_capture()
                }))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut capture = state.tube_bend_capture.take()?;
                let at = point.map(|point| self.camera.world(point, bounds));
                let threshold = self.camera.world_len(crate::canvas::pick::DRAG_SLOP_PX);
                let at = at.and_then(|at| capture.corrected(at, threshold));
                Some(if capture.dragged {
                    Action::publish(Edit::TubeBendFinish {
                        token: capture.token,
                        at,
                    })
                    .and_capture()
                } else {
                    Action::capture()
                })
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                let capture = state.tube_bend_capture.take()?;
                if inside && let Some(point) = point {
                    state.gesture = Some(Gesture::Pan { last: point });
                }
                Some(capture.abort())
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
                state.tube_bend_capture.map(|_| Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                state.tube_bend_capture.take().map(Capture::abort)
            }
            Event::Mouse(mouse::Event::CursorLeft)
            | Event::Window(iced::window::Event::Unfocused) => {
                state.cursor = None;
                state.hovering = false;
                state.bond_text_hover = None;
                Some(state.tube_bend_capture.take()?.abort())
            }
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
    fn bend_capture_projects_moves_and_final_release_with_inset_rulers() {
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
        assert!(matches!(edit, Edit::TubeBendDrag { token: (7, 2), at } if at == to));
        assert!(state.gesture.is_none());
        let final_at = World::new(60., 60.);
        let release = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(screen(final_at)),
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        assert!(
            matches!(release, Edit::TubeBendFinish { token: (7, 2), at: Some(at) } if at == final_at)
        );
        assert!(state.tube_bend_capture.is_none());
        // A press elsewhere follows the ordinary Select tool again.
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::CursorMoved { position: motion }),
            bounds,
            mouse::Cursor::Available(motion),
        );
        canvas.update(
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(motion),
        );
        assert!(matches!(state.gesture, Some(Gesture::Select { .. })));
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
        let abort = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        assert!(matches!(abort, Edit::TubeBendAbort { token: (8, 3) }));
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
        let abort = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::CursorLeft),
                bounds,
                mouse::Cursor::Unavailable,
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        assert!(matches!(abort, Edit::TubeBendAbort { token: (8, 3) }));
        assert!(state.tube_bend_capture.is_none());
        assert!(state.cursor.is_none());
    }

    #[test]
    fn release_without_a_motion_uses_its_cursor_and_tiny_drags_are_no_ops() {
        let doc = Document::default();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = crate::canvas::Tool::Select;
        canvas.tube_bend = Some(TubeBend {
            token: (6, 1),
            pivot: World::default(),
            leg: Some([World::new(20., 0.), World::new(80., 0.)]),
            can_rotate: true,
        });
        let bounds = Rectangle::with_size(Size::new(400., 300.));
        let start = canvas.camera.screen(World::new(40., 0.), bounds);
        for (at, expect_finish) in [(World::new(40.5, 0.), false), (World::new(20., 35.), true)] {
            let mut state = State::default();
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
            let release = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    bounds,
                    mouse::Cursor::Available(canvas.camera.screen(at, bounds)),
                )
                .unwrap()
                .into_inner()
                .0;
            if expect_finish {
                assert!(
                    matches!(release, Some(Edit::TubeBendFinish { token: (6, 1), at: Some(actual) }) if actual == at)
                );
            } else {
                assert!(release.is_none());
            }
            assert!(state.tube_bend_capture.is_none());
        }
    }

    #[test]
    fn an_outside_move_keeps_capture_until_finish_but_real_focus_loss_aborts() {
        let doc = Document::default();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = crate::canvas::Tool::Select;
        canvas.tube_bend = Some(TubeBend {
            token: (5, 2),
            pivot: World::default(),
            leg: Some([World::new(20., 0.), World::new(80., 0.)]),
            can_rotate: true,
        });
        let bounds = Rectangle::with_size(Size::new(400., 300.));
        let start = canvas.camera.screen(World::new(40., 0.), bounds);
        for focus_loss in [false, true] {
            let mut state = State::default();
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
            let outside = Point::new(450., 320.);
            let motion = canvas
                .update(
                    &mut state,
                    &Event::Mouse(mouse::Event::CursorMoved { position: outside }),
                    bounds,
                    mouse::Cursor::Available(outside),
                )
                .unwrap()
                .into_inner()
                .0
                .unwrap();
            assert!(matches!(motion, Edit::TubeBendDrag { token: (5, 2), .. }));
            assert!(state.tube_bend_capture.is_some());
            let event = if focus_loss {
                Event::Window(iced::window::Event::Unfocused)
            } else {
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            };
            let edit = canvas
                .update(&mut state, &event, bounds, mouse::Cursor::Unavailable)
                .unwrap()
                .into_inner()
                .0
                .unwrap();
            if focus_loss {
                assert!(matches!(edit, Edit::TubeBendAbort { token: (5, 2) }));
            } else {
                assert!(matches!(
                    edit,
                    Edit::TubeBendFinish {
                        token: (5, 2),
                        at: None
                    }
                ));
            }
            assert!(state.tube_bend_capture.is_none());
        }
    }

    #[test]
    fn ordinary_clicks_and_context_menus_fall_through_outside_the_rotating_leg() {
        let doc = Document::default();
        let mut canvas = chain_canvas(&doc, ChainMode::Straight);
        canvas.tool = crate::canvas::Tool::Select;
        canvas.tube_bend = Some(TubeBend {
            token: (4, 7),
            pivot: World::default(),
            leg: Some([World::new(20., 0.), World::new(80., 0.)]),
            can_rotate: true,
        });
        let bounds = Rectangle::new(Point::new(10., 20.), Size::new(400., 300.));
        let at = World::new(-50., 30.);
        let point = canvas.camera.screen(at, bounds) + iced::Vector::new(bounds.x, bounds.y);
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
        assert!(matches!(state.gesture, Some(Gesture::Select { .. })));
        assert!(state.tube_bend_capture.is_none());
        let edit = canvas
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        assert!(matches!(edit, Edit::ContextMenu { at: actual, .. } if actual == at));
        assert!(state.gesture.is_none());
    }
}
