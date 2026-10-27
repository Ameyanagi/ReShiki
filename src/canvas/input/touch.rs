//! Touch ownership and gestures, using the ordinary mouse paths for editing.

use crate::canvas::pick::DRAG_SLOP_PX;
use crate::canvas::{Edit, Gesture, MoleculeCanvas, Navigation, State, optimization};
use iced::time::{Duration, Instant};
use iced::widget::canvas::Action;
use iced::{Event, Point, Rectangle, Vector, mouse, touch};

const LONG_PRESS: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(in crate::canvas) struct Touch {
    fingers: Vec<(touch::Finger, Point)>,
    pending: Option<(Point, Instant)>,
    mouse_down: bool,
    /// Once navigation starts, no remaining finger resumes drawing until all lift.
    navigating: bool,
}

impl Touch {
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.pending.map(|(_, pressed)| pressed + LONG_PRESS)
    }
}

impl MoleculeCanvas<'_> {
    pub(super) fn touch_event(
        &self,
        state: &mut State,
        event: touch::Event,
        bounds: Rectangle,
    ) -> Option<Action<Edit>> {
        let paper = self.guides.paper(bounds);
        match event {
            touch::Event::FingerPressed { id, position } => {
                if !paper.contains(position) {
                    return None;
                }
                state.touch.fingers.push((id, position));
                if state.touch.fingers.len() == 1 {
                    state.cursor = Some(position);
                    // Wait for tap/drag intent so a long-press or a second finger
                    // cannot place an atom or start an eraser stroke.
                    state.touch.pending = Some((position, Instant::now()));
                    return Some(Action::request_redraw_at(state.touch.deadline()?).and_capture());
                }
                state.touch.pending = None;
                state.touch.mouse_down = false;
                state.touch.navigating = true;
                Some(self.cancel_touch_edit(state))
            }
            touch::Event::FingerMoved { id, position } => {
                let index = state
                    .touch
                    .fingers
                    .iter()
                    .position(|(finger, _)| *finger == id)?;
                if index >= 2 {
                    state.touch.fingers.get_mut(index)?.1 = position;
                    return Some(Action::capture());
                }
                if let [first, second, ..] = state.touch.fingers.as_mut_slice() {
                    let before = [first.1, second.1];
                    if index == 0 {
                        first.1 = position;
                    } else {
                        second.1 = position;
                    }
                    let after = [first.1, second.1];
                    let (navigation, midpoint) = pinch(before, after);
                    let offset = Vector::new(
                        midpoint.x - paper.x - paper.width / 2.,
                        midpoint.y - paper.y - paper.height / 2.,
                    );
                    return Some(Action::publish(Edit::Navigate(navigation, offset)).and_capture());
                }
                state.touch.fingers.first_mut()?.1 = position;
                if state.touch.navigating {
                    return Some(Action::capture());
                }
                if let Some((start, _)) = state.touch.pending {
                    if start.distance(position) <= DRAG_SLOP_PX {
                        state.cursor = Some(position);
                        return Some(
                            Action::request_redraw_at(state.touch.deadline()?).and_capture(),
                        );
                    }
                    state.touch.pending = None;
                    state.touch.mouse_down = true;
                    return Some(self.touch_mouse(
                        state,
                        [
                            mouse::Event::CursorMoved { position: start },
                            mouse::Event::ButtonPressed(mouse::Button::Left),
                            mouse::Event::CursorMoved { position },
                        ],
                        bounds,
                    ));
                }
                Some(if state.touch.mouse_down {
                    self.touch_mouse(state, [mouse::Event::CursorMoved { position }], bounds)
                } else {
                    Action::capture()
                })
            }
            touch::Event::FingerLifted { id, position }
            | touch::Event::FingerLost { id, position } => {
                let index = state
                    .touch
                    .fingers
                    .iter()
                    .position(|(finger, _)| *finger == id)?;
                let mut events = Vec::new();
                if !state.touch.navigating && index == 0 {
                    if let Some((start, _)) = state.touch.pending.take()
                        && matches!(event, touch::Event::FingerLifted { .. })
                    {
                        events.extend([
                            mouse::Event::CursorMoved { position: start },
                            mouse::Event::ButtonPressed(mouse::Button::Left),
                        ]);
                        state.touch.mouse_down = true;
                    }
                    if state.touch.mouse_down {
                        events.extend([
                            mouse::Event::CursorMoved { position },
                            mouse::Event::ButtonReleased(mouse::Button::Left),
                        ]);
                    }
                }
                state.touch.fingers.remove(index);
                let action = self.touch_mouse(state, events, bounds);
                if state.touch.fingers.is_empty() {
                    state.touch = Touch::default();
                    state.cursor = None;
                }
                Some(action)
            }
        }
    }

    pub(super) fn touch_long_press(
        &self,
        state: &mut State,
        bounds: Rectangle,
    ) -> Option<Action<Edit>> {
        let (position, _) = state.touch.pending.take()?;
        let paper = self.guides.paper(bounds);
        state.cursor = Some(position);
        self.right_press(
            state,
            Some(Point::new(position.x - paper.x, position.y - paper.y)),
            paper,
            bounds,
        )
    }

    fn cancel_touch_edit(&self, state: &mut State) -> Action<Edit> {
        let erasing = matches!(state.end_gesture(), Some(Gesture::Erase { .. }));
        state.last_click = None;
        state.last_transform_click = None;
        match optimization::cancelled(state).or(erasing.then_some(Edit::EraseEnd)) {
            Some(edit) => Action::publish(edit).and_capture(),
            None => Action::publish(Edit::Hover(None)).and_capture(),
        }
    }

    fn touch_mouse(
        &self,
        state: &mut State,
        events: impl IntoIterator<Item = mouse::Event>,
        bounds: Rectangle,
    ) -> Action<Edit> {
        let edits: Vec<_> = events
            .into_iter()
            .filter_map(|event| {
                self.handle_event(
                    state,
                    &Event::Mouse(event),
                    bounds,
                    mouse::Cursor::Unavailable,
                )
                .and_then(|action| action.into_inner().0)
            })
            .collect();
        if edits.is_empty() {
            Action::request_redraw().and_capture()
        } else {
            Action::publish(Edit::Touch(edits)).and_capture()
        }
    }
}

/// Zoom about the old midpoint, then translate it to the new midpoint. Keep
/// these in screen coordinates so batched events use the current app camera.
fn pinch(before: [Point; 2], after: [Point; 2]) -> (Navigation, Point) {
    let midpoint = |points: [Point; 2]| {
        Point::new(
            (points[0].x + points[1].x) / 2.,
            (points[0].y + points[1].y) / 2.,
        )
    };
    let old_midpoint = midpoint(before);
    let old_distance = before[0].distance(before[1]);
    let distance = after[0].distance(after[1]);
    let factor = if old_distance > 0. && distance > 0. {
        distance / old_distance
    } else {
        1.
    };
    (
        Navigation::Pinch {
            factor,
            pan: midpoint(after) - old_midpoint,
        },
        old_midpoint,
    )
}

#[cfg(test)]
mod tests;
