use super::Message;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{
        Id, Operation, Tree,
        operation::{Focusable, TextInput},
        tree,
    },
};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, keyboard};

/// Route file commands before text inputs see them. A subscription runs after
/// widgets and cannot prevent a command's character from entering a field.
///
/// Text fields, including those in overlays, follow one rule: Command
/// shortcuts never type their letter; Enter applies a field and leaves it, so
/// Undo and Redo then act on the drawing; while a field is focused, Undo and
/// Redo do nothing.
pub fn wrap(
    content: Element<'_, Message>,
    help_open: bool,
    image_open: bool,
    updates_open: bool,
    atom_text_open: bool,
) -> Element<'_, Message> {
    Element::new(FileShortcuts {
        content,
        help_open,
        image_open,
        updates_open,
        atom_text_open,
    })
}

struct FileShortcuts<'a> {
    content: Element<'a, Message>,
    help_open: bool,
    image_open: bool,
    updates_open: bool,
    atom_text_open: bool,
}

impl Widget<Message, Theme, Renderer> for FileShortcuts<'_> {
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
        // Updates has no text fields. Stop keys before file routing and any
        // still-focused editor underneath the dialog sees them.
        if self.updates_open && matches!(event, Event::Keyboard(_) | Event::InputMethod(_)) {
            if matches!(
                event,
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Escape),
                    ..
                })
            ) {
                shell.publish(Message::Updates(super::updates::Action::Show(false)));
            }
            shell.capture_event();
            return;
        }
        if self.image_open
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
        // A text input consumes Escape to unfocus itself. The atom-label
        // dialog promises Cancel on the first press, including while typing.
        if self.atom_text_open
            && matches!(
                event,
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::Escape),
                    ..
                })
            )
        {
            shell.publish(Message::AtomText(super::atom_text::Action::Cancel));
            shell.capture_event();
            return;
        }
        if self.help_open
            && !self.atom_text_open
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
            && let Some(message) = file_message(key, *modifiers)
        {
            shell.publish(message);
            shell.capture_event();
            return;
        }
        let untyped = without_command_text(event);
        let event = untyped.as_ref().unwrap_or(event);
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
        if let Some(mut fields) = Fields::after(event, shell.is_event_captured()) {
            self.content
                .as_widget_mut()
                .operate(tree, layout, renderer, &mut fields);
            fields.finish(shell);
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
        self.content
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
        // A popover in the workspace must not float above the modal dialog or
        // receive keys before this wrapper does.
        if self.updates_open {
            return None;
        }
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
            .map(|overlay| overlay::Element::new(Box::new(FieldKeys(overlay))))
    }
}

/// Overlays, such as the color popover, get key presses before the drawing,
/// so their fields follow the same rule here.
struct FieldKeys<'a>(overlay::Element<'a, Message, Theme, Renderer>);

impl overlay::Overlay<Message, Theme, Renderer> for FieldKeys<'_> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        self.0.as_overlay_mut().layout(renderer, bounds)
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.0
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.0.as_overlay_mut().operate(layout, renderer, operation);
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
        let untyped = without_command_text(event);
        let event = untyped.as_ref().unwrap_or(event);
        self.0
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
        if let Some(mut fields) = Fields::after(event, shell.is_event_captured()) {
            self.0
                .as_overlay_mut()
                .operate(layout, renderer, &mut fields);
            fields.finish(shell);
        }
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.0
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.0.as_overlay_mut().overlay(layout, renderer)
    }
    fn index(&self) -> f32 {
        self.0.as_overlay().index()
    }
}

/// A Command key press without the character that macOS reports for it, which
/// a focused field would otherwise insert (⌘Z typing "z"). Elsewhere Ctrl
/// reports control characters, and Ctrl+Alt may be AltGr typing a character.
fn without_command_text(event: &Event) -> Option<Event> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        modified_key,
        physical_key,
        location,
        modifiers,
        text: Some(_),
        repeat,
    }) = event
    else {
        return None;
    };
    let command = if cfg!(target_os = "macos") {
        modifiers.logo()
    } else {
        modifiers.control() && !modifiers.alt()
    };
    command.then(|| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: modified_key.clone(),
            physical_key: *physical_key,
            location: *location,
            modifiers: *modifiers,
            text: None,
            repeat: *repeat,
        })
    })
}

/// Finds the focused text field after a key press. With `leave`, a focused
/// single-line field loses focus; otherwise a focused field swallows the key.
#[derive(Default)]
struct Fields {
    leave: bool,
    single_line: bool,
    focused: bool,
}

impl Fields {
    /// Enter that a field applied leaves it; Undo and Redo that nothing
    /// handled do nothing while a field is focused.
    fn after(event: &Event, captured: bool) -> Option<Self> {
        let Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key,
            modifiers,
            ..
        }) = event
        else {
            return None;
        };
        if captured {
            matches!(key, keyboard::Key::Named(keyboard::key::Named::Enter)).then(|| Self {
                leave: true,
                ..Self::default()
            })
        } else {
            matches!(
                super::shortcuts::key_message(key, modified_key, *modifiers),
                Some(Message::Undo | Message::Redo)
            )
            .then(Self::default)
        }
    }

    fn finish(&self, shell: &mut Shell<'_, Message>) {
        if self.focused && !self.leave {
            shell.capture_event();
        }
    }
}

impl Operation for Fields {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    // A single-line field reports itself as a text input just before it
    // reports its focus; multi-line editors report only their focus.
    fn text_input(&mut self, _id: Option<&Id>, _bounds: Rectangle, _state: &mut dyn TextInput) {
        self.single_line = true;
    }
    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if state.is_focused() {
            self.focused = true;
            if self.leave && self.single_line {
                state.unfocus();
            }
        }
        self.single_line = false;
    }
}

pub(super) fn file_message(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Option<Message> {
    use super::tabs::Action as Tabs;
    if modifiers.control()
        && !modifiers.alt()
        && !modifiers.logo()
        && matches!(key, keyboard::Key::Named(keyboard::key::Named::Tab))
    {
        return Some(Message::Tabs(Tabs::Cycle(!modifiers.shift())));
    }
    if !modifiers.command() || modifiers.alt() {
        return None;
    }
    let keyboard::Key::Character(c) = key else {
        return None;
    };
    if let Some(n) = c.parse::<usize>().ok().filter(|n| (1..=9).contains(n))
        && !modifiers.shift()
    {
        return Some(Message::Tabs(Tabs::Number(n)));
    }
    Some(match c.to_ascii_lowercase().as_str() {
        "w" if !modifiers.shift() => Message::Tabs(Tabs::Close(None)),
        "p" if !modifiers.shift() => Message::Printing(super::printing::Action::Start(
            reshiki::printing::Scope::Document,
        )),
        "n" if !modifiers.shift() => Message::New,
        "o" if !modifiers.shift() => Message::Open,
        "s" if modifiers.shift() => Message::SaveAs,
        "s" => Message::Save,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn longer_chords_do_not_trigger_plain_file_commands() {
        use keyboard::{Key, Modifiers};
        let primary = if cfg!(target_os = "macos") {
            Modifiers::LOGO
        } else {
            Modifiers::CTRL
        };
        assert!(matches!(
            file_message(&Key::Character("s".into()), primary | Modifiers::SHIFT),
            Some(Message::SaveAs)
        ));
        for c in ["n", "o", "p"] {
            let key = Key::Character(c.into());
            assert!(file_message(&key, primary).is_some());
            assert!(file_message(&key, primary | Modifiers::SHIFT).is_none());
            assert!(file_message(&key, primary | Modifiers::ALT).is_none());
        }
    }
}
