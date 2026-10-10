//! A nonmodal palette owns pointer input within its bounds, including empty
//! areas and scroll events. Drawing input outside those bounds remains live.
use super::{Action, Message};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector, touch};
use std::collections::HashSet;

pub(super) fn guard<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Palette {
        content: content.into(),
        floating: None,
        dismiss: None,
    })
}

pub(super) fn menu<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Palette {
        content: content.into(),
        floating: None,
        dismiss: Some(Action::LabelMenu(false)),
    })
}

pub(super) fn floating<'a>(
    content: impl Into<Element<'a, Message>>,
    position: Option<Point>,
    size: Size,
    resizable: bool,
) -> Element<'a, Message> {
    Element::new(Palette {
        content: content.into(),
        dismiss: None,
        floating: Some(Floating {
            position,
            size,
            resizable,
        }),
    })
}

#[derive(Clone, Copy)]
struct Floating {
    position: Option<Point>,
    size: Size,
    resizable: bool,
}

// Keep the title and resize handle reachable at small window sizes. This
// changes only the overlay, never the drawing's layout or world camera.
pub(super) fn bounds(available: Size, position: Option<Point>, requested: Size) -> Rectangle {
    let left = 112_f32.min((available.width - 320.).max(0.));
    let top = 150_f32.min((available.height - requested.height.min(360.) - 38.).max(0.));
    let size = Size::new(
        requested.width.min((available.width - left - 12.).max(0.)),
        requested.height.min((available.height - top - 38.).max(0.)),
    );
    let right = (available.width - size.width - 12.).max(left);
    let bottom = (available.height - size.height - 38.).max(top);
    let position = position.unwrap_or(Point::new(right, top));
    Rectangle {
        x: position.x.clamp(left, right),
        y: position.y.clamp(top, bottom),
        ..Rectangle::with_size(size)
    }
}

struct Palette<'a> {
    content: Element<'a, Message>,
    floating: Option<Floating>,
    dismiss: Option<Action>,
}

#[derive(Default)]
struct State {
    pointer: Option<Point>,
    buttons: HashSet<mouse::Button>,
    fingers: HashSet<touch::Finger>,
    gesture: Option<Gesture>,
}

impl State {
    fn event_cursor(&mut self, event: &Event, cursor: mouse::Cursor) -> mouse::Cursor {
        match event {
            Event::Mouse(mouse::Event::CursorLeft | mouse::Event::CursorEntered)
            | Event::Window(iced::window::Event::Unfocused) => {
                self.pointer = None;
                cursor
            }
            Event::Mouse(_) => {
                // Iced sends a batch with its final cursor position. Recover
                // ordered mouse positions before routing to our children or
                // testing the title/corner, without bypassing an overlay mask.
                if cursor.position().is_none() {
                    self.pointer = None;
                    return cursor;
                }
                if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
                    self.pointer = Some(*position);
                }
                self.pointer.map_or(cursor, mouse::Cursor::Available)
            }
            // A redraw may arrive before queued mouse events. Never replace
            // their history with its newer snapshot of the cursor.
            _ => cursor,
        }
    }
}

#[derive(Clone, Copy)]
struct Gesture {
    resize: bool,
    press: Point,
    original: Rectangle,
    finger: Option<touch::Finger>,
}

impl Palette<'_> {
    fn child_layout<'a>(&self, layout: Layout<'a>) -> Option<Layout<'a>> {
        layout.children().next()
    }
}

impl Widget<Message, Theme, Renderer> for Palette<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content]);
    }
    fn size(&self) -> Size<Length> {
        if self.floating.is_some() {
            Size::new(Length::Fill, Length::Fill)
        } else {
            self.content.as_widget().size()
        }
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let Some(child) = tree.children.first_mut() else {
            return layout::Node::default();
        };
        if let Some(floating) = self.floating {
            let available = limits.max();
            // Use the full window for initial/reset placement: covering the
            // inspector keeps more drawing visible without moving its camera.
            // The measured content chooses its height; explicit drags stay free.
            let area = bounds(
                available,
                floating.position,
                Size::new(floating.size.width, available.height),
            );
            let measured = self.content.as_widget_mut().layout(
                child,
                renderer,
                &layout::Limits::new(Size::ZERO, area.size()),
            );
            let rect = bounds(available, floating.position, measured.size());
            let child = measured.move_to(rect.position());
            layout::Node::with_children(available, vec![child])
        } else {
            let child = self.content.as_widget_mut().layout(child, renderer, limits);
            layout::Node::with_children(child.size(), vec![child])
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let Some(child) = tree.children.first() else {
            return;
        };
        let Some(layout) = self.child_layout(layout) else {
            return;
        };
        self.content
            .as_widget()
            .draw(child, renderer, theme, style, layout, cursor, viewport);
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let Some(child) = tree.children.first_mut() else {
            return;
        };
        let Some(layout) = self.child_layout(layout) else {
            return;
        };
        self.content
            .as_widget_mut()
            .operate(child, layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let Some(child) = tree.children.first_mut() else {
            return;
        };
        let Some(content_layout) = self.child_layout(layout) else {
            return;
        };
        let rect = content_layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        let cursor = state.event_cursor(event, cursor);
        // Menu dismissal uses the same ordered position as its children. The
        // shared popover otherwise receives only the batch's final cursor, and
        // does not dismiss outside touches. Normal palette guards stay nonmodal.
        let outside_press = match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                cursor.position().is_some_and(|point| !rect.contains(point))
            }
            Event::Touch(touch::Event::FingerPressed { position, .. }) => !rect.contains(*position),
            _ => false,
        };
        if outside_press && let Some(action) = &self.dismiss {
            shell.publish(Message::Nmr(action.clone()));
            shell.capture_event();
            return;
        }
        if state.gesture.is_none() {
            self.content.as_widget_mut().update(
                child,
                event,
                content_layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        let inside = cursor.is_over(rect);
        let child_captured = shell.is_event_captured();
        let start = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                cursor.position().map(|p| (p, None))
            }
            Event::Touch(touch::Event::FingerPressed { id, position }) => {
                Some((*position, Some(*id)))
            }
            _ => None,
        };
        if !child_captured
            && state.gesture.is_none()
            && let (Some(config), Some((press, finger))) = (self.floating, start)
            && rect.contains(press)
        {
            let resize = config.resizable && resize_handle(rect).contains(press);
            if resize || press.y < rect.y + 36. {
                if resize && config.position.is_none() {
                    // A newly opened/reset panel is anchored to the canvas
                    // edge. Pin its origin before resizing so the grip follows
                    // the pointer instead of re-anchoring the opposite corner.
                    shell.publish(Message::Nmr(Action::Move(rect.position())));
                }
                state.gesture = Some(Gesture {
                    resize,
                    press,
                    original: rect,
                    finger,
                });
            }
        }
        if let Some(gesture) = state.gesture {
            let point = match event {
                Event::Mouse(mouse::Event::CursorMoved { position })
                    if gesture.finger.is_none() =>
                {
                    Some(*position)
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                    if gesture.finger.is_none() =>
                {
                    cursor.position()
                }
                Event::Touch(
                    touch::Event::FingerMoved { id, position }
                    | touch::Event::FingerLifted { id, position },
                ) if gesture.finger == Some(*id) => Some(*position),
                _ => None,
            };
            if let Some(point) = point {
                let delta = point - gesture.press;
                shell.publish(Message::Nmr(if gesture.resize {
                    Action::Resize(Size::new(
                        gesture.original.width + delta.x,
                        gesture.original.height + delta.y,
                    ))
                } else {
                    Action::Move(gesture.original.position() + delta)
                }));
            }
            let finished = match event {
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    gesture.finger.is_none()
                }
                Event::Touch(
                    touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. },
                ) => gesture.finger == Some(*id),
                Event::Window(iced::window::Event::Unfocused) => true,
                _ => false,
            };
            if finished {
                state.gesture = None;
            }
            shell.capture_event();
        }
        let captured = match event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) if inside => {
                state.buttons.insert(*button);
                true
            }
            Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                // An outside-started drawing gesture must receive its release
                // even when it ends over the palette. Its motion stayed hidden
                // here; forwarding termination cannot start a new gesture.
                state.buttons.remove(button)
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                rect.contains(*position) || !state.buttons.is_empty()
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => inside,
            Event::Touch(touch::Event::FingerPressed { id, position })
                if rect.contains(*position) =>
            {
                state.fingers.insert(*id);
                true
            }
            Event::Touch(touch::Event::FingerMoved { id, position }) => {
                state.fingers.contains(id) || rect.contains(*position)
            }
            Event::Touch(
                touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. },
            ) => state.fingers.remove(id),
            Event::Window(iced::window::Event::Unfocused) => {
                state.buttons.clear();
                state.fingers.clear();
                state.gesture = None;
                false
            }
            _ => false,
        };
        if captured {
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let Some(child) = tree.children.first() else {
            return mouse::Interaction::None;
        };
        let Some(content_layout) = self.child_layout(layout) else {
            return mouse::Interaction::None;
        };
        let rect = content_layout.bounds();
        if let Some(config) = self.floating {
            let gesture = tree.state.downcast_ref::<State>().gesture;
            if gesture.is_some_and(|g| g.resize)
                || (config.resizable && cursor.is_over(resize_handle(rect)))
            {
                return mouse::Interaction::ResizingDiagonallyDown;
            }
            if gesture.is_some() {
                return mouse::Interaction::Grabbing;
            }
        }
        let interaction = self.content.as_widget().mouse_interaction(
            child,
            content_layout,
            cursor,
            viewport,
            renderer,
        );
        if interaction == mouse::Interaction::None && cursor.is_over(rect) {
            if self.floating.is_some() && cursor.position().is_some_and(|p| p.y < rect.y + 36.) {
                mouse::Interaction::Grab
            } else {
                mouse::Interaction::Idle
            }
        } else {
            interaction
        }
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let layout = self.child_layout(layout)?;
        let inner = self.content.as_widget_mut().overlay(
            tree.children.first_mut()?,
            layout,
            renderer,
            viewport,
            translation,
        )?;
        Some(overlay::Element::new(Box::new(OwnedOverlay {
            inner,
            state: tree.state.downcast_mut::<State>(),
        })))
    }
}

// The shared popover owns all releases over its surface. NMR additionally
// preserves a canvas gesture that started before an accessibility-opened menu.
// Share the palette's ownership so dismissing a menu on press cannot leak its
// later release to the drawing after the overlay has disappeared.
struct OwnedOverlay<'a> {
    inner: overlay::Element<'a, Message, Theme, Renderer>,
    state: &'a mut State,
}
impl overlay::Overlay<Message, Theme, Renderer> for OwnedOverlay<'_> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.inner.as_overlay_mut().layout(renderer, bounds)
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.inner
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.inner
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let cursor = self.state.event_cursor(event, cursor);
        if self.state.gesture.is_some() && matches!(event, Event::Mouse(_) | Event::Touch(_)) {
            return;
        }
        let owned_release = match event {
            Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                Some(self.state.buttons.remove(button))
            }
            Event::Touch(
                touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. },
            ) => Some(self.state.fingers.remove(id)),
            _ => None,
        };
        if owned_release == Some(false) {
            return;
        }
        self.inner
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) if shell.is_event_captured() => {
                self.state.buttons.insert(*button);
            }
            Event::Touch(touch::Event::FingerPressed { id, .. }) if shell.is_event_captured() => {
                self.state.fingers.insert(*id);
            }
            Event::Window(iced::window::Event::Unfocused) => {
                self.state.buttons.clear();
                self.state.fingers.clear();
            }
            _ => {}
        }
        if owned_release == Some(true)
            || matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. }))
                && !self.state.buttons.is_empty()
            || matches!(event, Event::Touch(touch::Event::FingerMoved { id, .. }) if self.state.fingers.contains(id))
        {
            shell.capture_event();
        }
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.inner
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.inner.as_overlay_mut().overlay(layout, renderer)
    }
    fn index(&self) -> f32 {
        self.inner.as_overlay().index()
    }
}

fn resize_handle(rect: Rectangle) -> Rectangle {
    // The footer glyph is centered in the command row above 10px of padding.
    // Include it without widening this strip into the neighboring controls.
    Rectangle {
        x: rect.x + rect.width - 24.,
        y: rect.y + rect.height - 32.,
        width: 24.,
        height: 32.,
    }
}

#[cfg(test)]
mod tests {
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
}
