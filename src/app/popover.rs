//! A menu or panel anchored under a toolbar button, drawn over the rows below
//! it so that nothing moves. A click outside or Escape dismisses it.
use super::Message;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector, keyboard};

const GAP: f32 = 4.;
const MARGIN: f32 = 6.;

pub struct Popover<'a> {
    anchor: Element<'a, Message>,
    popup: Option<Element<'a, Message>>,
    close: Message,
    escape: Message,
    keys: fn(&keyboard::Key) -> Option<Message>,
}

/// `popup` is shown while it is Some; `close` is sent for a click outside it.
pub fn popover<'a>(
    anchor: impl Into<Element<'a, Message>>,
    popup: Option<Element<'a, Message>>,
    close: Message,
) -> Popover<'a> {
    Popover {
        anchor: anchor.into(),
        popup,
        escape: close.clone(),
        close,
        keys: |_| None,
    }
}
impl Popover<'_> {
    pub fn on_escape(mut self, message: Message) -> Self {
        self.escape = message;
        self
    }
    /// Keys the popup handles when none of its widgets does.
    pub fn keys(mut self, keys: fn(&keyboard::Key) -> Option<Message>) -> Self {
        self.keys = keys;
        self
    }
}

impl Widget<Message, Theme, Renderer> for Popover<'_> {
    fn children(&self) -> Vec<Tree> {
        std::iter::once(&self.anchor)
            .chain(&self.popup)
            .map(Tree::new)
            .collect()
    }
    fn diff(&self, tree: &mut Tree) {
        let children: Vec<_> = std::iter::once(&self.anchor)
            .chain(&self.popup)
            .map(Element::as_widget)
            .collect();
        tree.diff_children(&children);
    }
    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.anchor.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let Some(anchor) = tree.children.first_mut() else {
            return layout::Node::default();
        };
        self.anchor.as_widget_mut().layout(anchor, renderer, limits)
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
        if let Some(anchor) = tree.children.first() {
            self.anchor
                .as_widget()
                .draw(anchor, renderer, theme, style, layout, cursor, viewport);
        }
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some(anchor) = tree.children.first_mut() {
            self.anchor
                .as_widget_mut()
                .operate(anchor, layout, renderer, operation);
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
        if let Some(anchor) = tree.children.first_mut() {
            self.anchor.as_widget_mut().update(
                anchor, event, layout, cursor, renderer, clipboard, shell, viewport,
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
            .map_or_else(Default::default, |anchor| {
                self.anchor
                    .as_widget()
                    .mouse_interaction(anchor, layout, cursor, viewport, renderer)
            })
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        let mut children = tree.children.iter_mut();
        let anchor = children.next()?;
        let (Some(popup), Some(tree)) = (self.popup.as_mut(), children.next()) else {
            return self.anchor.as_widget_mut().overlay(
                anchor,
                layout,
                renderer,
                viewport,
                translation,
            );
        };
        // Open: the popup replaces the anchor's tooltip, which would cover it.
        Some(overlay::Element::new(Box::new(Popup {
            popup,
            tree,
            anchor: layout.bounds() + translation,
            close: &self.close,
            escape: &self.escape,
            keys: self.keys,
        })))
    }
}

impl<'a> From<Popover<'a>> for Element<'a, Message> {
    fn from(popover: Popover<'a>) -> Self {
        Element::new(popover)
    }
}

struct Popup<'a, 'b> {
    popup: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    anchor: Rectangle,
    close: &'b Message,
    escape: &'b Message,
    keys: fn(&keyboard::Key) -> Option<Message>,
}

impl overlay::Overlay<Message, Theme, Renderer> for Popup<'_, '_> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let top = self.anchor.y + self.anchor.height + GAP;
        let limits = layout::Limits::new(
            Size::ZERO,
            Size::new(
                (bounds.width - 2. * MARGIN).max(0.),
                (bounds.height - top - MARGIN).max(0.),
            ),
        );
        let node = self
            .popup
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        let x = self
            .anchor
            .x
            .min(bounds.width - node.size().width - MARGIN)
            .max(MARGIN);
        node.move_to(Point::new(x, top))
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.popup.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.popup
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
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
        // Escape closes even while a field inside has focus.
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        }) = event
        {
            shell.publish(self.escape.clone());
            shell.capture_event();
            return;
        }
        let bounds = layout.bounds();
        self.popup.as_widget_mut().update(
            self.tree, event, layout, cursor, renderer, clipboard, shell, &bounds,
        );
        if shell.is_event_captured() {
            return;
        }
        match event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                if let Some(message) = (self.keys)(key) {
                    shell.publish(message);
                    shell.capture_event();
                }
            }
            // The click only dismisses; the drawing below must not act on it.
            Event::Mouse(mouse::Event::ButtonPressed(_))
                if cursor.position().is_some_and(|p| !bounds.contains(p)) =>
            {
                shell.publish(self.close.clone());
                shell.capture_event();
            }
            // The popup is opaque: the canvas tracks the pointer itself, so it
            // would act on presses and scrolling over the popup's bare areas.
            Event::Mouse(
                mouse::Event::ButtonPressed(_)
                | mouse::Event::ButtonReleased(_)
                | mouse::Event::WheelScrolled { .. },
            ) if cursor.is_over(bounds) => shell.capture_event(),
            _ => {}
        }
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if !cursor.is_over(layout.bounds()) {
            return mouse::Interaction::None;
        }
        // Anything but None hides the pointer from the widgets below.
        match self.popup.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        ) {
            mouse::Interaction::None => mouse::Interaction::Idle,
            interaction => interaction,
        }
    }
    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'c, Message, Theme, Renderer>> {
        let bounds = layout.bounds();
        self.popup
            .as_widget_mut()
            .overlay(self.tree, layout, renderer, &bounds, Vector::ZERO)
    }
}
