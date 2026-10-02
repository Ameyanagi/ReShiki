//! Reveal and focus a numeric field even when its inspector was scrolled away.
use super::Field;
use iced::advanced::widget::{
    Id, Operation,
    operation::{Focusable, Outcome, Scrollable, TextInput, scrollable},
};
use iced::{Rectangle, Vector};

pub(super) struct FieldOperation {
    target: Id,
    field: Option<Rectangle>,
    viewport: Option<Rectangle>,
}

impl FieldOperation {
    pub(super) fn new(field: Field) -> Self {
        Self {
            target: field.id().into(),
            field: None,
            viewport: None,
        }
    }
}

impl<T: 'static> Operation<T> for FieldOperation {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _: Rectangle,
        _: Vector,
        _: &mut dyn Scrollable,
    ) {
        if id == Some(&Id::from("inspector-content")) {
            self.viewport = Some(bounds);
        }
    }

    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        if id == Some(&self.target) {
            state.focus();
        } else {
            state.unfocus();
        }
    }

    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn TextInput) {
        if id == Some(&self.target) {
            self.field = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<T> {
        let (Some(field), Some(viewport)) = (self.field, self.viewport) else {
            return Outcome::None;
        };
        // Operate sees content coordinates before the scroll translation.
        // Center the field so the section's heading and Apply remain nearby.
        let y = (field.center_y() - viewport.center_y()).max(0.);
        Outcome::Chain(Box::new(Reveal {
            target: self.target.clone(),
            y,
        }))
    }
}

struct Reveal {
    target: Id,
    y: f32,
}

impl<T> Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn text_input(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn TextInput) {
        if id == Some(&self.target) {
            // TextInput::focus moves its caret to the end, so select in the
            // following pass, after focus has been acquired.
            state.select_all();
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        _: Rectangle,
        _: Rectangle,
        _: Vector,
        state: &mut dyn Scrollable,
    ) {
        if id == Some(&Id::from("inspector-content")) {
            state.scroll_to(scrollable::AbsoluteOffset {
                x: None,
                y: Some(self.y),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Message};
    use crate::canvas::{Edit, TransformField};
    use iced::advanced::{Layout, layout, renderer::Headless, widget::Tree};
    use iced::keyboard::{self, Key, Modifiers, key};
    use iced::{Event, Size, mouse};

    fn operate(
        app: &App,
        renderer: &iced::Renderer,
        tree: &mut Tree,
        size: Size,
        mut operation: Box<dyn Operation>,
    ) {
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(tree, renderer, &layout::Limits::new(size, size));
        loop {
            view.as_widget_mut()
                .operate(tree, Layout::new(&node), renderer, operation.as_mut());
            if let Outcome::Chain(next) = operation.finish() {
                operation = next;
            } else {
                break;
            }
        }
    }

    fn send(
        app: &mut App,
        renderer: &iced::Renderer,
        tree: &mut Tree,
        size: Size,
        event: Event,
    ) -> Vec<String> {
        let mut messages = Vec::new();
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(tree, renderer, &layout::Limits::new(size, size));
        view.as_widget_mut().update(
            tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        drop(view);
        let names = messages
            .iter()
            .map(|message| format!("{message:?}"))
            .collect();
        for message in messages {
            let _ = app.update(message);
        }
        names
    }

    #[derive(Default)]
    struct Inspect {
        target: Option<Id>,
        field: Option<Rectangle>,
        viewport: Option<Rectangle>,
        translation: Vector,
        focused: Vec<Id>,
    }

    impl Operation for Inspect {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn scrollable(
            &mut self,
            id: Option<&Id>,
            bounds: Rectangle,
            _: Rectangle,
            translation: Vector,
            _: &mut dyn Scrollable,
        ) {
            if id == Some(&Id::from("inspector-content")) {
                self.viewport = Some(bounds);
                self.translation = translation;
            }
        }
        fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
            if state.is_focused() {
                self.focused.extend(id.cloned());
            }
        }
        fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, _: &mut dyn TextInput) {
            if id == self.target.as_ref() {
                self.field = Some(bounds);
            }
        }
    }

    fn inspect(
        app: &App,
        renderer: &iced::Renderer,
        tree: &mut Tree,
        size: Size,
        field: Field,
    ) -> Inspect {
        let mut view = app.view();
        tree.diff(view.as_widget());
        let node = view
            .as_widget_mut()
            .layout(tree, renderer, &layout::Limits::new(size, size));
        let mut result = Inspect {
            target: Some(field.id().into()),
            ..Default::default()
        };
        view.as_widget_mut()
            .operate(tree, Layout::new(&node), renderer, &mut result);
        result
    }

    #[tokio::test]
    #[ignore = "Opt-in renderer input check"]
    async fn handle_shortcut_focuses_selects_and_reveals_the_field_in_both_window_sizes() {
        let renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        for size in [Size::new(1280., 820.), Size::new(1040., 680.)] {
            for (target, field, replacement) in [
                (TransformField::Rotation, Field::Rotation, "15"),
                (TransformField::Scale, Field::Scale, "150"),
                (TransformField::Width, Field::Width, "150"),
                (TransformField::Height, Field::Height, "90"),
            ] {
                let mut app = super::super::tests::fixture();
                app.inspector_open = false;
                app.inspector_tab = crate::app::InspectorTab::Export;
                let before = app.tab.doc.clone();
                let _ = app.update(Message::Canvas(Edit::BeginTransform(target)));
                let mut tree = Tree::empty();
                // Exercise a previously scrolled inspector, then run the exact
                // chained widget operation returned by BeginTransform.
                operate(
                    &app,
                    &renderer,
                    &mut tree,
                    size,
                    Box::new(scrollable::snap_to(
                        "inspector-content".into(),
                        scrollable::RelativeOffset::END.into(),
                    )),
                );
                operate(
                    &app,
                    &renderer,
                    &mut tree,
                    size,
                    Box::new(FieldOperation::new(field)),
                );
                let state = inspect(&app, &renderer, &mut tree, size, field);
                assert_eq!(state.focused, vec![Id::from(field.id())]);
                let field_bounds = state.field.unwrap();
                let viewport = state.viewport.unwrap();
                assert!(
                    viewport.contains(field_bounds.center() - state.translation),
                    "{size:?} {field:?}: {field_bounds:?}, {viewport:?}, {:?}",
                    state.translation
                );
                for ch in replacement.chars() {
                    let value = ch.to_string();
                    let key = Key::Character(value.clone().into());
                    send(
                        &mut app,
                        &renderer,
                        &mut tree,
                        size,
                        Event::Keyboard(keyboard::Event::KeyPressed {
                            key: key.clone(),
                            modified_key: key,
                            physical_key: key::Physical::Code(key::Code::Digit1),
                            location: keyboard::Location::Standard,
                            modifiers: Modifiers::empty(),
                            text: Some(value.into()),
                            repeat: false,
                        }),
                    );
                }
                assert_eq!(
                    app.tab.numeric_transforms.value(field),
                    replacement,
                    "Existing text must be replaced, not appended"
                );
                assert_eq!(app.tab.doc, before, "Typing is only a draft");
                assert!(!app.tab.history.can_undo());
                let key = Key::Named(key::Named::Enter);
                let messages = send(
                    &mut app,
                    &renderer,
                    &mut tree,
                    size,
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key: key.clone(),
                        modified_key: key,
                        physical_key: key::Physical::Code(key::Code::Enter),
                        location: keyboard::Location::Standard,
                        modifiers: Modifiers::empty(),
                        text: None,
                        repeat: false,
                    }),
                );
                assert!(
                    messages
                        .iter()
                        .any(|message| message == &format!("NumericTransform(Apply({field:?}))")),
                    "{messages:?}"
                );
                assert!(!app.error, "{}", app.status);
                assert_ne!(app.tab.doc, before);
                let after = app.tab.doc.clone();
                let _ = app.update(Message::Undo);
                assert_eq!(app.tab.doc, before);
                assert!(!app.tab.history.can_undo());
                let _ = app.update(Message::Redo);
                assert_eq!(app.tab.doc, after);
            }
        }
    }
}
