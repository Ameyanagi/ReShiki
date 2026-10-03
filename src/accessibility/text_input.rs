use super::{LiveValueAction, Node, Role, ValueQuery};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{Id, Operation, Tree, tree},
};
use iced::widget::text_input as input;
use iced::{Element, Event, Font, Length, Padding, Pixels, Rectangle, Renderer, Size, Theme};
use std::rc::Rc;

type InputState = input::State<<Renderer as iced::advanced::text::Renderer>::Paragraph>;

/// Semantics and native editing for the actual Iced input and its own state.
pub struct TextInput<'a, Message> {
    id: String,
    name: String,
    value: String,
    inner: input::TextInput<'a, Message>,
    on_input: Option<Rc<dyn Fn(String) -> Message + 'a>>,
}

pub fn text_input<'a, Message: Clone + 'static>(
    id: impl Into<String>,
    name: impl Into<String>,
    placeholder: &str,
    value: &str,
) -> TextInput<'a, Message> {
    let id = id.into();
    TextInput {
        inner: input(placeholder, value).id(Id::from(id.clone())),
        id,
        name: name.into(),
        value: value.into(),
        on_input: None,
    }
}

impl<'a, Message: Clone + 'static> TextInput<'a, Message> {
    pub fn on_input(self, on_input: impl Fn(String) -> Message + 'a) -> Self {
        self.on_input_maybe(Some(on_input))
    }
    pub fn on_input_maybe(mut self, callback: Option<impl Fn(String) -> Message + 'a>) -> Self {
        self.on_input =
            callback.map(|callback| Rc::new(callback) as Rc<dyn Fn(String) -> Message + 'a>);
        let callback = self.on_input.clone();
        self.inner = self
            .inner
            .on_input_maybe(callback.map(|callback| move |value| callback(value)));
        self
    }
    pub fn on_submit(mut self, message: Message) -> Self {
        self.inner = self.inner.on_submit(message);
        self
    }
    pub fn on_submit_maybe(mut self, message: Option<Message>) -> Self {
        self.inner = self.inner.on_submit_maybe(message);
        self
    }
    pub fn font(mut self, font: Font) -> Self {
        self.inner = self.inner.font(font);
        self
    }
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.inner = self.inner.padding(padding);
        self
    }
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.inner = self.inner.size(size);
        self
    }
    pub fn style(mut self, style: impl Fn(&Theme, input::Status) -> input::Style + 'a) -> Self {
        self.inner = self.inner.style(style);
        self
    }
}

struct Identity {
    id: String,
    enabled: bool,
}
impl<Message: Clone + 'static> Widget<Message, Theme, Renderer> for TextInput<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Identity>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Identity {
            id: self.id.clone(),
            enabled: self.on_input.is_some(),
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(
            &self.inner as &dyn Widget<Message, Theme, Renderer>,
        )]
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<Identity>();
        if state.id != self.id || state.enabled != self.on_input.is_some() {
            *state = Identity {
                id: self.id.clone(),
                enabled: self.on_input.is_some(),
            };
            tree.children = self.children();
        } else {
            tree.diff_children(&[&self.inner as &dyn Widget<Message, Theme, Renderer>]);
        }
    }
    fn size(&self) -> Size<Length> {
        Widget::size(&self.inner)
    }
    fn size_hint(&self) -> Size<Length> {
        Widget::size_hint(&self.inner)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        match tree.children.first_mut() {
            Some(child) => Widget::layout(&mut self.inner, child, renderer, limits),
            None => layout::Node::new(Size::ZERO),
        }
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
        let enabled = self.on_input.is_some();
        if enabled {
            Widget::operate(&mut self.inner, child, layout, renderer, operation);
        } else {
            child.state.downcast_mut::<InputState>().unfocus();
        }
        let focused = child.state.downcast_ref::<InputState>().is_focused();
        let id = Id::from(self.id.clone());
        operation.custom(
            Some(&id),
            layout.bounds(),
            &mut Node {
                id: self.id.clone(),
                name: self.name.clone(),
                role: Role::TextInput,
                enabled,
                focused,
                checked: None,
                expanded: None,
                value: Some(self.value.clone()),
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
                    message: self.on_input.as_ref().map(|callback| callback(value)),
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
        if let Some(child) = tree.children.first_mut() {
            Widget::update(
                &mut self.inner,
                child,
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
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
        if let Some(child) = tree.children.first() {
            Widget::draw(
                &self.inner,
                child,
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
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
        tree.children
            .first()
            .map_or(mouse::Interaction::None, |child| {
                Widget::mouse_interaction(&self.inner, child, layout, cursor, viewport, renderer)
            })
    }
}
impl<'a, Message: Clone + 'static> From<TextInput<'a, Message>> for Element<'a, Message> {
    fn from(input: TextInput<'a, Message>) -> Self {
        Element::new(input)
    }
}
