//! A field's hover hint yields to editing without replacing its widget tree.
use super::super::Message;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Id, Operation, Tree, operation::Focusable, tree},
};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};

pub(super) fn idle<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Idle(content.into()))
}

pub(super) fn when_idle<'a>(
    suppress: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    if suppress {
        idle(content)
    } else {
        content.into()
    }
}

struct Idle<'a>(Element<'a, Message>);

#[derive(Default)]
struct Focused(bool);

impl Operation for Focused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, _: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        self.0 |= state.is_focused();
    }
}

impl Widget<Message, Theme, Renderer> for Idle<'_> {
    fn tag(&self) -> tree::Tag {
        self.0.as_widget().tag()
    }
    fn state(&self) -> tree::State {
        self.0.as_widget().state()
    }
    fn children(&self) -> Vec<Tree> {
        self.0.as_widget().children()
    }
    fn diff(&self, tree: &mut Tree) {
        self.0.as_widget().diff(tree);
    }
    fn size(&self) -> Size<Length> {
        self.0.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.0.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.0.as_widget_mut().layout(tree, renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.0
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
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
        self.0.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
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
        self.0
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.0
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let mut focused = Focused::default();
        self.0
            .as_widget_mut()
            .operate(tree, layout, renderer, &mut focused);
        if focused.0 {
            None
        } else {
            self.0
                .as_widget_mut()
                .overlay(tree, layout, renderer, viewport, translation)
        }
    }
}
