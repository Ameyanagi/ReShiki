use super::*;
use iced::advanced::{renderer::Headless, widget::operation};
use iced::keyboard::{
    Key, Modifiers,
    key::{Code, Named, Physical},
};
use iced::widget::{container, row, scrollable, text, text_input};

fn arrow(key: Named, modifiers: Modifiers) -> Event {
    let code = if key == Named::ArrowLeft {
        Code::ArrowLeft
    } else {
        Code::ArrowRight
    };
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(key),
        modified_key: Key::Named(key),
        physical_key: Physical::Code(code),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

fn event(
    scope: &mut dyn Widget<Message, Theme, Renderer>,
    tree: &mut Tree,
    renderer: &Renderer,
    node: &layout::Node,
    event: Event,
    cursor: mouse::Cursor,
) -> (iced::event::Status, Vec<Message>) {
    let mut messages = Vec::new();
    let mut shell = Shell::new(&mut messages);
    scope.update(
        tree,
        &event,
        Layout::new(node),
        cursor,
        renderer,
        &mut iced::advanced::clipboard::Null,
        &mut shell,
        &Rectangle::with_size(Size::new(400., 200.)),
    );
    (shell.event_status(), messages)
}

#[tokio::test]
#[ignore = "requires a headless renderer"]
async fn arrow_keys_only_step_the_focused_paused_path_and_never_escape_while_playing() {
    use crate::app::xtb::{Action as XtbAction, ts::Action};
    let renderer = <Renderer as Headless>::new(iced::Font::DEFAULT, iced::Pixels(16.), None)
        .await
        .expect("headless renderer");
    let mut scope = Scope {
        id: Id::new("test-irc-path"),
        paused: true,
        step: |step| Message::Xtb(XtbAction::Ts(Action::IrcStep(step))),
        content: container(text("Reaction path"))
            .width(200)
            .height(100)
            .into(),
    };
    let mut tree = Tree::new(&scope as &dyn Widget<Message, Theme, Renderer>);
    let node = scope.layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(200., 100.)),
    );
    let key = || arrow(Named::ArrowRight, Modifiers::empty());
    let (status, messages) = event(
        &mut scope,
        &mut tree,
        &renderer,
        &node,
        key(),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Ignored);
    assert!(messages.is_empty());

    scope.operate(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &mut operation::focusable::focus::<()>(Id::new("test-irc-path")),
    );
    let (status, messages) = event(
        &mut scope,
        &mut tree,
        &renderer,
        &node,
        key(),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(
        messages.as_slice(),
        [Message::Xtb(XtbAction::Ts(Action::IrcStep(1)))]
    ));

    scope.paused = false;
    let (status, messages) = event(
        &mut scope,
        &mut tree,
        &renderer,
        &node,
        key(),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(messages.is_empty());

    scope.paused = true;
    event(
        &mut scope,
        &mut tree,
        &renderer,
        &node,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(iced::Point::new(300., 50.)),
    );
    let (status, messages) = event(
        &mut scope,
        &mut tree,
        &renderer,
        &node,
        key(),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Ignored);
    assert!(messages.is_empty());

    event(
        &mut scope,
        &mut tree,
        &renderer,
        &node,
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(iced::Point::new(50., 50.)),
    );
    let (status, messages) = event(
        &mut scope,
        &mut tree,
        &renderer,
        &node,
        arrow(Named::ArrowLeft, Modifiers::empty()),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(matches!(
        messages.as_slice(),
        [Message::Xtb(XtbAction::Ts(Action::IrcStep(-1)))]
    ));
    let (status, messages) = event(
        &mut scope,
        &mut tree,
        &renderer,
        &node,
        arrow(Named::ArrowLeft, Modifiers::SHIFT),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Ignored);
    assert!(messages.is_empty());
}

#[tokio::test]
#[ignore = "requires a headless renderer"]
async fn a_captured_text_field_press_clears_the_scrolled_timeline() {
    use crate::app::xtb::{Action as XtbAction, ts::Action};

    let renderer = <Renderer as Headless>::new(iced::Font::DEFAULT, iced::Pixels(16.), None)
        .await
        .expect("headless renderer");
    let timeline = scope(
        "test-path",
        true,
        |step| Message::Xtb(XtbAction::Ts(Action::IrcStep(step))),
        container(text("Reaction path")).width(200).height(100),
    );
    // The timeline precedes the field in dispatch order.
    let content = row![
        scrollable(timeline).width(200).height(100),
        text_input("Name", "Molecule")
            .width(200)
            .on_input(|_| Message::Escape),
    ];
    let mut root =
        crate::app::file_shortcuts::wrap(content.into(), None, None, false, false, false, false);
    let mut tree = Tree::new(root.as_widget());
    let node = root.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(400., 200.)),
    );
    let mut send = |input, cursor| {
        event(
            root.as_widget_mut(),
            &mut tree,
            &renderer,
            &node,
            input,
            cursor,
        )
    };
    let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let key = arrow(Named::ArrowRight, Modifiers::empty());
    send(
        press.clone(),
        mouse::Cursor::Available(iced::Point::new(50., 15.)),
    );
    assert!(matches!(
        send(key.clone(), mouse::Cursor::Unavailable).1.as_slice(),
        [Message::Xtb(XtbAction::Ts(Action::IrcStep(1)))]
    ));

    let (status, _) = send(
        press.clone(),
        mouse::Cursor::Available(iced::Point::new(250., 15.)),
    );
    assert_eq!(status, iced::event::Status::Captured);
    let (status, messages) = send(key.clone(), mouse::Cursor::Unavailable);
    assert_eq!(status, iced::event::Status::Captured);
    assert!(messages.is_empty());

    send(press, mouse::Cursor::Available(iced::Point::new(50., 15.)));
    assert!(matches!(
        send(key, mouse::Cursor::Unavailable).1.as_slice(),
        [Message::Xtb(XtbAction::Ts(Action::IrcStep(1)))]
    ));
}
