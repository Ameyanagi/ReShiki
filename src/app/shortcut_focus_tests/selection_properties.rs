//! The new Properties contexts use the real app's keyboard and semantic paths.
use super::*;
use crate::app::inspector::{Action, Section};
use crate::canvas::Tool;
use reshiki::document::Point;

fn drawing() -> (App, Vec<u64>) {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let atoms = vec![
        app.tab.doc.add_atom("C", Point::default()),
        app.tab.doc.add_atom("C", Point::new(42., 0.)),
        app.tab.doc.add_atom("O", Point::new(84., 0.)),
    ];
    app.tab.doc.add_bond(atoms[0], atoms[1], 1, "plain");
    app.tab.doc.add_bond(atoms[1], atoms[2], 1, "plain");
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    (app, atoms)
}

#[tokio::test]
#[ignore = "Opt-in actual keyboard Escape and Close analysis focus regression"]
async fn requested_properties_close_from_focused_controls_without_editing() {
    let mut ui = Ui::new().await;
    for fragment in [false, true] {
        let (mut app, atoms) = drawing();
        if fragment {
            app.set_selection(atoms[..2].to_vec());
        }
        let original = app.tab.doc.clone();
        let selected = app.tab.selected.clone();
        for button in [false, true] {
            let task = app.show_molecular_properties();
            ui.run_task(&mut app, task).await;
            let target = if button || !fragment {
                "properties-close-analysis"
            } else {
                let task = app.update(Message::InspectorAction(Action::Section(
                    Section::Transform,
                    true,
                )));
                ui.run_task(&mut app, task).await;
                "transform-rotation"
            };
            ui.focus_control(&mut app, target).await;
            assert!(ui.focused(&app).iter().any(|id| id == target));
            assert_eq!(app.tool, Tool::Select);
            let (key, code) = if button {
                (Named::Enter, Code::Enter)
            } else {
                (Named::Escape, Code::Escape)
            };
            ui.key(&mut app, Key::Named(key), code, Modifiers::empty(), None)
                .await;
            assert!(!app.requested_molecular_properties());
            ui.assert_unfocused(&app);
            assert_eq!(app.tab.doc, original);
            assert_eq!(app.tab.selected, selected);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in real Properties defaults keyboard and native action regression"]
async fn property_defaults_support_keyboard_and_native_values_without_drawing_edits() {
    use iced::advanced::widget::operation;
    use reshiki::accessibility::{Activate, Role, SetValue};
    let mut ui = Ui::new().await;
    let (mut app, _) = drawing();
    let original = app.tab.doc.clone();
    let _ = app.update(Message::Tool(Tool::Ring));
    let before = app.ring_size;
    ui.focus_control(&mut app, "properties-ring-size").await;
    assert!(
        ui.focused(&app)
            .iter()
            .any(|id| id == "properties-ring-size")
    );
    ui.key(
        &mut app,
        Key::Named(Named::ArrowDown),
        Code::ArrowDown,
        Modifiers::empty(),
        None,
    )
    .await;
    assert_eq!(app.ring_size, (before + 1).min(8));
    assert_eq!(app.tab.doc, original);

    let _ = app.update(Message::Tool(Tool::Bond(1)));
    let snapshot = ui.semantics(&app);
    assert!(snapshot.duplicate_ids.is_empty());
    for scope in ["properties", "context"] {
        let id = format!("{scope}-bond-length");
        let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
        assert_eq!(node.role, Role::TextInput);
        assert_eq!(node.name, "Bond length (pt)");
    }
    let mut value = SetValue::<Message>::new("properties-bond-length", "20".into());
    ui.inspect(&app, &mut operation::black_box(&mut value));
    let operation::Outcome::Some(message) = value.finish() else {
        panic!("The native value action reaches the visible defaults field");
    };
    let _ = app.update(message);
    assert_eq!(app.tab.drawing_length_input, "20");
    let before = app.tab.bond_drawing.fixed_angles;
    let mut activate = Activate::<Message>::new("properties-fixed-angles");
    ui.inspect(&app, &mut operation::black_box(&mut activate));
    let operation::Outcome::Some(message) = activate.finish() else {
        panic!("The native toggle action reaches the visible defaults control");
    };
    let _ = app.update(message);
    assert_eq!(app.tab.bond_drawing.fixed_angles, !before);
    assert!(app.tab.selected.is_empty());
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());
}
