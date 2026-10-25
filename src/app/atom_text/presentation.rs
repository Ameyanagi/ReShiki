//! Keep the focused editor mounted during a possible double-click, without
//! painting it or intercepting the second pointer click on the canvas.
use super::{Action, Message};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::time::Instant;
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};

pub(super) fn wrap<'a>(
    content: Element<'a, Message>,
    until: Option<Instant>,
) -> Element<'a, Message> {
    Element::new(Presentation { content, until })
}

struct Presentation<'a> {
    content: Element<'a, Message>,
    until: Option<Instant>,
}

impl Widget<Message, Theme, Renderer> for Presentation<'_> {
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
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        // Focus and accessibility still reach the real field. Its tree and
        // selection are identical when the dialog becomes visible.
        self.content
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
        if let Some(until) = self.until {
            match event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed { .. })
                | Event::InputMethod(
                    iced::advanced::input_method::Event::Preedit(..)
                    | iced::advanced::input_method::Event::Commit(..),
                ) => {
                    shell.publish(Message::AtomText(Action::Reveal(until)));
                }
                Event::Window(iced::window::Event::RedrawRequested(now)) => {
                    if *now >= until {
                        shell.publish(Message::AtomText(Action::Reveal(until)));
                    } else {
                        shell.request_redraw_at(until);
                    }
                }
                Event::Mouse(_) | Event::Touch(_) => return,
                _ => {}
            }
        }
        // The very event which reveals the dialog must still reach its
        // already focused editor, including the first typed key or IME commit.
        self.content.as_widget_mut().update(
            tree,
            event,
            layout,
            if self.until.is_some() {
                mouse::Cursor::Unavailable
            } else {
                cursor
            },
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
        if self.until.is_none() {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
        if self.until.is_some() {
            mouse::Interaction::None
        } else {
            self.content
                .as_widget()
                .mouse_interaction(tree, layout, cursor, viewport, renderer)
        }
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        if self.until.is_some() {
            None
        } else {
            self.content
                .as_widget_mut()
                .overlay(tree, layout, renderer, viewport, translation)
        }
    }
}
