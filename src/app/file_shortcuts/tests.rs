use super::*;

#[test]
fn unhandled_optimization_chord_is_swallowed_only_by_a_focused_field() {
    use keyboard::{
        Key, Modifiers,
        key::{Code, Physical},
    };
    let command = if cfg!(target_os = "macos") {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    };
    let event = Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character("d".into()),
        modified_key: Key::Character("D".into()),
        physical_key: Physical::Code(Code::KeyD),
        location: keyboard::Location::Standard,
        modifiers: command | Modifiers::SHIFT,
        text: None,
        repeat: false,
    });
    assert!(matches!(
        super::super::shortcuts::key_message(
            &Key::Character("d".into()),
            &Key::Character("D".into()),
            command | Modifiers::SHIFT
        ),
        Some(Message::Optimization(
            super::super::optimization::Action::Begin
        ))
    ));
    let mut fields =
        Fields::after(&event, false).expect("Check focus after an unhandled optimization chord");
    let mut messages = Vec::new();
    let mut shell = Shell::new(&mut messages);
    fields.finish(&mut shell);
    assert_eq!(shell.event_status(), iced::event::Status::Ignored);
    fields.focused = true;
    fields.finish(&mut shell);
    assert_eq!(shell.event_status(), iced::event::Status::Captured);
    assert!(!fields.leave, "Keep the field focused");
    assert!(Fields::after(&event, true).is_none());
    assert!(
        messages.is_empty(),
        "Focused optimization chords publish no drawing action"
    );
}

#[test]
fn modal_activation_rejects_control_alt_and_logo_on_every_platform() {
    use keyboard::{Key, Modifiers, key::Named};
    for modifiers in [
        Modifiers::empty(),
        Modifiers::SHIFT,
        Modifiers::CTRL,
        Modifiers::CTRL | Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::ALT | Modifiers::SHIFT,
        Modifiers::LOGO,
        Modifiers::LOGO | Modifiers::SHIFT,
    ] {
        let allowed = modifiers.is_empty() || modifiers == Modifiers::SHIFT;
        for named in [Named::Enter, Named::Space, Named::Tab] {
            let key = Key::Named(named);
            let event = Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key: keyboard::key::Physical::Code(match named {
                    Named::Enter => keyboard::key::Code::Enter,
                    Named::Space => keyboard::key::Code::Space,
                    _ => keyboard::key::Code::Tab,
                }),
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            });
            assert_eq!(activation_event(&event), allowed, "{event:?}");
        }
        let event = Event::Keyboard(keyboard::Event::KeyReleased {
            key: Key::Named(Named::Space),
            modified_key: Key::Named(Named::Space),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Space),
            location: keyboard::Location::Standard,
            modifiers,
        });
        assert_eq!(activation_event(&event), allowed, "{event:?}");
    }
}

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
