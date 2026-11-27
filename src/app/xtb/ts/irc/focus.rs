//! Arrow-key stepping belongs to the focused path, never a document shortcut.
use crate::app::Message;
use iced::advanced::{
    Clipboard, Layout, Renderer as _, Shell, Widget, layout, mouse, renderer,
    widget::{Id, Operation, Tree, operation::Focusable, tree},
};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, keyboard, touch};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

pub(crate) fn scope<'a>(
    id: impl Into<String>,
    paused: bool,
    step: fn(i32) -> Message,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Scope {
        id: Id::from(id.into()),
        paused,
        step,
        content: content.into(),
    })
}

struct Scope<'a> {
    id: Id,
    paused: bool,
    step: fn(i32) -> Message,
    content: Element<'a, Message>,
}

#[derive(Clone, Default)]
struct Focus(Arc<AtomicBool>);
impl Focusable for Focus {
    fn is_focused(&self) -> bool {
        self.0.load(Relaxed)
    }
    fn focus(&mut self) {
        self.0.store(true, Relaxed);
    }
    fn unfocus(&mut self) {
        self.0.store(false, Relaxed);
    }
}

#[derive(Clone, Default)]
pub(in crate::app) struct Timelines(Vec<Focus>);

impl Timelines {
    pub(in crate::app) fn clear(&self, shell: &mut Shell<'_, Message>) {
        for focus in &self.0 {
            if focus.0.swap(false, Relaxed) {
                shell.request_redraw();
            }
        }
    }
}

impl Operation for Timelines {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        visit(self);
    }

    fn custom(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn std::any::Any) {
        if let Some(focus) = state
            .downcast_ref::<Focus>()
            .filter(|focus| focus.is_focused())
        {
            self.0.push(focus.clone());
        }
    }
}

pub(in crate::app) fn pointer_press(event: &Event) -> bool {
    matches!(
        event,
        Event::Mouse(mouse::Event::ButtonPressed(_))
            | Event::Touch(touch::Event::FingerPressed { .. })
    )
}

pub(in crate::app) fn step_direction(event: &Event) -> Option<i32> {
    let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
        return None;
    };
    if !modifiers.is_empty() {
        return None;
    }
    match key {
        keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => Some(-1),
        keyboard::Key::Named(keyboard::key::Named::ArrowRight) => Some(1),
        _ => None,
    }
}

#[allow(clippy::indexing_slicing)] // The widget tree holds exactly one child.
impl Widget<Message, Theme, Renderer> for Scope<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Focus>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Focus::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let focus = tree.state.downcast_mut::<Focus>();
        operation.custom(Some(&self.id), layout.bounds(), focus);
        operation.custom(
            Some(&self.id),
            layout.bounds(),
            &mut reshiki::accessibility::ButtonFocus,
        );
        operation.focusable(Some(&self.id), layout.bounds(), focus);
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        let focus = tree.state.downcast_mut::<Focus>();
        let pressed = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => cursor.position(),
            Event::Touch(touch::Event::FingerPressed { position, .. }) => Some(*position),
            _ => None,
        };
        if pointer_press(event) {
            let inside = !shell.is_event_captured()
                && pressed.is_some_and(|point| {
                    layout.bounds().contains(point) && viewport.contains(point)
                });
            if focus.0.swap(inside, Relaxed) != inside {
                shell.request_redraw();
            }
        }
        if focus.is_focused()
            && !shell.is_event_captured()
            && let Some(direction) = step_direction(event)
        {
            if self.paused {
                shell.publish((self.step)(direction));
            }
            shell.capture_event();
            return;
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if tree.state.downcast_ref::<Focus>().is_focused() {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: iced::Border {
                        color: theme.extended_palette().primary.base.color.scale_alpha(0.5),
                        width: 1.,
                        radius: 4.into(),
                    },
                    ..Default::default()
                },
                iced::Color::TRANSPARENT,
            );
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
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
