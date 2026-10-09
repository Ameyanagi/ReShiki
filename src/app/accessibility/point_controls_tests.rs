//! Exercise the selected-graphic controls in the actual App widget tree.
use super::{App, Message};
use crate::canvas::{Edit, Tool};
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
use reshiki::accessibility::{Activate, Collect, FocusControl, Role, Snapshot, tree::NativeTree};
use reshiki::document::Point;
use reshiki::graphics::GraphicKind;

struct Ui {
    renderer: iced::Renderer,
    cache: Cache,
    size: Size,
}

impl Ui {
    async fn new(size: Size) -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .expect("headless renderer"),
            cache: Cache::new(),
            size,
        }
    }

    fn snapshot(&mut self, app: &App) -> Snapshot {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut collect = Collect::new(Rectangle::with_size(self.size));
        ui.operate(&self.renderer, &mut operation::black_box(&mut collect));
        let snapshot = collect.snapshot().clone();
        self.cache = ui.into_cache();
        assert!(snapshot.duplicate_ids.is_empty());
        NativeTree::default()
            .update(&snapshot, "ReShiki", Rectangle::with_size(self.size), 2.)
            .expect("selected graphics publish a valid native tree");
        snapshot
    }

    fn focus(&mut self, app: &App, id: &str) {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut focus: Box<dyn Operation> = Box::new(FocusControl::new(id));
        loop {
            ui.operate(&self.renderer, focus.as_mut());
            match focus.finish() {
                operation::Outcome::Chain(next) => focus = next,
                _ => break,
            }
        }
        self.cache = ui.into_cache();
        self.assert_focused(app, id);
    }

    fn assert_focused(&mut self, app: &App, id: &str) {
        let snapshot = self.snapshot(app);
        let focused: Vec<_> = snapshot.nodes.iter().filter(|node| node.focused).collect();
        assert_eq!(focused.len(), 1, "expected one focused control: {id}");
        assert_eq!(focused[0].id, id);
        assert!(focused[0].visible_bounds.is_some(), "focus reveals {id}");
    }

    fn activate(&mut self, app: &App, id: &str) -> operation::Outcome<Message> {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut activate = Activate::<Message>::new(id);
        ui.operate(&self.renderer, &mut operation::black_box(&mut activate));
        self.cache = ui.into_cache();
        activate.finish()
    }

    fn event(&mut self, app: &App, event: Event) -> (iced::event::Status, Vec<Message>) {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            std::slice::from_ref(&event),
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        self.cache = ui.into_cache();
        let status = statuses[0];
        // Match the app's ignored-key subscription after real widget/overlay
        // dispatch. In particular, Escape returns from EditPoints to Select.
        if status == iced::event::Status::Ignored
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                ..
            }) = &event
            && let Some(message) = crate::app::shortcuts::key_message(key, modified_key, *modifiers)
        {
            messages.push(message);
        }
        (status, messages)
    }

    fn tab(&mut self, app: &App, backwards: bool) {
        let modifiers = if backwards {
            Modifiers::SHIFT
        } else {
            Modifiers::empty()
        };
        self.modifiers(app, modifiers);
        let (status, messages) = self.event(app, key(Named::Tab, modifiers, false));
        assert_eq!(status, iced::event::Status::Captured);
        assert!(messages.is_empty(), "Tab only changes focus");
        assert!(
            self.event(app, release(Key::Named(Named::Tab), Code::Tab, modifiers))
                .1
                .is_empty()
        );
        self.modifiers(app, Modifiers::empty());
    }

    fn modifiers(&mut self, app: &App, modifiers: Modifiers) {
        let (_, messages) = self.event(
            app,
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
        );
        assert!(
            messages.is_empty(),
            "modifier changes only update key state"
        );
    }

    fn command(
        &mut self,
        app: &App,
        text: &str,
        code: Code,
    ) -> (iced::event::Status, Vec<Message>) {
        self.modifiers(app, Modifiers::COMMAND);
        let result = self.event(app, character(text, code, Modifiers::COMMAND));
        assert!(
            self.event(
                app,
                release(Key::Character(text.into()), code, Modifiers::COMMAND),
            )
            .1
            .is_empty()
        );
        self.modifiers(app, Modifiers::empty());
        result
    }

    fn enter_once(&mut self, app: &App, tool: Tool) -> Message {
        let message = self.activate_focused(app);
        assert!(matches!(&message, Message::Tool(actual) if *actual == tool));
        message
    }

    fn activate_focused(&mut self, app: &App) -> Message {
        let (status, mut messages) = self.event(app, key(Named::Enter, Modifiers::empty(), false));
        assert_eq!(status, iced::event::Status::Captured);
        assert_eq!(
            messages.len(),
            1,
            "Enter activates the control exactly once"
        );
        let (status, repeats) = self.event(app, key(Named::Enter, Modifiers::empty(), true));
        assert_eq!(status, iced::event::Status::Captured);
        assert!(repeats.is_empty(), "held Enter does not activate again");
        let release = Event::Keyboard(keyboard::Event::KeyReleased {
            key: Key::Named(Named::Enter),
            modified_key: Key::Named(Named::Enter),
            physical_key: Physical::Code(Code::Enter),
            location: keyboard::Location::Standard,
            modifiers: Modifiers::empty(),
        });
        assert!(self.event(app, release).1.is_empty());
        messages.remove(0)
    }

    fn type_width(&mut self, app: &mut App, id: &str) {
        self.focus(app, id);
        let (status, messages) = self.command(app, "a", Code::KeyA);
        assert_eq!(status, iced::event::Status::Captured);
        assert!(
            messages.is_empty(),
            "Select All only selects the input text"
        );
        for (text, code) in [
            ("1", Code::Digit1),
            (".", Code::Period),
            ("5", Code::Digit5),
        ] {
            let (status, messages) = self.event(app, character(text, code, Modifiers::empty()));
            assert_eq!(status, iced::event::Status::Captured);
            assert_eq!(messages.len(), 1);
            assert!(matches!(
                messages[0],
                Message::ArrowAction(crate::app::arrows::Action::Number(
                    crate::app::arrows::Field::Line,
                    _
                )) | Message::Graphics(crate::app::graphics::Action::Width(_))
            ));
            for message in messages {
                let _ = app.update(message);
            }
        }
        let snapshot = self.snapshot(app);
        let field = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
        assert_eq!(field.value.as_deref(), Some("1.5"));
    }

    fn undo(&mut self, app: &mut App) {
        let (_, messages) = self.command(app, "z", Code::KeyZ);
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0], Message::Undo));
        for message in messages {
            let _ = app.update(message);
        }
    }
}

fn release(key: Key, code: Code, modifiers: Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyReleased {
        modified_key: key.clone(),
        key,
        physical_key: Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
    })
}

fn character(text: &str, code: Code, modifiers: Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character(text.into()),
        modified_key: Key::Character(text.into()),
        physical_key: Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: modifiers.is_empty().then(|| text.into()),
        repeat: false,
    })
}

fn key(named: Named, modifiers: Modifiers, repeat: bool) -> Event {
    let code = match named {
        Named::Tab => Code::Tab,
        Named::Enter => Code::Enter,
        Named::Escape => Code::Escape,
        Named::F1 => Code::F1,
        _ => unreachable!("test only dispatches Tab, Enter, Escape and F1"),
    };
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(named),
        modified_key: Key::Named(named),
        physical_key: Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat,
    })
}

fn selected_graphic(kind: GraphicKind, size: Size) -> App {
    let (mut app, _) = App::new();
    app.viewport = size;
    let drawn = if kind == GraphicKind::Path {
        GraphicKind::Curve
    } else {
        kind
    };
    let _ = app.update(Message::Tool(Tool::Graphic(drawn)));
    let _ = app.update(Message::Canvas(Edit::Graphic(
        Point::new(-60., -30.),
        Point::new(60., 30.),
        false,
    )));
    if kind == GraphicKind::Path {
        let id = app.tab.selected[0];
        let _ = app.update(Message::Tool(Tool::EditPoints));
        let _ = app.update(Message::Canvas(Edit::GraphicPoint(
            id,
            1,
            Point::new(20., -50.),
        )));
        let _ = app.update(Message::Tool(Tool::Select));
    }
    assert_eq!(app.tool, Tool::Select);
    assert_eq!(app.tab.selected.len(), 1);
    assert_eq!(app.tab.doc.graphics[0].kind, kind);
    app
}

#[tokio::test]
#[ignore = "Opt-in real mechanism geometry button metadata and action regression"]
async fn mechanism_geometry_buttons_publish_native_actions_and_one_keyboard_undo() {
    use crate::app::arrows::Action;
    use reshiki::document::Document;
    let size = Size::new(1280., 820.);
    let mut ui = Ui::new(size).await;
    for (id, label) in [
        ("arrow.reverse", "Reverse"),
        ("arrow.flip-bend", "Flip bend"),
        ("arrow.straighten", "Straighten"),
    ] {
        ui.cache = Cache::new();
        let (mut app, _) = App::new();
        app.viewport = size;
        app.tab.doc = Document::from_native_file(include_bytes!(
            "../../../tests/fixtures/mechanism-curvature-91/after.rsk"
        ))
        .unwrap();
        app.tab.saved = app.tab.doc.clone();
        let _ = app.update(Message::Canvas(Edit::Select(vec![101])));
        let before = app.tab.doc.clone();
        let snapshot = ui.snapshot(&app);
        let button = snapshot.nodes.iter().find(|node| node.id == id).expect(id);
        assert_eq!(button.name, label);
        assert_eq!(button.role, Role::Button);
        assert!(button.enabled);
        let operation::Outcome::Some(native_message) = ui.activate(&app, id) else {
            panic!("native activation publishes {id}")
        };
        assert!(matches!(
            (&native_message, id),
            (Message::ArrowAction(Action::Reverse), "arrow.reverse")
                | (Message::ArrowAction(Action::Flip), "arrow.flip-bend")
                | (Message::ArrowAction(Action::Straighten), "arrow.straighten")
        ));
        assert_eq!(
            app.tab.doc, before,
            "collecting or activating metadata alone does not edit"
        );
        ui.focus(&app, id);
        let message = ui.activate_focused(&app);
        assert_eq!(
            std::mem::discriminant(&message),
            std::mem::discriminant(&native_message)
        );
        assert!(matches!(
            (&message, id),
            (Message::ArrowAction(Action::Reverse), "arrow.reverse")
                | (Message::ArrowAction(Action::Flip), "arrow.flip-bend")
                | (Message::ArrowAction(Action::Straighten), "arrow.straighten")
        ));
        let _ = app.update(message);
        assert_ne!(app.tab.doc, before);
        assert_eq!(app.tab.doc.bonds, before.bonds);
        assert_eq!(app.tab.doc.arrows[1], before.arrows[1]);
        ui.undo(&mut app);
        assert_eq!(app.tab.doc, before, "one geometry action is one Undo");
    }
}

#[tokio::test]
#[ignore = "Opt-in real pen controls accessibility and keyboard regression"]
async fn pen_node_buttons_publish_actions_and_keyboard_insert_is_one_undo() {
    let size = Size::new(1280., 1000.);
    let mut ui = Ui::new(size).await;
    let mut app = selected_graphic(GraphicKind::Path, size);
    let _ = app.update(Message::Graphics(crate::app::graphics::Action::Path(
        crate::app::graphics::path::Action::Node(0),
    )));
    let snapshot = ui.snapshot(&app);
    for id in [
        "pen-new",
        "pen-finish",
        "pen-insert",
        "pen-delete",
        "pen-straight",
        "pen-curved",
        "pen-close",
        "pen-continue",
    ] {
        let node = snapshot.nodes.iter().find(|node| node.id == id).expect(id);
        assert_eq!(node.role, Role::Button);
        assert!(!node.name.is_empty());
    }
    assert!(
        !snapshot
            .nodes
            .iter()
            .find(|node| node.id == "pen-delete")
            .unwrap()
            .enabled
    );
    assert!(
        !snapshot
            .nodes
            .iter()
            .find(|node| node.id == "pen-close")
            .unwrap()
            .enabled
    );
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    ui.focus(&app, "pen-insert");
    assert_eq!(
        app.tab.revision, revision,
        "focus alone never edits the path"
    );
    let message = ui.activate_focused(&app);
    assert!(matches!(
        message,
        Message::Graphics(crate::app::graphics::Action::Path(
            crate::app::graphics::path::Action::Insert
        ))
    ));
    let _ = app.update(message);
    assert_eq!(
        app.tab.doc.graphics[0]
            .path_handles()
            .unwrap()
            .iter()
            .filter(|handle| handle.node)
            .count(),
        3
    );
    ui.undo(&mut app);
    assert_eq!(app.tab.doc, before, "one keyboard insert is one Undo");
}

#[tokio::test]
#[ignore = "Opt-in real selected-graphic keyboard and accessibility regression"]
async fn selected_arc_and_curve_point_controls_are_reachable_and_exit_without_editing() {
    for size in [Size::new(1280., 820.), Size::new(1040., 680.)] {
        let mut ui = Ui::new(size).await;
        for kind in [GraphicKind::Arc, GraphicKind::Curve, GraphicKind::Path] {
            ui.cache = Cache::new();
            let mut app = selected_graphic(kind, size);
            let drawing = app.tab.doc.clone();
            let selected = app.tab.selected.clone();
            let revision = app.tab.revision;
            let (entry_id, entry_name) = if kind == GraphicKind::Arc {
                ("arc-edit-endpoints", "Edit arc endpoints")
            } else {
                ("curve-edit-points", "Edit curve points")
            };
            let snapshot = ui.snapshot(&app);
            let entry = snapshot
                .nodes
                .iter()
                .find(|node| node.id == entry_id)
                .unwrap();
            assert_eq!(entry.name, entry_name);
            assert_eq!(entry.role, Role::Button);
            assert!(entry.enabled);
            assert!(
                !snapshot
                    .nodes
                    .iter()
                    .any(|node| node.id == "edit-points-done")
            );

            if kind == GraphicKind::Arc {
                ui.focus(&app, "arc-start");
                ui.tab(&app, false);
                ui.assert_focused(&app, "arc-sweep");
                ui.tab(&app, false);
                ui.assert_focused(&app, entry_id);
                ui.tab(&app, true);
                ui.assert_focused(&app, "arc-sweep");
                ui.tab(&app, false);
            } else {
                ui.focus(&app, entry_id);
                ui.tab(&app, true);
                ui.tab(&app, false);
            }
            ui.assert_focused(&app, entry_id);
            let message = ui.enter_once(&app, Tool::EditPoints);
            let _ = app.update(message);
            assert_eq!(app.tool, Tool::EditPoints);
            let editing = ui.snapshot(&app);
            assert!(!editing.nodes.iter().any(|node| node.id == entry_id));
            let done = editing
                .nodes
                .iter()
                .find(|node| node.id == "edit-points-done")
                .unwrap();
            assert_eq!(done.name, "Finish editing points");
            assert_eq!(done.role, Role::Button);
            assert!(done.enabled);
            ui.focus(&app, "edit-points-done");
            ui.tab(&app, true);
            ui.tab(&app, false);
            ui.assert_focused(&app, "edit-points-done");
            let message = ui.enter_once(&app, Tool::Select);
            let _ = app.update(message);
            assert_eq!(app.tool, Tool::Select);

            // The same live native activation re-enters the mode. A foreground
            // dialog hides both point controls and cannot activate the Done below.
            let operation::Outcome::Some(message) = ui.activate(&app, entry_id) else {
                panic!("native activation must find the selected-graphic entry");
            };
            assert!(matches!(message, Message::Tool(Tool::EditPoints)));
            let _ = app.update(message);
            let _ = app.update(Message::ToggleHelp);
            let modal = ui.snapshot(&app);
            assert!(modal.nodes.iter().all(|node| node.id.starts_with("help-")));
            assert!(matches!(
                ui.activate(&app, entry_id),
                operation::Outcome::None
            ));
            assert!(matches!(
                ui.activate(&app, "edit-points-done"),
                operation::Outcome::None
            ));
            let (_, messages) = ui.event(&app, key(Named::Escape, Modifiers::empty(), false));
            assert_eq!(messages.len(), 1);
            for message in messages {
                let _ = app.update(message);
            }
            assert!(!app.help_open);
            assert_eq!(
                app.tool,
                Tool::EditPoints,
                "Escape first closes the foreground"
            );
            ui.focus(&app, "edit-points-done");
            let (_, messages) = ui.event(&app, key(Named::Escape, Modifiers::empty(), false));
            assert_eq!(messages.len(), 1);
            assert!(matches!(messages[0], Message::Escape));
            for message in messages {
                let _ = app.update(message);
            }
            assert_eq!(app.tool, Tool::Select);
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.selected, selected);
            assert_eq!(app.tab.revision, revision);
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in real selected-arrow and graphic width input regression"]
async fn selected_width_fields_publish_units_and_apply_one_undo_step() {
    use crate::app::arrows::{Action, Field};
    use reshiki::document::Document;

    for size in [Size::new(1280., 820.), Size::new(1040., 680.)] {
        let mut ui = Ui::new(size).await;
        for selected in [vec![10], vec![1, 2, 3, 10, 20]] {
            ui.cache = Cache::new();
            let (mut app, _) = App::new();
            app.viewport = size;
            app.tab.doc = Document::from_json(include_bytes!(
                "../../../tests/fixtures/ui-declutter/mixed-arrow-width.rsk"
            ))
            .unwrap();
            app.tab.saved = app.tab.doc.clone();
            let _ = app.update(Message::Canvas(Edit::Select(selected.clone())));
            let before = app.tab.doc.clone();
            assert!(!app.tab.history.can_undo());
            let snapshot = ui.snapshot(&app);
            let field = snapshot
                .nodes
                .iter()
                .find(|node| node.id == "arrow-line-width")
                .unwrap();
            assert_eq!(field.name, "Arrow line width (pt)");
            assert_eq!(field.role, Role::TextInput);
            assert_eq!(field.value.as_deref(), Some("1.2"));
            assert!(field.enabled);
            if selected.contains(&20) {
                let graphic = snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == "graphic-line-width")
                    .unwrap();
                assert_eq!(graphic.name, "Graphic line width (pt)");
                assert_eq!(graphic.value.as_deref(), Some("0.3"));
            }

            ui.type_width(&mut app, "arrow-line-width");
            assert_eq!(app.tab.doc, before, "typing only changes the width draft");
            assert!(!app.tab.history.can_undo());
            let (status, messages) = ui.event(&app, key(Named::Enter, Modifiers::empty(), false));
            assert_eq!(status, iced::event::Status::Captured);
            assert_eq!(messages.len(), 1);
            assert!(matches!(
                messages[0],
                Message::ArrowAction(Action::ApplyNumber(Field::Line))
            ));
            for message in messages {
                let _ = app.update(message);
            }
            assert!(!app.error, "{}", app.status);
            let mut expected = before.clone();
            expected
                .arrows
                .iter_mut()
                .find(|arrow| arrow.id == 10)
                .unwrap()
                .style
                .as_mut()
                .unwrap()
                .width_pt = 1.5;
            assert_eq!(
                app.tab.doc, expected,
                "only the selected arrow width changes"
            );
            assert_eq!(app.tab.doc.bonds, before.bonds);
            assert_eq!(app.tab.doc.drawing_style, before.drawing_style);
            assert_eq!(app.tab.selected, selected);
            ui.undo(&mut app);
            assert_eq!(app.tab.doc, before);
            assert_eq!(app.tab.selected, selected);
            assert!(
                !app.tab.history.can_undo(),
                "one Enter produces one Undo step"
            );
        }

        ui.cache = Cache::new();
        let mut app = selected_graphic(GraphicKind::Arc, size);
        let before = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let snapshot = ui.snapshot(&app);
        let field = snapshot
            .nodes
            .iter()
            .find(|node| node.id == "graphic-line-width")
            .unwrap();
        assert_eq!(field.name, "Graphic line width (pt)");
        assert_eq!(field.role, Role::TextInput);
        assert_eq!(field.value.as_deref(), Some("0.6"));
        assert!(field.enabled);
        ui.type_width(&mut app, "graphic-line-width");
        assert_eq!(app.tab.doc, before);
        let (status, messages) = ui.event(&app, key(Named::Enter, Modifiers::empty(), false));
        assert_eq!(status, iced::event::Status::Captured);
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0],
            Message::Graphics(crate::app::graphics::Action::ApplyWidth)
        ));
        for message in messages {
            let _ = app.update(message);
        }
        let mut expected = before.clone();
        expected.graphics[0].style.width_pt = 1.5;
        assert_eq!(app.tab.doc, expected);
        ui.undo(&mut app);
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, selected);
    }
}

#[tokio::test]
#[ignore = "Opt-in real Help open/close focus and input-selection regression"]
async fn help_returns_keyboard_focus_and_preserves_the_underlying_input_selection() {
    for size in [Size::new(1280., 820.), Size::new(1040., 680.)] {
        let mut ui = Ui::new(size).await;
        let mut app = selected_graphic(GraphicKind::Arc, size);
        let drawing = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        let revision = app.tab.revision;

        ui.focus(&app, "help-open");
        ui.tab(&app, false);
        let next = ui
            .snapshot(&app)
            .nodes
            .into_iter()
            .find(|node| node.focused)
            .expect("Tab after Help reaches a control")
            .id;
        assert_ne!(next, "header-about");
        ui.focus(&app, "help-open");
        let before = ui.snapshot(&app);
        let bounds = |snapshot: &Snapshot| {
            snapshot
                .nodes
                .iter()
                .map(|node| (node.id.clone(), node.bounds, node.visible_bounds))
                .collect::<Vec<_>>()
        };

        for close_with_done in [false, true] {
            let message = ui.activate_focused(&app);
            assert!(matches!(message, Message::ToggleHelp));
            let _ = app.update(message);
            assert!(app.help_open);
            let modal = ui.snapshot(&app);
            assert_eq!(modal.nodes.len(), 3);
            assert!(modal.nodes.iter().all(|node| node.id.starts_with("help-")));
            assert!(matches!(
                ui.activate(&app, "help-open"),
                operation::Outcome::None
            ));
            assert!(matches!(
                ui.activate(&app, "arc-edit-endpoints"),
                operation::Outcome::None
            ));

            // Modal traversal stays inside Help, including wrapping and reverse
            // traversal, without unfocusing the inert opener underneath it.
            for id in ["help-close", "help-examples", "help-done", "help-close"] {
                ui.tab(&app, false);
                ui.assert_focused(&app, id);
            }
            ui.tab(&app, true);
            ui.assert_focused(&app, "help-done");
            let message = if close_with_done {
                ui.activate_focused(&app)
            } else {
                let (status, mut messages) =
                    ui.event(&app, key(Named::Escape, Modifiers::empty(), false));
                assert_eq!(status, iced::event::Status::Captured);
                assert_eq!(messages.len(), 1);
                messages.remove(0)
            };
            assert!(matches!(message, Message::ToggleHelp));
            let _ = app.update(message);
            assert!(!app.help_open);
            ui.assert_focused(&app, "help-open");
            assert_eq!(bounds(&ui.snapshot(&app)), bounds(&before));
            ui.tab(&app, false);
            ui.assert_focused(&app, &next);
            ui.tab(&app, true);
            ui.assert_focused(&app, "help-open");
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.selected, selected);
            assert_eq!(app.tab.revision, revision);
        }

        // F1 is also available while an input owns the keyboard. Preserve its
        // actual selection, not just a semantic focus flag or its draft value.
        ui.type_width(&mut app, "graphic-line-width");
        assert!(ui.command(&app, "a", Code::KeyA).1.is_empty());
        let (_, mut messages) = ui.event(&app, key(Named::F1, Modifiers::empty(), false));
        assert_eq!(messages.len(), 1);
        let message = messages.remove(0);
        assert!(matches!(message, Message::ToggleHelp));
        let _ = app.update(message);
        assert!(app.help_open);
        ui.tab(&app, false);
        ui.assert_focused(&app, "help-close");
        let (_, mut messages) = ui.event(&app, key(Named::Escape, Modifiers::empty(), false));
        assert_eq!(messages.len(), 1);
        let message = messages.remove(0);
        assert!(matches!(message, Message::ToggleHelp));
        let _ = app.update(message);
        ui.assert_focused(&app, "graphic-line-width");
        let (status, mut messages) =
            ui.event(&app, character("2", Code::Digit2, Modifiers::empty()));
        assert_eq!(status, iced::event::Status::Captured);
        assert_eq!(messages.len(), 1);
        let message = messages.remove(0);
        assert!(
            matches!(&message, Message::Graphics(crate::app::graphics::Action::Width(value)) if value == "2")
        );
        let _ = app.update(message);
        assert_eq!(app.tab.graphic_width_input, "2");
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.selected, selected);
        assert_eq!(app.tab.revision, revision);
    }
}

#[tokio::test]
#[ignore = "Opt-in real Import, Export and Arrange popup focus-return regression"]
async fn changed_popovers_keep_the_keyboard_opener_focused_after_closing() {
    use crate::app::{InspectorTab, inspector};

    for size in [Size::new(1280., 820.), Size::new(1040., 680.)] {
        let mut ui = Ui::new(size).await;
        for popup in ["import", "export", "arrange"] {
            ui.cache = Cache::new();
            let mut app = selected_graphic(GraphicKind::Arc, size);
            let (opener, prefix) = match popup {
                "import" => {
                    app.inspector_tab = InspectorTab::Import;
                    app.imports.set_text("CCO");
                    ("import-insert-menu", "import-replace")
                }
                "export" => {
                    app.inspector_tab = InspectorTab::Export;
                    ("export-figure-format", "figure-format-")
                }
                _ => {
                    let snapshot = ui.snapshot(&app);
                    let opener = if snapshot
                        .nodes
                        .iter()
                        .any(|node| node.id == "arrange-compact")
                    {
                        "arrange-compact"
                    } else {
                        "arrange-menu-Order"
                    };
                    (opener, "menu-")
                }
            };
            let drawing = app.tab.doc.clone();
            let selected = app.tab.selected.clone();
            let revision = app.tab.revision;
            ui.focus(&app, opener);
            let message = ui.activate_focused(&app);
            let _ = app.update(message);
            let foreground = ui.snapshot(&app);
            assert!(!foreground.nodes.is_empty());
            assert!(
                foreground
                    .nodes
                    .iter()
                    .all(|node| node.id.starts_with(prefix))
            );
            assert!(matches!(
                ui.activate(&app, opener),
                operation::Outcome::None
            ));
            ui.tab(&app, false);
            let focused = ui.snapshot(&app);
            assert_eq!(focused.nodes.iter().filter(|node| node.focused).count(), 1);
            assert!(
                focused
                    .nodes
                    .iter()
                    .any(|node| node.focused && node.enabled)
            );
            let (status, messages) = ui.event(&app, key(Named::Escape, Modifiers::empty(), false));
            assert_eq!(status, iced::event::Status::Captured);
            assert_eq!(messages.len(), 1);
            for message in messages {
                let _ = app.update(message);
            }
            ui.assert_focused(&app, opener);
            assert!(app.context_menu.is_none());
            assert!(!app.imports.menu && !app.tab.inspector_ui.menu_open());

            if popup == "import" {
                ui.tab(&app, false);
                ui.assert_focused(&app, "import-choose-file");
            } else if popup == "export" {
                let message = ui.activate_focused(&app);
                let _ = app.update(message);
                ui.tab(&app, false);
                ui.assert_focused(&app, "figure-format-svg");
                let message = ui.activate_focused(&app);
                assert!(matches!(
                    message,
                    Message::InspectorAction(inspector::Action::Figure(
                        inspector::FigureFormat::Svg
                    ))
                ));
                let _ = app.update(message);
                assert!(!app.tab.inspector_ui.menu_open());
                let closed = ui.snapshot(&app);
                let format = closed.nodes.iter().find(|node| node.id == opener).unwrap();
                assert_eq!(format.name, "Figure format: SVG");
                assert_eq!(format.expanded, Some(false));
                ui.assert_focused(&app, opener);
            }
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.selected, selected);
            assert_eq!(app.tab.revision, revision);
        }
    }
}
