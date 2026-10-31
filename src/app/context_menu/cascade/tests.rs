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

#[test]
fn pinned_rows_stay_hittable_while_scrolling_rows_are_translated_and_clipped() {
    let top = Rectangle::new(Point::new(15., 25.), Size::new(190., 30.));
    let middle = Rectangle::new(Point::new(15., 100.), Size::new(190., 30.));
    let lower = Rectangle::new(Point::new(15., 190.), Size::new(190., 30.));
    let copy = Rectangle::new(Point::new(15., 130.), Size::new(190., 28.));
    let delete = Rectangle::new(Point::new(15., 160.), Size::new(190., 28.));
    let mut rows = Rows {
        pinned: Some(3),
        panel: Rectangle::new(Point::new(10., 20.), Size::new(200., 180.)),
        targets: vec![],
        found: vec![(0, top), (1, middle), (2, lower), (3, copy), (4, delete)],
        viewport: Some(Rectangle::new(Point::new(15., 25.), Size::new(190., 100.))),
        translation: Vector::ZERO,
    };

    assert_eq!(rows.bounds(0), Some(top));
    assert_eq!(rows.at(top.center()), Some(0));
    assert_eq!(rows.at(Point::new(20., 110.)), Some(1));
    assert_eq!(rows.at(Point::new(20., 128.)), None);
    for translation in [Vector::ZERO, Vector::new(0., 60.)] {
        rows.translation = translation;
        assert_eq!(rows.bounds(3), Some(copy));
        assert_eq!(rows.bounds(4), Some(delete));
        assert_eq!(rows.at(copy.center()), Some(3));
        assert_eq!(rows.at(delete.center()), Some(4));
    }
    assert_eq!(rows.bounds(0), Some(top - Vector::new(0., 60.)));
    assert_eq!(rows.bounds(1), Some(middle - Vector::new(0., 60.)));
    assert_eq!(rows.at(Point::new(20., -20.)), None);
    assert_eq!(rows.at(Point::new(20., 50.)), Some(1));
    // The translated lower body row overlaps Copy, but its scroll viewport
    // clips it away before hit testing the footer in panel coordinates.
    assert_eq!(rows.bounds(2), Some(lower - Vector::new(0., 60.)));
    assert_eq!(rows.at(copy.center()), Some(3));
}
