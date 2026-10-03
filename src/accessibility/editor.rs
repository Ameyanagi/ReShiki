//! Native semantics attached to an app-owned, editable Iced text editor.
use super::{LiveValueAction, Node, Role, ValueQuery};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{Id, Operation, Tree, operation::Focusable, tree},
};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme};
pub fn editor<'a, Message: 'static>(
    id: impl Into<String>,
    name: impl Into<String>,
    value: String,
    content: impl Into<Element<'a, Message>>,
    on_value: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    Element::new(Editor {
        id: id.into(),
        name: name.into(),
        value,
        content: content.into(),
        on_value: Box::new(on_value),
    })
}
struct Editor<'a, Message> {
    id: String,
    name: String,
    value: String,
    content: Element<'a, Message>,
    on_value: Box<dyn Fn(String) -> Message + 'a>,
}
struct Focus {
    id: Id,
    focused: bool,
}
impl Operation for Focus {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        if id == Some(&self.id) {
            self.focused = state.is_focused();
        }
    }
}
impl<Message: 'static> Widget<Message, Theme, Renderer> for Editor<'_, Message> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }
    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }
    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }
    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
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
        self.content.as_widget_mut().layout(tree, renderer, limits)
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
        let id = Id::from(self.id.clone());
        let mut probe = Focus {
            id: id.clone(),
            focused: false,
        };
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, &mut probe);
        operation.custom(
            Some(&id),
            layout.bounds(),
            &mut Node {
                id: self.id.clone(),
                name: self.name.clone(),
                role: Role::TextArea,
                enabled: true,
                focused: probe.focused,
                checked: None,
                expanded: None,
                value: (self.value.len() <= 16384).then(|| self.value.clone()),
                bounds: layout.bounds(),
                visible_bounds: Some(layout.bounds()),
            },
        );
        let mut query = ValueQuery {
            id: self.id.clone(),
            value: None,
        };
        operation.custom(Some(&id), layout.bounds(), &mut query);
        if let Some(value) = query.value {
            operation.custom(
                Some(&id),
                layout.bounds(),
                &mut LiveValueAction {
                    id: self.id.clone(),
                    message: Some((self.on_value)(value)),
                },
            );
        }
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
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }
}
