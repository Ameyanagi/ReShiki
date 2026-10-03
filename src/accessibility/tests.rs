use super::*;
use iced::advanced::{
    Layout, Shell, layout, mouse,
    renderer::{Headless, Style},
    widget::{
        Id, Operation, Tree,
        operation::{self, Focusable},
    },
};
use iced::keyboard::{
    self, Key, Modifiers,
    key::{Code, Named, Physical},
};
use iced::widget::{Space, column, scrollable, text, text_input};
use iced::{Element, Event, Rectangle, Size, Theme};

#[derive(Debug, Clone, PartialEq)]
enum Message {
    Apply,
    Close,
    Edited(String),
}

struct Ui {
    renderer: iced::Renderer,
    tree: Tree,
    viewport: Rectangle,
}

impl Ui {
    async fn new(size: Size) -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::default(),
                iced::Pixels(16.),
                None,
            )
            .await
            .expect("headless renderer"),
            tree: Tree::empty(),
            viewport: Rectangle::with_size(size),
        }
    }
    fn layout(&mut self, view: &mut Element<'_, Message>) -> layout::Node {
        self.tree.diff(view.as_widget());
        view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.viewport.size(), self.viewport.size()),
        )
    }
    fn event(
        &mut self,
        view: &mut Element<'_, Message>,
        event: Event,
        cursor: mouse::Cursor,
    ) -> (iced::event::Status, Vec<Message>) {
        let node = self.layout(view);
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
        (shell.event_status(), messages)
    }
    fn operate<T>(&mut self, view: &mut Element<'_, Message>, operation: &mut dyn Operation<T>) {
        let node = self.layout(view);
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut operation::black_box(operation),
        );
    }
    fn snapshot(&mut self, view: &mut Element<'_, Message>) -> Snapshot {
        let mut collect = Collect::new(self.viewport);
        self.operate(view, &mut collect);
        collect.snapshot().clone()
    }
    fn focused(&mut self, view: &mut Element<'_, Message>) -> Vec<Id> {
        #[derive(Default)]
        struct Find(Vec<Id>);
        impl Operation for Find {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
                if state.is_focused() {
                    self.0.extend(id.cloned());
                }
            }
        }
        let mut find = Find::default();
        self.operate(view, &mut find);
        find.0
    }
    fn pixels(&mut self, view: &mut Element<'_, Message>) -> Vec<u8> {
        use iced::advanced::Renderer as _;
        let node = self.layout(view);
        self.event(
            view,
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            mouse::Cursor::Unavailable,
        );
        self.renderer.reset(self.viewport);
        view.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            &Theme::Light,
            &Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &self.viewport,
        );
        Headless::screenshot(
            &mut self.renderer,
            Size::new(self.viewport.width as u32, self.viewport.height as u32),
            1.,
            iced::Color::WHITE,
        )
    }
}

fn key(named: Named, modifiers: Modifiers, repeat: bool, release: bool) -> Event {
    let key = Key::Named(named);
    let physical_key = Physical::Code(match named {
        Named::Tab => Code::Tab,
        Named::Space => Code::Space,
        _ => Code::Enter,
    });
    Event::Keyboard(if release {
        keyboard::Event::KeyReleased {
            key: key.clone(),
            modified_key: key,
            physical_key,
            location: keyboard::Location::Standard,
            modifiers,
        }
    } else {
        keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key,
            location: keyboard::Location::Standard,
            modifiers,
            text: None,
            repeat,
        }
    })
}

fn controls(enabled: bool, first_id: &'static str) -> Element<'static, Message> {
    focus_scope(
        column![
            button(first_id, "Apply transform", text("Apply"))
                .on_press_maybe(enabled.then_some(Message::Apply))
                .width(140)
                .height(36),
            button("disabled", "Unavailable action", text("Unavailable"))
                .width(140)
                .height(36),
            text_input("degrees", "15")
                .id("rotation")
                .on_input(Message::Edited)
                .width(140),
            button("close", "Close dialog", text("Close"))
                .on_press(Message::Close)
                .width(140)
                .height(36),
        ]
        .spacing(8),
    )
}

#[tokio::test]
#[ignore = "Opt-in real renderer keyboard and focus-ring check"]
async fn keyboard_and_native_actions_use_the_live_control_and_real_focus_state() {
    for size in [Size::new(1280., 820.), Size::new(1040., 680.)] {
        let mut ui = Ui::new(size).await;
        let mut view = controls(true, "apply");
        let before = ui.pixels(&mut view);
        let snapshot = ui.snapshot(&mut view);
        assert!(snapshot.duplicate_ids.is_empty());
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .map(|node| (node.id.as_str(), node.enabled))
                .collect::<Vec<_>>(),
            vec![("apply", true), ("disabled", false), ("close", true)]
        );
        let tab = |modifiers| key(Named::Tab, modifiers, false, false);
        assert_eq!(
            ui.event(
                &mut view,
                tab(Modifiers::empty()),
                mouse::Cursor::Unavailable
            )
            .0,
            iced::event::Status::Captured
        );
        assert_eq!(ui.focused(&mut view), vec![Id::from("apply")]);
        let focused = ui.pixels(&mut view);
        assert_ne!(before, focused, "keyboard focus must be painted");
        let bounds = snapshot.nodes[0].bounds;
        let changed: Vec<_> = before
            .chunks_exact(4)
            .zip(focused.chunks_exact(4))
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(index, _)| {
                iced::Point::new(
                    (index % size.width as usize) as f32,
                    (index / size.width as usize) as f32,
                )
            })
            .collect();
        assert!(changed.len() > 20);
        assert!(
            changed.iter().all(|point| bounds.contains(*point)),
            "focus ring must not alter other controls"
        );
        assert_eq!(
            ui.event(
                &mut view,
                key(Named::Enter, Modifiers::empty(), false, false),
                mouse::Cursor::Unavailable
            )
            .1,
            vec![Message::Apply]
        );
        assert!(
            ui.event(
                &mut view,
                key(Named::Enter, Modifiers::empty(), true, false),
                mouse::Cursor::Unavailable
            )
            .1
            .is_empty()
        );
        let cursor = mouse::Cursor::Available(bounds.center());
        let pressed = ui.event(
            &mut view,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            cursor,
        );
        assert!(pressed.1.is_empty());
        assert_eq!(
            ui.event(
                &mut view,
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                cursor,
            )
            .1,
            vec![Message::Apply],
            "mouse and keyboard must dispatch the same action",
        );
        for repeat in [false, true, true] {
            assert!(
                ui.event(
                    &mut view,
                    key(Named::Space, Modifiers::empty(), repeat, false),
                    mouse::Cursor::Unavailable
                )
                .1
                .is_empty()
            );
        }
        assert_eq!(
            ui.event(
                &mut view,
                key(Named::Space, Modifiers::empty(), false, true),
                mouse::Cursor::Unavailable
            )
            .1,
            vec![Message::Apply]
        );
        assert!(
            ui.event(
                &mut view,
                key(Named::Space, Modifiers::empty(), false, true),
                mouse::Cursor::Unavailable
            )
            .1
            .is_empty()
        );
        ui.event(
            &mut view,
            tab(Modifiers::empty()),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(ui.focused(&mut view), vec![Id::from("rotation")]);
        ui.event(
            &mut view,
            tab(Modifiers::empty()),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(ui.focused(&mut view), vec![Id::from("close")]);
        ui.event(
            &mut view,
            tab(Modifiers::empty()),
            mouse::Cursor::Unavailable,
        );
        assert_eq!(ui.focused(&mut view), vec![Id::from("apply")]);
        ui.event(&mut view, tab(Modifiers::SHIFT), mouse::Cursor::Unavailable);
        assert_eq!(ui.focused(&mut view), vec![Id::from("close")]);
        assert_eq!(
            ui.event(&mut view, tab(Modifiers::CTRL), mouse::Cursor::Unavailable)
                .0,
            iced::event::Status::Ignored
        );
        assert_eq!(ui.focused(&mut view), vec![Id::from("close")]);
        let mut activate = Activate::<Message>::new("apply");
        ui.operate(&mut view, &mut activate);
        assert_eq!(activate.message(), Some(&Message::Apply));
        // Rebuild in the same position; stale requests must use new enabled state.
        view = controls(false, "apply");
        let mut activate = Activate::<Message>::new("apply");
        ui.operate(&mut view, &mut activate);
        assert_eq!(activate.message(), None);
        view = controls(true, "replacement");
        let mut activate = Activate::<Message>::new("apply");
        ui.operate(&mut view, &mut activate);
        assert_eq!(activate.message(), None);
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer scrolling and semantic-identity check"]
async fn focus_reveals_scrolled_control_and_ambiguous_ids_cannot_activate() {
    let mut ui = Ui::new(Size::new(320., 220.)).await;
    let mut view = focus_scope(
        scrollable(column![
            button("top", "Top", text("Top"))
                .on_press(Message::Apply)
                .height(36),
            Space::new().height(800),
            button("bottom", "Bottom", text("Bottom"))
                .on_press(Message::Close)
                .height(36),
        ])
        .height(150),
    );
    let initial = ui.snapshot(&mut view);
    assert!(initial.nodes[1].visible_bounds.is_none());
    ui.event(
        &mut view,
        key(Named::Tab, Modifiers::SHIFT, false, false),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(ui.focused(&mut view), vec![Id::from("bottom")]);
    let focused = ui.snapshot(&mut view);
    let bottom = &focused.nodes[1];
    assert!(bottom.focused);
    assert!(
        bottom
            .visible_bounds
            .unwrap()
            .contains(bottom.bounds.center())
    );
    assert!(focused.nodes[0].visible_bounds.is_none());
    assert_eq!(
        ui.event(
            &mut view,
            key(Named::Enter, Modifiers::empty(), false, false),
            mouse::Cursor::Unavailable
        )
        .1,
        vec![Message::Close]
    );
    view = focus_scope(column![
        button("duplicate", "First", text("First")).on_press(Message::Apply),
        button("duplicate", "Second", text("Second")).on_press(Message::Close),
    ]);
    assert_eq!(ui.snapshot(&mut view).duplicate_ids, vec!["duplicate"]);
    let mut activate = Activate::<Message>::new("duplicate");
    ui.operate(&mut view, &mut activate);
    assert_eq!(activate.message(), None);
}

#[tokio::test]
#[ignore = "Opt-in real runtime overlay and in-flight gesture regression"]
async fn tooltip_does_not_trap_tab_and_replaced_controls_do_not_inherit_mouse_down() {
    use iced::widget::tooltip;
    use iced_runtime::{UserInterface, user_interface::Cache};
    let size = Size::new(320., 220.);
    let mut ui = Ui::new(size).await;
    let mut view: Element<'_, Message> = button("before", "Apply", text("Apply"))
        .on_press(Message::Apply)
        .width(140)
        .height(36)
        .into();
    let cursor = mouse::Cursor::Available(iced::Point::new(10., 10.));
    ui.event(
        &mut view,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        cursor,
    );
    view = button("after", "Close", text("Close"))
        .on_press(Message::Close)
        .width(140)
        .height(36)
        .into();
    assert!(
        ui.event(
            &mut view,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            cursor
        )
        .1
        .is_empty(),
        "replacement never received mouse-down"
    );
    ui.event(
        &mut view,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        cursor,
    );
    view = button("after", "Close", text("Close"))
        .width(140)
        .height(36)
        .into();
    ui.layout(&mut view);
    view = button("after", "Close", text("Close"))
        .on_press(Message::Close)
        .width(140)
        .height(36)
        .into();
    assert!(
        ui.event(
            &mut view,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            cursor
        )
        .1
        .is_empty(),
        "disabled transition cancels pending mouse-down"
    );
    drop(view);

    let view = focus_scope(
        column![
            tooltip(
                button("apply", "Apply", text("Apply"))
                    .on_press(Message::Apply)
                    .width(140)
                    .height(36),
                text("Visible tooltip"),
                tooltip::Position::Right,
            )
            .delay(std::time::Duration::ZERO),
            button("close", "Close", text("Close"))
                .on_press(Message::Close)
                .width(140)
                .height(36),
        ]
        .spacing(8),
    );
    let mut runtime = UserInterface::build(view, size, Cache::new(), &mut ui.renderer);
    let mut messages = Vec::new();
    runtime.update(
        &[key(Named::Tab, Modifiers::empty(), false, false)],
        mouse::Cursor::Unavailable,
        &mut ui.renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    // The real tooltip opens synchronously on hover (zero delay). The runtime
    // then calls its overlay before dispatching the next key to the base.
    runtime.update(
        &[Event::Mouse(mouse::Event::CursorMoved {
            position: iced::Point::new(10., 10.),
        })],
        cursor,
        &mut ui.renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    runtime.draw(&mut ui.renderer, &Theme::Light, &Style::default(), cursor);
    let (_, status) = runtime.update(
        &[key(Named::Tab, Modifiers::empty(), false, false)],
        cursor,
        &mut ui.renderer,
        &mut iced::advanced::clipboard::Null,
        &mut messages,
    );
    assert_eq!(status, vec![iced::event::Status::Captured]);
    let mut collect = Collect::new(Rectangle::with_size(size));
    runtime.operate(&ui.renderer, &mut operation::black_box(&mut collect));
    assert_eq!(
        collect
            .snapshot()
            .nodes
            .iter()
            .filter(|node| node.focused)
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec!["close"]
    );
}

#[test]
#[ignore = "requires the real Iced headless renderer"]
fn native_field_edits_and_modal_focus_use_live_widgets() {
    iced::futures::executor::block_on(async {
        let mut ui = Ui::new(Size::new(320., 220.)).await;
        let field = |enabled: bool| {
            super::text_input("width", "Width (pt)", "", "20")
                .on_input_maybe(enabled.then_some(Message::Edited))
        };
        let mut view: Element<'_, Message> = focus_scope(column![
            field(true),
            button("apply", "Apply", text("Apply")).on_press(Message::Apply)
        ]);
        let mut focus = FocusControl::new("width");
        ui.operate(&mut view, &mut focus);
        if let operation::Outcome::Chain(mut focus) = focus.finish() {
            ui.operate(&mut view, focus.as_mut());
        }
        let snapshot = ui.snapshot(&mut view);
        assert_eq!(snapshot.nodes[0].value.as_deref(), Some("20"));
        assert!(snapshot.nodes[0].focused);
        let mut edit = SetValue::<Message>::new("width", "42".into());
        ui.operate(&mut view, &mut edit);
        assert!(
            matches!(edit.finish(), operation::Outcome::Some(Message::Edited(value)) if value == "42")
        );
        let mut disabled: Element<'_, Message> = focus_scope(column![field(false)]);
        let mut edit = SetValue::<Message>::new("width", "99".into());
        ui.operate(&mut disabled, &mut edit);
        assert!(matches!(edit.finish(), operation::Outcome::None));
        assert!(!ui.snapshot(&mut disabled).nodes[0].enabled);
        assert!(ui.focused(&mut disabled).is_empty());
        let mut modal: Element<'_, Message> = focus_scope(column![
            inert(field(true)),
            button("close", "Close", text("Close")).on_press(Message::Close)
        ]);
        let snapshot = ui.snapshot(&mut modal);
        assert_eq!(snapshot.nodes.len(), 1);
        assert_eq!(snapshot.nodes[0].id, "close");
        let mut edit = SetValue::<Message>::new("width", "99".into());
        ui.operate(&mut modal, &mut edit);
        assert!(matches!(edit.finish(), operation::Outcome::None));
    });
}
