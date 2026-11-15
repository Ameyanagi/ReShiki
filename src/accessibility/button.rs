use super::{LiveAction, Node, Role};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{
        Id, Operation, Tree,
        operation::{self, Focusable, focusable},
        tree,
    },
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
    enabled: Option<bool>,
    checked: Option<bool>,
    expanded: Option<bool>,
    value: Option<String>,
    tab_stop: bool,
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
        enabled: None,
        checked: None,
        expanded: None,
        value: None,
        tab_stop: true,
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

    /// Override the accessible state while keeping the press action, so an
    /// unavailable command can still explain why it cannot run.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
    /// False leaves the button out of Tab traversal (list rows that arrow
    /// keys move through); it stays named and natively pressable.
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }
}

#[derive(Default)]
struct State {
    id: String,
    enabled: bool,
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
            enabled: self.action.is_some(),
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
        if state.id != self.id || state.enabled != self.action.is_some() {
            *state = State {
                id: self.id.clone(),
                enabled: self.action.is_some(),
                ..State::default()
            };
            // The inner Iced button owns a mouse-down flag. An in-flight
            // gesture belongs to the old control, including when a control
            // becomes disabled and later re-enabled before mouse-up.
            tree.children = self.children();
            return;
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
        if self.action.is_some() && self.tab_stop {
            operation.custom(Some(&id), layout.bounds(), &mut super::ButtonFocus);
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
            enabled: self.enabled.unwrap_or(self.action.is_some()),
            focused: state.focused,
            checked: self.checked,
            expanded: self.expanded,
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
        let pressed = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                | Event::Touch(touch::Event::FingerPressed { .. })
        );
        match event {
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
        if let Some(child) = tree.children.first_mut() {
            self.inner.update(
                child, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
            if pressed {
                // A press on a control inside this one, such as a tab's
                // Close, focuses only that control.
                state.focused = self.action.is_some()
                    && cursor.is_over(layout.bounds())
                    && cursor.is_over(*viewport)
                    && {
                        let mut count = focusable::count();
                        self.inner.operate(
                            child,
                            layout,
                            renderer,
                            &mut operation::black_box(&mut count),
                        );
                        !matches!(
                            count.finish(),
                            operation::Outcome::Some(focusable::Count {
                                focused: Some(_),
                                ..
                            })
                        )
                    };
                state.space_down = false;
            }
        }
        if was_focused != state.focused {
            shell.request_redraw();
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
        if tree.state.downcast_ref::<State>().focused && self.action.is_some() {
            focus_ring(renderer, theme, layout.bounds(), viewport);
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

/// Outside the control, so focus never reads as the active style's fill and
/// border: a 1 px surface gap, then a 2 px accent ring.
pub(super) fn focus_ring(
    renderer: &mut Renderer,
    theme: &Theme,
    bounds: Rectangle,
    viewport: &Rectangle,
) {
    use iced::advanced::Renderer as _;
    let ring = bounds.expand(3.);
    let Some(clip) = ring.intersection(viewport) else {
        return;
    };
    let palette = theme.extended_palette();
    renderer.with_layer(clip, |renderer| {
        for (bounds, width, radius, color) in [
            (ring, 2., 8., palette.primary.strong.color),
            (bounds.expand(1.), 1., 6., palette.background.base.color),
        ] {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border {
                        color,
                        width,
                        radius: radius.into(),
                    },
                    ..renderer::Quad::default()
                },
                Color::TRANSPARENT,
            );
        }
    });
}
