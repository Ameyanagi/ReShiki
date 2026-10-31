use super::*;
use reshiki::arrows::{ArrowStyle, Preset};
use reshiki::document::Arrow;
use reshiki::graphics::{Graphic, PathCommand};

fn path(id: u64, curved: bool) -> Graphic {
    let mut graphic = Graphic::pen_curve(
        id,
        Point::default(),
        Point::new(100., 0.),
        Default::default(),
    );
    graphic.path = if curved {
        vec![
            PathCommand::Move(Point::default()),
            PathCommand::Cubic(
                Point::new(25., -20.),
                Point::new(75., -22.),
                Point::new(100., 0.),
            ),
        ]
    } else {
        vec![
            PathCommand::Move(Point::default()),
            PathCommand::Line(Point::new(50., 0.05)),
            PathCommand::Line(Point::new(100., 0.)),
        ]
    };
    graphic.node_types.clear();
    graphic
}

fn selection(curved: bool) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let mut arrow = Arrow::new(
        3,
        Point::new(200., 0.),
        Point::new(300., 0.),
        Preset::Curved,
        ArrowStyle::default(),
    );
    arrow.cubic = Some([Point::new(225., -20.), Point::new(275., -22.)]);
    arrow.control = None;
    app.tab.doc = Document {
        graphics: vec![path(1, curved), path(2, curved), path(4, curved)],
        arrows: vec![arrow],
        ..Document::default()
    };
    app.tab.selected = vec![1, 2, 3];
    app
}

#[test]
fn cleanup_of_two_paths_and_an_arrow_is_one_undo_step() {
    for (command, status, curved) in [
        (Cleanup::Smooth, "Smoothed 3 objects", false),
        (
            Cleanup::Symmetrise { upright: false },
            "Symmetrised 3 objects",
            true,
        ),
    ] {
        let mut app = selection(curved);
        let before = app.tab.doc.clone();
        let _ = app.update(Message::Graphics(Action::Cleanup(command)));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.status, status);
        for i in 0..2 {
            assert_ne!(app.tab.doc.graphics[i], before.graphics[i]);
        }
        assert_ne!(app.tab.doc.arrows, before.arrows);
        assert_eq!(app.tab.doc.graphics[2], before.graphics[2]);
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
        assert!(!app.tab.history.can_redo());
        app.tab.history = Default::default();
        let _ = app.update(Message::Graphics(Action::Cleanup(command)));
        assert_eq!(app.tab.doc, after);
        assert!(!app.tab.history.can_undo());
        assert!(!app.error, "{}", app.status);
        assert_eq!(
            app.status,
            match command {
                Cleanup::Smooth => "No smoothing needed",
                Cleanup::Symmetrise { .. } => "Already symmetric",
            }
        );
    }
}

#[test]
fn cleanup_reports_empty_selection_and_refusals_without_history() {
    let mut app = selection(true);
    app.tab.selected.clear();
    let _ = app.update(Message::Graphics(Action::Cleanup(Cleanup::Smooth)));
    assert!(app.error);
    assert_eq!(
        app.status,
        "Select a pen path, Bézier curve or curved arrow"
    );
    assert!(!app.tab.history.can_undo());
    app.tab.selected = vec![3];
    app.tab.doc.arrows[0].cubic = Some([Point::new(225., -20.), Point::new(275., 22.)]);
    let before = app.tab.doc.clone();
    let _ = app.update(Message::Graphics(Action::Cleanup(Cleanup::Symmetrise {
        upright: false,
    })));
    assert!(app.error);
    assert!(
        app.status.starts_with("Not symmetric enough:"),
        "{}",
        app.status
    );
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    app.tab.selected.push(1);
    let _ = app.update(Message::Graphics(Action::Cleanup(Cleanup::Symmetrise {
        upright: false,
    })));
    assert!(app.error);
    assert!(
        app.status
            .starts_with("Symmetrised 1 object; Not symmetric enough:")
    );
    assert_eq!(app.tab.doc.arrows, before.arrows);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn shift_symmetrise_uses_upright_axis_and_modifier_updates_keep_menus_open() {
    let mut app = selection(true);
    app.tab.selected = vec![1];
    let before = app.tab.doc.clone();
    let mut expected = before.graphics[0].clone();
    expected.symmetrise_path(true).unwrap();
    app.context_menu = Some(crate::app::context_menu::State::new(
        iced::Point::ORIGIN,
        crate::app::context_menu::Page::Main,
    ));
    let _ = app.update(Message::ModifiersChanged(iced::keyboard::Modifiers::SHIFT));
    assert!(app.shift_held);
    assert!(app.context_menu.is_some());
    let _ = app.update(Message::ContextMenu(crate::app::context_menu::Action::Run(
        Box::new(Message::Graphics(Action::Cleanup(Cleanup::Symmetrise {
            upright: false,
        }))),
    )));
    assert_eq!(app.tab.doc.graphics[0], expected);
    assert!(!app.error, "{}", app.status);
    let _ = app.update(Message::ModifiersChanged(iced::keyboard::Modifiers::empty()));
    assert!(!app.shift_held);
}

#[test]
fn shift_leaves_arrow_symmetrisation_unchanged() {
    for modifiers in [
        iced::keyboard::Modifiers::empty(),
        iced::keyboard::Modifiers::SHIFT,
    ] {
        let mut app = selection(true);
        app.tab.selected = vec![3];
        let mut expected = app.tab.doc.arrows[0].clone();
        assert!(expected.symmetrise().unwrap());
        let _ = app.update(Message::ModifiersChanged(modifiers));
        let _ = app.update(Message::Graphics(Action::Cleanup(Cleanup::Symmetrise {
            upright: false,
        })));
        assert_eq!(app.tab.doc.arrows[0], expected);
        assert_eq!(app.status, "Symmetrised 1 object");
        assert!(!app.error);
    }
}
