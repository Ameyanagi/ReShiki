use super::*;
use iced::keyboard::{Key, Modifiers};

fn pressed(character: &str, modifiers: Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character(character.into()),
        modified_key: Key::Character(character.into()),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

#[test]
fn application_shortcuts_use_the_platform_command_modifier() {
    let command = Modifiers::COMMAND;
    assert!(matches!(
        application_shortcut(&pressed("c", command)),
        Some(Message::Copy(false))
    ));
    assert!(matches!(
        application_shortcut(&pressed("x", command)),
        Some(Message::Copy(true))
    ));
    assert!(matches!(
        application_shortcut(&pressed("z", command)),
        Some(Message::Undo)
    ));
    assert!(matches!(
        application_shortcut(&pressed("Z", command | Modifiers::SHIFT)),
        Some(Message::Redo)
    ));
}

#[test]
fn typing_and_unbound_shortcuts_do_not_dispatch_drawing_commands() {
    for modifiers in [Modifiers::empty(), Modifiers::SHIFT, Modifiers::ALT] {
        assert!(application_shortcut(&pressed("n", modifiers)).is_none());
    }
    assert!(application_shortcut(&pressed("b", Modifiers::COMMAND)).is_none());
    assert!(
        application_shortcut(&Event::InputMethod(
            iced::advanced::input_method::Event::Commit("窒素".into())
        ))
        .is_none()
    );
}
