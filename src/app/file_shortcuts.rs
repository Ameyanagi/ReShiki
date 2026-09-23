use super::Message;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, keyboard};

/// Route file commands before text inputs see them. A subscription runs after
/// widgets and cannot prevent a command's character from entering a field.
pub fn wrap(
    content: Element<'_, Message>,
    help_open: bool,
    image_open: bool,
) -> Element<'_, Message> {
    Element::new(FileShortcuts(content, help_open, image_open))
}

struct FileShortcuts<'a>(Element<'a, Message>, bool, bool);

impl Widget<Message, Theme, Renderer> for FileShortcuts<'_> {
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
        if self.2
            && let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event
        {
            if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                shell.publish(Message::Assistant(super::assistant::Action::ViewImage(
                    None,
                )));
            }
            shell.capture_event();
            return;
        }
        if self.1
            && let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
        {
            if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                || super::help::is_shortcut(key, *modifiers)
            {
                shell.publish(Message::ToggleHelp);
            }
            shell.capture_event();
            return;
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
            && modifiers.command()
            && let keyboard::Key::Character(c) = key
        {
            let message = match c.to_ascii_lowercase().as_str() {
                "p" => Some(Message::Printing(super::printing::Action::Start(
                    reshiki::printing::Scope::Document,
                ))),
                "n" => Some(Message::New),
                "o" => Some(Message::Open),
                "s" if modifiers.shift() => Some(Message::SaveAs),
                "s" => Some(Message::Save),
                _ => None,
            };
            if let Some(message) = message {
                shell.publish(message);
                shell.capture_event();
                return;
            }
        }
        self.0.as_widget_mut().update(
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
        self.0
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}
