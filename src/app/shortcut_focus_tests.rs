//! Real widget dispatch, followed by the same ignored-key routing as the app.
use super::{App, InspectorTab, Message, assistant, shortcuts, updates};
use iced::advanced::{
    Layout, Shell, layout, mouse,
    renderer::Headless,
    widget::{Id, Operation, Tree, operation::Focusable},
};
use iced::keyboard::{
    self, Key, Modifiers,
    key::{Code, Named, Physical},
};
use iced::{Event, Rectangle, Size};

struct Ui {
    renderer: iced::Renderer,
    tree: Tree,
    viewport: Rectangle,
}

impl Ui {
    async fn new() -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .expect("headless renderer"),
            tree: Tree::empty(),
            viewport: Rectangle::with_size(Size::new(1280., 1000.)),
        }
    }

    fn event(
        &mut self,
        app: &App,
        event: Event,
        cursor: mouse::Cursor,
    ) -> (iced::event::Status, Vec<Message>) {
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.viewport.size(), self.viewport.size()),
        );
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        view.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            cursor,
            &self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &self.viewport,
        );
        let status = shell.event_status();
        if status == iced::event::Status::Ignored
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                ..
            }) = &event
            && let Some(message) = shortcuts::key_message(key, modified_key, *modifiers)
        {
            messages.push(message);
        }
        (status, messages)
    }

    fn inspect(&mut self, app: &App, operation: &mut dyn Operation) {
        let mut view = app.view();
        self.tree.diff(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.viewport.size(), self.viewport.size()),
        );
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            operation,
        );
    }

    fn input(&mut self, app: &App, target: &'static str) -> (Rectangle, bool) {
        struct Find(Id, Option<(Rectangle, bool)>);
        impl Operation for Find {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
                if id == Some(&self.0) {
                    self.1 = Some((bounds, state.is_focused()));
                }
            }
        }
        let mut find = Find(Id::new(target), None);
        self.inspect(app, &mut find);
        find.1.expect("input in the real app view")
    }

    fn text(&mut self, app: &App, target: &'static str) -> Rectangle {
        struct Find(&'static str, Option<Rectangle>);
        impl Operation for Find {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
                if text == self.0 {
                    self.1 = Some(bounds);
                }
            }
        }
        let mut find = Find(target, None);
        self.inspect(app, &mut find);
        find.1.expect("visible dialog control")
    }

    fn click(&mut self, app: &mut App, point: iced::Point) {
        for event in [
            mouse::Event::CursorMoved { position: point },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            let (_, messages) =
                self.event(app, Event::Mouse(event), mouse::Cursor::Available(point));
            apply(app, messages);
        }
    }
}

fn apply(app: &mut App, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

fn press(key: Key, code: Code, modifiers: Modifiers, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

fn assistant_app() -> App {
    let (mut app, _) = App::new();
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Assistant;
    let _ = app.update(Message::Assistant(assistant::Action::Input(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
            "Draw ferrocene".to_owned().into(),
        )),
    )));
    app
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn first_escape_cancels_focused_atom_label_without_applying_the_draft() {
    let mut ui = Ui::new().await;
    let (mut app, _) = App::new();
    let atom = app
        .tab
        .doc
        .add_atom("N", reshiki::document::Point::default());
    app.tab.selected = vec![atom];
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::AtomText(super::atom_text::Action::Begin(Some(
        atom,
    ))));
    let bounds = ui.input(&app, "atom-text").0;
    ui.click(&mut app, bounds.center());
    let (_, messages) = ui.event(
        &app,
        press(
            Key::Character("H".into()),
            Code::KeyH,
            Modifiers::SHIFT,
            Some("H"),
        ),
        mouse::Cursor::Unavailable,
    );
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::AtomText(super::atom_text::Action::Input(_))
    )));
    apply(&mut app, messages);
    assert!(ui.input(&app, "atom-text").1);
    let (status, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(
        messages.as_slice(),
        [Message::AtomText(super::atom_text::Action::Cancel)]
    ));
    apply(&mut app, messages);
    assert!(app.tab.atom_text.is_none());
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.revision, revision);
    assert_eq!(app.tab.selected, [atom]);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn assistant_shortcuts_follow_actual_prompt_focus() {
    let mut ui = Ui::new().await;
    let mut app = assistant_app();
    let draft = app.assistant.input_text();
    let events = [
        press(
            Key::Character("v".into()),
            Code::KeyV,
            Modifiers::COMMAND,
            Some("v"),
        ),
        press(
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::COMMAND,
            None,
        ),
    ];
    // Unfocused on first display, focused by a click, then unfocused by a
    // canvas click. Keep the same widget tree through each transition.
    for focused in [false, true, false] {
        if focused {
            let bounds = ui.input(&app, "assistant-input").0;
            ui.click(&mut app, bounds.center());
        } else {
            ui.click(&mut app, iced::Point::new(500., 400.));
        }
        assert_eq!(ui.input(&app, "assistant-input").1, focused);
        for (index, event) in events.iter().enumerate() {
            let (status, messages) = ui.event(&app, event.clone(), mouse::Cursor::Unavailable);
            if focused {
                assert_eq!(status, iced::event::Status::Captured);
                assert!(
                    matches!(messages.as_slice(),
                        [Message::Assistant(assistant::Action::Paste { image_only: false })] if index == 0
                    ) || matches!(messages.as_slice(),
                        [Message::Assistant(assistant::Action::Send)] if index == 1
                    ),
                    "{messages:?}"
                );
            } else {
                assert_eq!(status, iced::event::Status::Ignored);
                assert!(!messages.iter().any(|m| matches!(m, Message::Assistant(_))));
                if index == 0 {
                    assert!(matches!(messages.as_slice(), [Message::Paste]));
                }
            }
        }
    }
    // Dispatch only: no real clipboard read or Assistant request is started.
    assert_eq!(app.assistant.input_text(), draft);
    assert!(!app.assistant.busy);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in renderer input check"]
async fn updates_dialog_captures_keys_before_editors_file_commands_and_canvas() {
    let mut ui = Ui::new().await;
    let mut app = assistant_app();
    let atom = app
        .tab
        .doc
        .add_atom("C", reshiki::document::Point::default());
    let bounds = ui.input(&app, "assistant-input").0;
    ui.click(&mut app, bounds.center());
    assert!(ui.input(&app, "assistant-input").1);
    app.tab.selected = vec![atom];
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    let draft = app.assistant.input_text();
    let _ = app.update(Message::Updates(updates::Action::Show(true)));
    let mut events = vec![
        press(
            Key::Named(Named::Delete),
            Code::Delete,
            Modifiers::empty(),
            None,
        ),
        press(
            Key::Named(Named::ArrowRight),
            Code::ArrowRight,
            Modifiers::empty(),
            None,
        ),
        press(Key::Named(Named::F1), Code::F1, Modifiers::empty(), None),
        press(
            Key::Character("N".into()),
            Code::KeyN,
            Modifiers::SHIFT,
            Some("N"),
        ),
        press(
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::COMMAND,
            None,
        ),
        press(Key::Named(Named::Tab), Code::Tab, Modifiers::CTRL, None),
        Event::InputMethod(iced::advanced::input_method::Event::Commit("窒素".into())),
    ];
    events.extend(
        [
            ("v", Code::KeyV),
            ("n", Code::KeyN),
            ("o", Code::KeyO),
            ("s", Code::KeyS),
            ("w", Code::KeyW),
            ("p", Code::KeyP),
            ("1", Code::Digit1),
        ]
        .map(|(text, code)| {
            press(
                Key::Character(text.into()),
                code,
                Modifiers::COMMAND,
                Some(text),
            )
        }),
    );
    for event in events {
        let (status, messages) = ui.event(&app, event.clone(), mouse::Cursor::Unavailable);
        assert_eq!(status, iced::event::Status::Captured, "{event:?}");
        assert!(messages.is_empty(), "{event:?}: {messages:?}");
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.selected, [atom]);
        assert_eq!(app.assistant.input_text(), draft);
        assert!(!app.tab.history.can_undo());
        assert!(app.updates.open);
        assert!(!app.help_open);
    }
    let (status, messages) = ui.event(
        &app,
        press(
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        ),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(
        messages.as_slice(),
        [Message::Updates(updates::Action::Show(false))]
    ));
    apply(&mut app, messages);
    assert!(!app.updates.open);
    let _ = app.update(Message::Updates(updates::Action::Show(true)));
    let close = ui.text(&app, "Close");
    ui.click(&mut app, close.center());
    assert!(
        !app.updates.open,
        "the modal still accepts its mouse controls"
    );
    assert_eq!(app.tab.doc, drawing);
    assert!(!app.tab.history.can_undo());
}
