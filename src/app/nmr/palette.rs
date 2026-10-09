//! A nonmodal palette owns pointer input within its bounds, including empty
//! areas and scroll events. Drawing input outside those bounds remains live.
use super::Message;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, touch};
use std::collections::HashSet;

pub(super) fn guard<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Palette {
        content: content.into(),
    })
}

struct Palette<'a> {
    content: Element<'a, Message>,
}

#[derive(Default)]
struct State {
    buttons: HashSet<mouse::Button>,
    fingers: HashSet<touch::Finger>,
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
        self.content.as_widget().size()
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
        self.content.as_widget_mut().layout(child, renderer, limits)
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
        self.content.as_widget_mut().update(
            child, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
        let state = tree.state.downcast_mut::<State>();
        let inside = cursor.is_over(layout.bounds());
        let captured = match event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) if inside => {
                state.buttons.insert(*button);
                true
            }
            Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                state.buttons.remove(button) || inside
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                layout.bounds().contains(*position) || !state.buttons.is_empty()
            }
            Event::Mouse(mouse::Event::WheelScrolled { .. }) => inside,
            Event::Touch(touch::Event::FingerPressed { id, position })
                if layout.bounds().contains(*position) =>
            {
                state.fingers.insert(*id);
                true
            }
            Event::Touch(touch::Event::FingerMoved { id, position }) => {
                state.fingers.contains(id) || layout.bounds().contains(*position)
            }
            Event::Touch(
                touch::Event::FingerLifted { id, position }
                | touch::Event::FingerLost { id, position },
            ) => state.fingers.remove(id) || layout.bounds().contains(*position),
            Event::Window(iced::window::Event::Unfocused) => {
                state.buttons.clear();
                state.fingers.clear();
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
        let interaction = self
            .content
            .as_widget()
            .mouse_interaction(child, layout, cursor, viewport, renderer);
        if interaction == mouse::Interaction::None && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Idle
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
        self.content.as_widget_mut().overlay(
            tree.children.first_mut()?,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
