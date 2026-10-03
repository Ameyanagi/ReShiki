use super::*;
use crate::canvas::Edit;

const RINGS: [(&str, &str, u8); 6] = [
    ("3", "#", 3),
    ("4", "$", 4),
    ("5", "%", 5),
    ("6", "^", 6),
    ("7", "&", 7),
    ("8", "*", 8),
];

fn character(text: &str) -> Key {
    Key::Character(text.into())
}

#[test]
fn shifted_ring_digits_use_the_unmodified_key_and_preserve_other_chords() {
    for (digit, punctuation, size) in RINGS {
        // US punctuation, other shifted layouts, and digits reported unchanged.
        for modified in [punctuation, "&", "'", "(", digit] {
            let message = key_message(&character(digit), &character(modified), Modifiers::SHIFT);
            assert!(matches!(
                message,
                Some(Message::Shortcut(Action::SelectRing(actual))) if actual == size
            ));
        }
        let message = Message::Shortcut(Action::SelectRing(size));
        assert_eq!(
            label(&message),
            Some(keys(Modifiers::SHIFT, digit)),
            "The displayed chord selects its advertised size"
        );
        assert_eq!(
            super::super::palettes::ring_hint(
                &format!("{size}-membered"),
                &super::super::palettes::Action::Ring(size, false),
            ),
            format!("{size}-membered · {}", keys(Modifiers::SHIFT, digit))
        );
        assert!(matches!(
            key_message(&character(digit), &character(digit), Modifiers::empty()),
            Some(Message::ContextKey(key)) if key == digit
        ));
        for extra in [Modifiers::CTRL, Modifiers::ALT, Modifiers::LOGO] {
            assert!(!matches!(
                key_message(
                    &character(digit),
                    &character(punctuation),
                    Modifiers::SHIFT | extra,
                ),
                Some(Message::Shortcut(Action::SelectRing(_)))
            ));
        }
        assert!(!matches!(
            key_message(
                &character(punctuation),
                &character(punctuation),
                Modifiers::empty(),
            ),
            Some(Message::Shortcut(Action::SelectRing(_)))
        ));
    }
    for digit in ["0", "1", "2", "9"] {
        assert!(!matches!(
            key_message(&character(digit), &character(digit), Modifiers::SHIFT),
            Some(Message::Shortcut(Action::SelectRing(_)))
        ));
    }
}

#[test]
fn ring_tool_shortcuts_preserve_the_drawing_selection_and_redo_at_every_target() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![b];
    let _ = app.update(Message::ContextKey("1".into()));
    let grown = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    let before = app.tab.doc.clone();
    assert_eq!(before.atoms.len(), 2);
    assert_eq!(grown.atoms.len(), 3);
    assert!(app.tab.history.can_redo());

    for target in [
        "blank",
        "hover atom",
        "hover bond",
        "selected atom",
        "selected bond",
    ] {
        let selected = match target {
            "selected atom" => vec![a],
            "selected bond" => vec![a, b],
            _ => vec![],
        };
        let point = match target {
            "hover atom" => Point::default(),
            "hover bond" => Point::new(21., 0.),
            _ => Point::new(400., 200.),
        };
        app.tab.selected = selected.clone();
        let _ = app.update(Message::Canvas(Edit::Hover(Some(point))));
        for (digit, punctuation, size) in RINGS {
            app.tool = Tool::Bond(3);
            app.toolbar.ring = Tool::RingPreset(reshiki::rings::Preset::ChairDown);
            app.ring_size = 8;
            app.aromatic_ring = true;
            app.palette = Some(super::super::palettes::Family::Rings);
            let message =
                key_message(&character(digit), &character(punctuation), Modifiers::SHIFT).unwrap();
            let _ = app.update(message);
            assert_eq!(app.tool, Tool::Ring, "{target}, {digit}");
            assert_eq!(app.toolbar.ring, Tool::Ring);
            assert_eq!(app.ring_size, size);
            assert!(!app.aromatic_ring);
            assert!(app.palette.is_none());
            assert_eq!(app.tab.selected, selected);
            assert_eq!(app.tab.doc, before);
            assert!(!app.tab.history.can_undo());
            assert!(app.tab.history.can_redo());
        }
    }
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, grown);
}

#[test]
fn atom_label_drafts_capture_shifted_ring_selectors() {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let atom = app.tab.doc.add_atom("N", Point::default());
    app.tool = Tool::Atom;
    app.ring_size = 7;
    app.aromatic_ring = true;
    let before = app.tab.doc.clone();
    let _ = app.update(Message::AtomText(super::super::atom_text::Action::Begin(
        Some(atom),
    )));
    for (digit, punctuation, _) in RINGS {
        let message =
            key_message(&character(digit), &character(punctuation), Modifiers::SHIFT).unwrap();
        let _ = app.update(message);
        assert!(app.tab.atom_text.is_some());
        assert_eq!(app.tool, Tool::Atom);
        assert_eq!(app.ring_size, 7);
        assert!(app.aromatic_ring);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}
