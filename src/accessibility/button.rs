use super::{LiveAction, Node, Role};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Id, Operation, Tree, operation::Focusable, tree},
};
use iced::widget::button as iced_button;
use iced::{
    Border, Color, Element, Event, Length, Padding, Rectangle, Renderer, Size, Theme, Vector,
    keyboard, touch, window,
};

/// An ordinary Iced button with semantics and actual keyboard focus.
/// The same `on_press` value drives mouse, keyboard and native activation.
pub struct Button<'a, Message> {
    id: String,
    name: String,
    inner: iced_button::Button<'a, Message>,
    action: Option<Message>,
    checked: Option<bool>,
    value: Option<String>,
}

pub fn button<'a, Message: Clone + 'static>(
    id: impl Into<String>,
    name: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
) -> Button<'a, Message> {
    Button {
        id: id.into(),
        name: name.into(),
        inner: iced_button(content),
        action: None,
        checked: None,
        value: None,
    }
}

impl<'a, Message: Clone + 'static> Button<'a, Message> {
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.inner = self.inner.padding(padding);
        self
    }

    pub fn style(
        mut self,
        style: impl Fn(&Theme, iced_button::Status) -> iced_button::Style + 'a,
    ) -> Self {
        self.inner = self.inner.style(style);
        self
    }

    pub fn on_press(self, message: Message) -> Self {
        self.on_press_maybe(Some(message))
    }

    pub fn on_press_maybe(mut self, message: Option<Message>) -> Self {
        self.inner = self.inner.on_press_maybe(message.clone());
        self.action = message;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
}

#[derive(Default)]
struct State {
    id: String,
    focused: bool,
    space_down: bool,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
        self.space_down = false;
    }
    fn unfocus(&mut self) {
        self.focused = false;
        self.space_down = false;
    }
}

impl<Message: Clone + 'static> Widget<Message, Theme, Renderer> for Button<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            id: self.id.clone(),
            ..State::default()
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(
            &self.inner as &dyn Widget<Message, Theme, Renderer>,
        )]
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();
        if state.id != self.id {
            *state = State {
                id: self.id.clone(),
                ..State::default()
            };
        }
        if self.action.is_none() {
            state.unfocus();
        }
        tree.diff_children(&[&self.inner as &dyn Widget<Message, Theme, Renderer>]);
    }
    fn size(&self) -> Size<Length> {
        self.inner.size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.inner.size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        match tree.children.first_mut() {
            Some(child) => self.inner.layout(child, renderer, limits),
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
        let state = tree.state.downcast_mut::<State>();
        let id = Id::from(self.id.clone());
        if self.action.is_some() {
            operation.focusable(Some(&id), layout.bounds(), state);
        } else {
            state.unfocus();
        }
        let mut node = Node {
            id: self.id.clone(),
            name: self.name.clone(),
            role: if self.checked.is_some() {
                Role::ToggleButton
            } else {
                Role::Button
            },
            enabled: self.action.is_some(),
            focused: state.focused,
            checked: self.checked,
            value: self.value.clone(),
            bounds: layout.bounds(),
            visible_bounds: Some(layout.bounds()),
        };
        operation.custom(Some(&id), layout.bounds(), &mut node);
        operation.custom(
            Some(&id),
            layout.bounds(),
            &mut LiveAction {
                id: self.id.clone(),
                message: self.action.clone(),
            },
        );
        if let Some(child) = tree.children.first_mut() {
            self.inner.operate(child, layout, renderer, operation);
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
        let state = tree.state.downcast_mut::<State>();
        let was_focused = state.focused;
        if self.action.is_none() {
            state.unfocus();
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                state.focused = self.action.is_some()
                    && cursor.is_over(layout.bounds())
                    && cursor.is_over(*viewport);
                state.space_down = false;
            }
            // Keep logical focus like Iced text inputs, but never activate a
            // held Space after the user switches applications.
            Event::Window(window::Event::Unfocused) => state.space_down = false,
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) if state.focused && modifiers.is_empty() => match key {
                keyboard::Key::Named(keyboard::key::Named::Enter) => {
                    if !repeat && let Some(message) = &self.action {
                        shell.publish(message.clone());
                    }
                    shell.capture_event();
                    return;
                }
                keyboard::Key::Named(keyboard::key::Named::Space) => {
                    if !repeat {
                        state.space_down = true;
                    }
                    shell.capture_event();
                    return;
                }
                _ => {}
            },
            Event::Keyboard(keyboard::Event::KeyReleased {
                key: keyboard::Key::Named(keyboard::key::Named::Space),
                modifiers,
                ..
            }) if state.focused => {
                if state.space_down
                    && modifiers.is_empty()
                    && let Some(message) = &self.action
                {
                    shell.publish(message.clone());
                }
                state.space_down = false;
                shell.capture_event();
                return;
            }
            _ => {}
        }
        if was_focused != state.focused {
            shell.request_redraw();
        }
        if let Some(child) = tree.children.first_mut() {
            self.inner.update(
                child, event, layout, cursor, renderer, clipboard, shell, viewport,
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
            self.inner
                .draw(child, renderer, theme, style, layout, cursor, viewport);
        }
        if tree.state.downcast_ref::<State>().focused
            && self.action.is_some()
            && let Some(clip) = layout.bounds().intersection(viewport)
        {
            use iced::advanced::Renderer as _;
            renderer.with_layer(clip, |renderer| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: layout.bounds(),
                        border: Border {
                            color: theme.extended_palette().primary.strong.color,
                            width: 2.,
                            radius: 4.into(),
                        },
                        ..renderer::Quad::default()
                    },
                    Color::TRANSPARENT,
                );
            });
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
                self.inner
                    .mouse_interaction(child, layout, cursor, viewport, renderer)
            })
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.inner.overlay(
            tree.children.first_mut()?,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: Clone + 'static> From<Button<'a, Message>> for Element<'a, Message> {
    fn from(button: Button<'a, Message>) -> Self {
        Element::new(button)
    }
}
