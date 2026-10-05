use super::*;
use crate::app::inline_text;
use reshiki::document::Annotation;

fn selected_caption() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    app.tab.doc.annotations = vec![
        Annotation {
            id: 1,
            position: Point::new(20., 30.),
            text: "First label".into(),
            format: TextFormat {
                width_pt: Some(120.),
                line_spacing: 1.0,
                ..Default::default()
            },
        },
        Annotation {
            id: 2,
            position: Point::new(200., 30.),
            text: "Other label".into(),
            format: Default::default(),
        },
    ];
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected = vec![1];
    app.sync_typography();
    app
}

fn begin_inline(app: &mut App) {
    let _ = app.update(Message::InlineText(inline_text::Action::Begin(
        Some(1),
        Point::default(),
    )));
    assert!(app.tab.inline_text.is_some());
}

fn assert_document_undo_redo(app: &mut App, before: &Document) {
    assert!(!app.error, "{}", app.status);
    let after = app.tab.doc.clone();
    assert_ne!(&after, before);
    let _ = app.update(Message::Undo);
    assert_eq!(&app.tab.doc, before, "one undo restores the paragraph edit");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
    assert_eq!(app.tab.selected, [1]);
}

#[test]
fn selected_caption_spacing_and_width_are_separate_undoable_edits() {
    let mut app = selected_caption();
    let unselected = app.tab.doc.annotations[1].clone();
    let before = app.tab.doc.clone();
    let _ = app.update(Message::TextSpacing(1.5));
    assert_eq!(app.tab.doc.annotations[0].format.line_spacing, 1.5);
    assert_eq!(app.tab.doc.annotations[1], unselected);
    assert_document_undo_redo(&mut app, &before);

    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    let _ = app.update(Message::TextWidth(" 160 ".into()));
    assert_eq!(app.tab.doc, before, "typing updates only the width draft");
    assert_eq!(app.tab.revision, revision);
    let _ = app.update(Message::ApplyTextWidth);
    assert_eq!(app.tab.doc.annotations[0].format.width_pt, Some(160.));
    assert_eq!(app.tab.text_width_input, "160");
    assert_eq!(app.tab.doc.annotations[1], unselected);
    assert_eq!(app.tab.revision, revision + 1);
    assert_document_undo_redo(&mut app, &before);

    let before = app.tab.doc.clone();
    let _ = app.update(Message::TextWidth("  ".into()));
    let _ = app.update(Message::ApplyTextWidth);
    assert_eq!(app.tab.doc.annotations[0].format.width_pt, None);
    assert!(app.tab.text_width_input.is_empty());
    assert_eq!(app.tab.doc.annotations[1], unselected);
    assert_document_undo_redo(&mut app, &before);
}

#[test]
fn invalid_caption_width_keeps_the_draft_and_document_redo() {
    let mut app = selected_caption();
    let initial = app.tab.doc.clone();
    let _ = app.update(Message::TextSpacing(1.5));
    let _ = app.update(Message::TextWidth("180".into()));
    let _ = app.update(Message::ApplyTextWidth);
    let redo = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    let before = app.tab.doc.clone();
    let format = app.tab.caption_format.clone();
    let revision = app.tab.revision;
    for draft in ["NaN", "9.9", "2000.1", "160 pt"] {
        let _ = app.update(Message::TextWidth(draft.into()));
        let _ = app.update(Message::ApplyTextWidth);
        assert!(app.error);
        assert_eq!(
            app.status,
            "Text width must be 10–2000 pt, or blank for automatic width"
        );
        assert_eq!(app.tab.text_width_input, draft);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.caption_format, format);
        assert_eq!(app.tab.revision, revision);
        assert!(app.tab.history.can_undo());
        assert!(app.tab.history.can_redo());
    }
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, redo);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, initial);
}

#[test]
fn inline_paragraph_controls_keep_local_undo_until_one_document_commit() {
    let mut app = selected_caption();
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    begin_inline(&mut app);
    let original = app.tab.caption_format.clone();
    let _ = app.update(Message::TextSpacing(1.5));
    let spaced = app.tab.caption_format.clone();
    let _ = app.update(Message::TextWidth("160".into()));
    let _ = app.update(Message::ApplyTextWidth);
    let wrapped = app.tab.caption_format.clone();
    assert_eq!(wrapped.line_spacing, 1.5);
    assert_eq!(wrapped.width_pt, Some(160.));
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());

    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.caption_format, spaced);
    let _ = app.update(Message::TextWidth("NaN".into()));
    let _ = app.update(Message::ApplyTextWidth);
    assert!(app.error);
    assert_eq!(app.tab.text_width_input, "NaN");
    assert_eq!(app.tab.caption_format, spaced);
    assert_eq!(app.text_history_available(true), Some(true));
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.caption_format, wrapped);
    let _ = app.update(Message::Undo);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.caption_format, original);
    let _ = app.update(Message::Redo);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.caption_format, wrapped);
    let _ = app.update(Message::TextWidth(" ".into()));
    let _ = app.update(Message::ApplyTextWidth);
    assert_eq!(app.tab.caption_format.width_pt, None);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.caption_format, wrapped);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.caption_format.width_pt, None);
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());

    let _ = app.update(Message::InlineText(inline_text::Action::Finish(true)));
    assert!(app.tab.inline_text.is_none());
    assert_eq!(app.tab.doc.annotations[0].format.line_spacing, 1.5);
    assert_eq!(app.tab.doc.annotations[0].format.width_pt, None);
    assert_eq!(app.tab.doc.annotations[1], before.annotations[1]);
    assert_eq!(app.tab.revision, revision + 1);
    assert_document_undo_redo(&mut app, &before);
    let _ = app.update(Message::Undo);
    assert!(
        !app.tab.history.can_undo(),
        "the session creates one document edit"
    );
}

mod rendered {
    use super::*;
    use iced::advanced::{
        renderer::Headless,
        widget::{Operation, operation},
    };
    use iced::keyboard::{
        self, Key, Modifiers,
        key::{Code, Named, Physical},
    };
    use iced::{Event, Rectangle, Size, mouse};
    use iced_runtime::{UserInterface, user_interface::Cache};

    #[derive(Default)]
    struct Fields {
        spacing_label: Option<Rectangle>,
        width: Option<Rectangle>,
        has_width_units: bool,
        select_width: bool,
    }
    impl Operation for Fields {
        fn traverse(&mut self, children: &mut dyn FnMut(&mut dyn Operation)) {
            children(self);
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, bounds: Rectangle, text: &str) {
            if text == "Line spacing" {
                self.spacing_label = Some(bounds);
            }
            if text == "Wrap width (pt)" {
                self.has_width_units = true;
            }
        }
        fn text_input(
            &mut self,
            _: Option<&iced::widget::Id>,
            bounds: Rectangle,
            state: &mut dyn operation::TextInput,
        ) {
            assert!(
                self.width.replace(bounds).is_none(),
                "the caption panel has one width input"
            );
            if self.select_width {
                state.select_all();
            }
        }
    }

    struct Ui {
        renderer: iced::Renderer,
        cache: Cache,
        size: Size,
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
                cache: Cache::new(),
                size: Size::new(280., 440.),
            }
        }
        fn fields(&mut self, app: &App, select_width: bool) -> Fields {
            let mut ui = UserInterface::build(
                app.text_panel(),
                self.size,
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
            let mut fields = Fields {
                select_width,
                ..Default::default()
            };
            ui.operate(&self.renderer, &mut fields);
            self.cache = ui.into_cache();
            fields
        }
        fn event(
            &mut self,
            app: &App,
            event: Event,
            cursor: mouse::Cursor,
        ) -> (iced::event::Status, Vec<Message>) {
            let mut ui = UserInterface::build(
                app.text_panel(),
                self.size,
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );
            let mut messages = Vec::new();
            let (_, statuses) = ui.update(
                &[event],
                cursor,
                &mut self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut messages,
            );
            self.cache = ui.into_cache();
            (statuses[0], messages)
        }
    }

    fn key(key: Key, code: Code, text: Option<&str>) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: Physical::Code(code),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
            text: text.map(Into::into),
            repeat: false,
        })
    }

    #[tokio::test]
    #[ignore = "Opt-in headless caption inspector controls; requires a renderer"]
    async fn caption_controls_publish_spacing_width_and_enter_in_both_modes() {
        for inline in [false, true] {
            let mut app = selected_caption();
            if inline {
                begin_inline(&mut app);
            }
            let mut ui = Ui::new().await;
            let fields = ui.fields(&app, false);
            assert!(fields.has_width_units);
            let label = fields.spacing_label.expect("line spacing label");
            let cursor = mouse::Cursor::Available(iced::Point::new(
                label.x + label.width + 8.,
                label.center_y(),
            ));
            // Iced supports Command+wheel to select the next dropdown value.
            assert!(
                ui.event(
                    &app,
                    Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND)),
                    cursor
                )
                .1
                .is_empty()
            );
            let (status, mut messages) = ui.event(
                &app,
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0., y: -1. },
                }),
                cursor,
            );
            assert_eq!(status, iced::event::Status::Captured);
            assert_eq!(messages.len(), 1);
            assert!(
                matches!(messages[0], Message::TextSpacing(value) if value == 1.2),
                "the next spacing after the fixture's 1.0 is 1.2: {messages:?}"
            );
            let _ = app.update(messages.remove(0));
            assert!(
                ui.event(
                    &app,
                    Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::empty())),
                    cursor
                )
                .1
                .is_empty()
            );

            let width = ui.fields(&app, false).width.expect("wrap width input");
            let (status, messages) = ui.event(
                &app,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                mouse::Cursor::Available(width.center()),
            );
            assert_eq!(status, iced::event::Status::Captured);
            assert!(messages.is_empty(), "clicking the input only focuses it");
            // Select after focusing: TextInput::focus moves its cursor to the end.
            ui.fields(&app, true);
            for (text, code) in [
                ("1", Code::Digit1),
                ("6", Code::Digit6),
                ("0", Code::Digit0),
            ] {
                let (status, mut messages) = ui.event(
                    &app,
                    key(Key::Character(text.into()), code, Some(text)),
                    mouse::Cursor::Unavailable,
                );
                assert_eq!(status, iced::event::Status::Captured);
                assert_eq!(messages.len(), 1);
                assert!(matches!(&messages[0], Message::TextWidth(_)));
                let _ = app.update(messages.remove(0));
            }
            assert_eq!(app.tab.text_width_input, "160");
            let (status, mut messages) = ui.event(
                &app,
                key(Key::Named(Named::Enter), Code::Enter, None),
                mouse::Cursor::Unavailable,
            );
            assert_eq!(status, iced::event::Status::Captured);
            assert_eq!(messages.len(), 1);
            assert!(matches!(messages[0], Message::ApplyTextWidth));
            let _ = app.update(messages.remove(0));
            assert_eq!(app.tab.caption_format.line_spacing, 1.2);
            assert_eq!(app.tab.caption_format.width_pt, Some(160.));
            if inline {
                assert_eq!(
                    app.tab.doc, app.tab.saved,
                    "controls retain the inline draft"
                );
                assert!(!app.tab.history.can_undo());
            } else {
                assert_eq!(app.tab.doc.annotations[0].format.line_spacing, 1.2);
                assert_eq!(app.tab.doc.annotations[0].format.width_pt, Some(160.));
                app.tool = Tool::Text;
                let fields = ui.fields(&app, false);
                assert!(fields.spacing_label.is_none());
                assert!(
                    fields.width.is_none(),
                    "Text tool retains its early-return panel"
                );
            }
        }
    }
}
