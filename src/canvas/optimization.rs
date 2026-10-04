//! Pointer interaction for a detached optimization preview.
//!
//! These gestures publish physical constraints instead of ordinary drawing edits.
use super::{Edit, MoleculeCanvas, State, Tool, layered};
use iced::widget::canvas::{Action, Path, Stroke};
use iced::{Color, Event, Point, Rectangle, mouse};

#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub session: u64,
    pub atoms: &'a [u64],
    pub pins: &'a [u64],
    pub interactive: bool,
}

#[derive(Debug)]
pub(super) enum Drag {
    Atom { session: u64, atom: u64 },
    Rotate { session: u64, last: Point },
    Pan { last: Point },
}

fn cancelled(state: &mut State) -> Option<Edit> {
    match state.relaxation.take()? {
        Drag::Atom { session, .. } | Drag::Rotate { session, .. } => {
            Some(Edit::RelaxDragCancel { session })
        }
        Drag::Pan { .. } => None,
    }
}

pub(super) fn update(
    canvas: &MoleculeCanvas<'_>,
    context: Context<'_>,
    state: &mut State,
    event: &Event,
    bounds: Rectangle,
    point: Option<Point>,
    inside: bool,
) -> Option<Action<Edit>> {
    // A new session cannot inherit an unfinished gesture from an older preview.
    if state.relaxation.as_ref().is_some_and(|drag| match drag {
        Drag::Atom { session, .. } | Drag::Rotate { session, .. } => *session != context.session,
        Drag::Pan { .. } => false,
    }) {
        state.relaxation = None;
    }
    match event {
        Event::Window(iced::window::Event::Unfocused) | Event::Mouse(mouse::Event::CursorLeft) => {
            state.cursor = None;
            Some(match cancelled(state) {
                Some(edit) => Action::publish(edit).and_capture(),
                None => Action::publish(Edit::Hover(None)),
            })
        }
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        }) => cancelled(state).map(|edit| Action::publish(edit).and_capture()),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) if inside => {
            state.relaxation = Some(Drag::Pan { last: point? });
            Some(Action::request_redraw().and_capture())
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside => {
            state.end_gesture();
            let point = point?;
            let world = canvas.camera.world(point, bounds);
            if !context.interactive {
                return Some(Action::request_redraw().and_capture());
            }
            let hit = canvas
                .doc
                .atoms
                .iter()
                .filter(|atom| context.atoms.contains(&atom.id) && canvas.doc.atom_visible(atom.id))
                .filter(|atom| atom.position.distance(world) < 10. / canvas.camera.zoom)
                .min_by(|a, b| {
                    a.position
                        .distance(world)
                        .total_cmp(&b.position.distance(world))
                })
                .map(|atom| atom.id);
            if canvas.tool == Tool::Tilt {
                if hit.is_some()
                    || reshiki::editing::ring_at(canvas.doc, world)
                        .is_some_and(|ids| ids.iter().any(|id| context.atoms.contains(id)))
                {
                    state.relaxation = Some(Drag::Rotate {
                        session: context.session,
                        last: point,
                    });
                }
                return Some(Action::request_redraw().and_capture());
            }
            let Some(atom) = hit else {
                return Some(Action::publish(Edit::Select(vec![])).and_capture());
            };
            if state.modifiers.shift() {
                let selected = reshiki::selection_region::combine(
                    canvas.selected,
                    &[atom],
                    true,
                    canvas.selected.contains(&atom),
                );
                return Some(Action::publish(Edit::Select(selected)).and_capture());
            }
            if context.pins.contains(&atom) {
                return Some(Action::publish(Edit::Select(vec![atom])).and_capture());
            }
            state.relaxation = Some(Drag::Atom {
                session: context.session,
                atom,
            });
            Some(
                Action::publish(Edit::RelaxDragStart {
                    session: context.session,
                    atom,
                })
                .and_capture(),
            )
        }
        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            let point = point?;
            let edit = match state.relaxation.as_mut() {
                Some(Drag::Atom { session, atom }) => Edit::RelaxDragTarget {
                    session: *session,
                    atom: *atom,
                    target: canvas.camera.world(point, bounds),
                },
                Some(Drag::Rotate { session, last }) => {
                    let delta = point - *last;
                    *last = point;
                    Edit::RelaxRotate {
                        session: *session,
                        x: f64::from(-delta.y) * 0.5,
                        y: f64::from(delta.x) * 0.5,
                    }
                }
                Some(Drag::Pan { last }) => {
                    let delta = point - *last;
                    *last = point;
                    Edit::Pan(delta.x / canvas.camera.zoom, delta.y / canvas.camera.zoom)
                }
                None => Edit::Hover(inside.then(|| canvas.camera.world(point, bounds))),
            };
            Some(Action::publish(edit).and_capture())
        }
        Event::Mouse(mouse::Event::ButtonReleased(_)) => {
            let edit = match state.relaxation.take()? {
                Drag::Atom { session, atom } => Edit::RelaxDragEnd {
                    session,
                    atom,
                    target: point.map(|p| canvas.camera.world(p, bounds)),
                },
                Drag::Rotate { .. } | Drag::Pan { .. } => {
                    return Some(Action::request_redraw().and_capture());
                }
            };
            Some(Action::publish(edit).and_capture())
        }
        Event::Mouse(mouse::Event::WheelScrolled { delta })
            if inside && state.relaxation.is_none() =>
        {
            let (x, y, zoom) = match delta {
                mouse::ScrollDelta::Lines { x, y } => (*x * 40., *y * 40., *y * 0.12),
                mouse::ScrollDelta::Pixels { x, y } => (*x, *y, *y * 0.003),
            };
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            let edit = if super::command_held(state.modifiers) {
                Edit::Zoom((zoom * 0.9).exp(), canvas.camera.world(point?, bounds))
            } else {
                Edit::Pan(-x / canvas.camera.zoom, -y / canvas.camera.zoom)
            };
            Some(Action::publish(edit).and_capture())
        }
        _ => None,
    }
}

/// Pin outlines remain upright as the molecule's projection rotates.
pub(crate) fn draw_pins(
    canvas: &MoleculeCanvas<'_>,
    context: Context<'_>,
    frame: &mut layered::Frame<'_>,
    bounds: Rectangle,
) {
    for id in context.pins {
        if let Some(atom) = canvas
            .doc
            .atom(*id)
            .filter(|_| canvas.doc.atom_visible(*id))
        {
            frame.stroke(
                &Path::circle(canvas.camera.screen(atom.position, bounds), 7.),
                Stroke::default()
                    .with_width(1.6)
                    .with_color(Color::from_rgb8(133, 101, 210)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::World;
    use super::*;
    use iced::widget::canvas::Program;

    fn pointer(
        canvas: &MoleculeCanvas<'_>,
        state: &mut State,
        event: mouse::Event,
    ) -> Option<Edit> {
        let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(400., 300.));
        canvas
            .update(
                state,
                &Event::Mouse(event),
                bounds,
                mouse::Cursor::Available(Point::new(200., 150.)),
            )
            .and_then(|action| action.into_inner().0)
    }

    #[test]
    fn optimizer_drag_publishes_targets_instead_of_drawing_moves_and_fixed_pins_do_not_drag() {
        let mut doc = reshiki::document::Document::default();
        let atom = doc.add_atom("C", World::default());
        let ids = [atom];
        let mut canvas =
            super::super::tests::chain_canvas(&doc, reshiki::chains::ChainMode::Straight);
        canvas.tool = Tool::Select;
        canvas.optimizer = Some(Context {
            session: 7,
            atoms: &ids,
            pins: &[],
            interactive: true,
        });
        let mut state = State::default();
        assert!(
            matches!(pointer(&canvas, &mut state, mouse::Event::ButtonPressed(mouse::Button::Left)), Some(Edit::RelaxDragStart { atom: id, .. }) if id == atom)
        );
        assert!(
            matches!(pointer(&canvas, &mut state, mouse::Event::CursorMoved { position: Point::new(230., 160.) }), Some(Edit::RelaxDragTarget { target, .. }) if target == World::new(30., 10.))
        );
        assert!(matches!(
            pointer(
                &canvas,
                &mut state,
                mouse::Event::ButtonReleased(mouse::Button::Left)
            ),
            Some(Edit::RelaxDragEnd { .. })
        ));
        canvas.optimizer = Some(Context {
            session: 7,
            atoms: &ids,
            pins: &ids,
            interactive: true,
        });
        state.cursor = None;
        assert!(
            matches!(pointer(&canvas, &mut state, mouse::Event::ButtonPressed(mouse::Button::Left)), Some(Edit::Select(selected)) if selected == ids)
        );
        assert!(state.relaxation.is_none());
    }

    #[test]
    fn leaving_the_canvas_removes_the_temporary_constraint() {
        let mut state = State {
            relaxation: Some(Drag::Atom {
                session: 3,
                atom: 1,
            }),
            ..Default::default()
        };
        assert!(matches!(
            cancelled(&mut state),
            Some(Edit::RelaxDragCancel { session: 3 })
        ));
        assert!(state.relaxation.is_none());
    }
}
