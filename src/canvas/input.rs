//! Pointer and keyboard events on the molecule canvas.

mod motion;
mod press;
mod release;

use crate::canvas::{Edit, Gesture, Handle, MoleculeCanvas, State, Tool, optimization};
use iced::widget::canvas::Action;
use iced::{Event, Point, Rectangle, mouse};

impl MoleculeCanvas<'_> {
    pub(super) fn handle_event(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Edit>> {
        let canvas_bounds = bounds;
        let bounds = self.guides.paper(bounds);
        // Iced may dispatch a batch with the final cursor position. Preserve the
        // position carried by each motion event so fast drags retain their origin.
        let was_inside = state.cursor.is_some_and(|p| bounds.contains(p));
        let pointer_moved = matches!(event, Event::Mouse(mouse::Event::CursorMoved { position }) if state.cursor != Some(*position));
        if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
            state.cursor = Some(*position);
        }
        if let Event::Keyboard(
            iced::keyboard::Event::ModifiersChanged(modifiers)
            | iced::keyboard::Event::KeyPressed { modifiers, .. }
            | iced::keyboard::Event::KeyReleased { modifiers, .. },
        ) = event
        {
            state.modifiers = *modifiers;
        }
        let point = state
            .cursor
            .or(cursor.position())
            .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
        let inside = point.is_some_and(|p| Rectangle::with_size(bounds.size()).contains(p));
        if let Some(context) = self.optimizer {
            return optimization::update(self, context, state, event, bounds, point, inside);
        }
        state.relaxation = None;
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && !inside
            || matches!(
                event,
                Event::Keyboard(iced::keyboard::Event::KeyPressed { .. })
                    | Event::Window(iced::window::Event::Unfocused)
                    | Event::Mouse(
                        mouse::Event::ButtonPressed(mouse::Button::Right | mouse::Button::Middle)
                            | mouse::Event::WheelScrolled { .. }
                            | mouse::Event::CursorLeft
                    )
            )
        {
            state.last_transform_click = None;
        }
        match event {
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(_)) => {
                Some(Action::request_redraw())
            }
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => {
                if matches!(state.end_gesture(), Some(Gesture::Erase { .. })) {
                    return Some(Action::publish(Edit::EraseEnd));
                }
                Some(Action::request_redraw())
            }
            Event::Window(iced::window::Event::Unfocused) => {
                let erasing = matches!(state.end_gesture(), Some(Gesture::Erase { .. }));
                state.last_click = None;
                state.cursor = None;
                Some(Action::publish(if erasing {
                    Edit::EraseEnd
                } else {
                    Edit::Hover(None)
                }))
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                state.cursor = None;
                if matches!(state.gesture, Some(Gesture::Erase { .. })) {
                    state.gesture = None;
                    return Some(Action::publish(Edit::EraseEnd));
                }
                Some(Action::publish(Edit::Hover(None)))
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if inside && state.gesture.is_none() =>
            {
                let (x, y, zoom_amount) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (*x * 40., *y * 40., *y * 0.12),
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y, *y * 0.003),
                };
                if !x.is_finite() || !y.is_finite() {
                    return None;
                }
                let edit = if state.modifiers.command() || state.modifiers.control() {
                    Edit::Zoom(zoom_amount.exp(), self.camera.world(point?, bounds))
                } else {
                    Edit::Pan(x / self.camera.zoom, y / self.camera.zoom)
                };
                Some(Action::publish(edit).and_capture())
            }
            // macOS can report Control-click as a secondary click.
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if inside && self.tool == Tool::Template && state.modifiers.control() =>
            {
                state.gesture = Some(Gesture::Ring {
                    start: self.camera.world(point?, bounds),
                    attached: true,
                });
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if inside => {
                self.right_press(state, point, bounds, canvas_bounds)
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) if inside => {
                state.gesture = Some(Gesture::Pan { last: point? });
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside => {
                self.left_press(state, point, bounds)
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                self.cursor_moved(state, point, bounds, inside, was_inside, pointer_moved)
            }
            Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                self.button_released(state, point, bounds, inside)
            }
            _ => None,
        }
    }
    pub(super) fn pointer_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let bounds = self.guides.paper(bounds);
        if let Some(Gesture::Transform(drag)) = &state.gesture {
            return if matches!(drag.handle, Handle::Rotate) {
                mouse::Interaction::Grabbing
            } else {
                drag.handle.cursor()
            };
        }
        if matches!(state.gesture, Some(Gesture::ArrowHandle { .. })) {
            return mouse::Interaction::Grabbing;
        }
        if let Tool::StretchBond { fixed, moving } = self.tool
            && cursor.is_over(bounds)
        {
            if matches!(state.gesture, Some(Gesture::StretchBond { .. })) {
                return mouse::Interaction::Grabbing;
            }
            if let Some(p) = cursor.position_in(bounds)
                && let Ok(plan) = reshiki::editing::reference::Stretch::new(self.doc, fixed, moving)
                && crate::canvas::hit::hit_selection(
                    self.doc,
                    self.camera.world(p, bounds),
                    10. / self.camera.zoom,
                )
                .iter()
                .any(|id| plan.ids.contains(id))
            {
                return mouse::Interaction::Grab;
            }
            return mouse::Interaction::Crosshair;
        }
        if self.tool == Tool::Tilt && cursor.is_over(bounds) {
            return if matches!(state.gesture, Some(Gesture::Tilt(_))) {
                mouse::Interaction::Grabbing
            } else {
                mouse::Interaction::Grab
            };
        }
        if self.tool == Tool::Erase && cursor.is_over(bounds) {
            return mouse::Interaction::Crosshair;
        }
        if self.selected.len() == 1
            && let Some(p) = cursor.position_in(bounds)
        {
            let p = self.camera.world(p, bounds);
            if self
                .doc
                .arrows
                .iter()
                .filter(|a| self.selected.contains(&a.id) && self.doc.atom_visible(a.id))
                .any(|a| {
                    a.handles()
                        .iter()
                        .any(|q| q.distance(p) < 8. / self.camera.zoom)
                })
            {
                return mouse::Interaction::Grab;
            }
        }
        if cursor.is_over(bounds) {
            if self.tool.selects() {
                if let Some(selection) =
                    state
                        .scene
                        .borrow_mut()
                        .selection(self.doc, self.selected, self.camera, bounds)
                    && let Some(p) = cursor.position()
                    && let Some(handle) = selection.hit(Point::new(p.x - bounds.x, p.y - bounds.y))
                {
                    return handle.cursor();
                }
                mouse::Interaction::default()
            } else {
                mouse::Interaction::Crosshair
            }
        } else {
            mouse::Interaction::default()
        }
    }
}
