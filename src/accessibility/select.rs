//! A named combo box around a pick list or combo box, for assistive technology.

use super::{LiveValueAction, Node, Role, ValueQuery};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{
        Id, Operation, Tree,
        operation::{self, Focusable, focusable},
        tree,
    },
};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, keyboard, touch};

/// A named combo box around an Iced pick list or combo box. Its value is what
/// the control shows; ↑ and ↓ choose the option before or after it, and
/// assistive technology chooses an option by setting the value to its name.
/// `options` leaves out menu entries that open something else.
/// A pick list takes keyboard focus here; a combo box's own search field
/// keeps it.
pub struct Select<'a, T, Message> {
    id: String,
    name: String,
    options: Vec<T>,
    value: String,
    on_select: Box<dyn Fn(T) -> Message + 'a>,
    inner: Element<'a, Message>,
}

pub fn select<'a, T: ToString + Clone, Message: Clone + 'static>(
    id: impl Into<String>,
    name: impl Into<String>,
    options: Vec<T>,
    value: impl Into<String>,
    on_select: impl Fn(T) -> Message + 'a,
    inner: impl Into<Element<'a, Message>>,
) -> Select<'a, T, Message> {
    Select {
        id: id.into(),
        name: name.into(),
        options,
        value: value.into(),
        on_select: Box::new(on_select),
        inner: inner.into(),
    }
}

#[derive(Default)]
struct State {
    focused: bool,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<T: ToString + Clone, Message> Select<'_, T, Message> {
    /// The option with this name.
    fn option(&self, name: &str) -> Option<usize> {
        self.options.iter().position(|o| o.to_string() == name)
    }
    /// Whether the inner widget has a focusable field, and whether it has focus.
    fn inner_focus(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
    ) -> (bool, bool) {
        let mut count = focusable::count();
        self.inner.as_widget_mut().operate(
            tree,
            layout,
            renderer,
            &mut operation::black_box(&mut count),
        );
        match count.finish() {
            operation::Outcome::Some(count) => (count.total > 0, count.focused.is_some()),
            _ => (false, false),
        }
    }
}

impl<T: ToString + Clone, Message: Clone + 'static> Widget<Message, Theme, Renderer>
    for Select<'_, T, Message>
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.inner)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.inner));
    }
    fn size(&self) -> Size<Length> {
        self.inner.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.inner.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let [child] = tree.children.as_mut_slice() else {
            return layout::Node::new(Size::ZERO);
        };
        self.inner.as_widget_mut().layout(child, renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let [child] = tree.children.as_mut_slice() else {
            return;
        };
        let id = Id::from(self.id.clone());
        let (field, field_focused) = self.inner_focus(child, layout, renderer);
        let state = tree.state.downcast_mut::<State>();
        if field {
            state.unfocus();
        } else {
            operation.custom(Some(&id), layout.bounds(), &mut super::ButtonFocus);
            operation.focusable(Some(&id), layout.bounds(), state);
        }
        operation.custom(
            Some(&id),
            layout.bounds(),
            &mut Node {
                id: self.id.clone(),
                name: self.name.clone(),
                role: Role::ComboBox,
                enabled: true,
                focused: state.focused || field_focused,
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
                    message: (self.options.iter())
                        .find(|option| option.to_string() == value)
                        .map(|option| (self.on_select)(option.clone())),
                },
            );
        }
        self.inner
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
        let [child] = tree.children.as_mut_slice() else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        let focused = state.focused;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                let over = cursor.is_over(layout.bounds()) && cursor.is_over(*viewport);
                state.focused = over && !self.inner_focus(child, layout, renderer).0;
                let own = state.focused;
                if own != focused {
                    shell.request_redraw();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if focused && modifiers.is_empty() =>
            {
                let step = match key {
                    keyboard::Key::Named(keyboard::key::Named::ArrowUp) => -1,
                    keyboard::Key::Named(keyboard::key::Named::ArrowDown) => 1,
                    _ => 0,
                };
                if step != 0 {
                    let current = self.option(&self.value);
                    let next = current.map_or(0, |i| i.saturating_add_signed(step));
                    if Some(next) != current
                        && let Some(option) = self.options.get(next)
                    {
                        shell.publish((self.on_select)(option.clone()));
                    }
                    shell.capture_event();
                    return;
                }
            }
            _ => {}
        }
        self.inner.as_widget_mut().update(
            child, event, layout, cursor, renderer, clipboard, shell, viewport,
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
        let [child] = tree.children.as_slice() else {
            return;
        };
        self.inner
            .as_widget()
            .draw(child, renderer, theme, style, layout, cursor, viewport);
        if tree.state.downcast_ref::<State>().focused {
            super::button::focus_ring(renderer, theme, layout.bounds(), viewport);
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
        let [child] = tree.children.as_slice() else {
            return mouse::Interaction::None;
        };
        self.inner
            .as_widget()
            .mouse_interaction(child, layout, cursor, viewport, renderer)
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let [child] = tree.children.as_mut_slice() else {
            return None;
        };
        self.inner
            .as_widget_mut()
            .overlay(child, layout, renderer, viewport, translation)
    }
}

impl<'a, T: ToString + Clone + 'a, Message: Clone + 'static> From<Select<'a, T, Message>>
    for Element<'a, Message>
{
    fn from(select: Select<'a, T, Message>) -> Self {
        Element::new(select)
    }
}
