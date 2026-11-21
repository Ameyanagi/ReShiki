use super::*;
use crate::app::{inspector, naming};
use reshiki::document::Point;

fn drawing() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let carbon = app.tab.doc.add_atom("C", Point::default());
    app.tab.selected = vec![carbon];
    app
}

fn open(app: &mut App, page: InspectorTab) {
    // These UI checks exercise page routing and fields, not a connection task.
    let _ = app.update(Message::Inspector(page));
}

#[tokio::test]
#[ignore = "Opt-in real inspector page navigation and semantic controls"]
async fn inspector_pages_keep_tabs_and_named_back_close_controls() {
    let mut ui = Ui::new().await;
    for (page, parent) in [
        (InspectorTab::Names, InspectorTab::Properties),
        (InspectorTab::Names, InspectorTab::Templates),
        (InspectorTab::Names, InspectorTab::Import),
        (InspectorTab::Assistant, InspectorTab::Export),
        (InspectorTab::Assistant, InspectorTab::Templates),
        (InspectorTab::Assistant, InspectorTab::Import),
    ] {
        for control in ["inspector-page-back", "inspector-page-close"] {
            let mut app = drawing();
            open(&mut app, parent);
            open(&mut app, page);
            let snapshot = ui.semantics(&app);
            assert!(snapshot.duplicate_ids.is_empty());
            assert!(
                !snapshot
                    .nodes
                    .iter()
                    .any(|node| node.id == "inspector-tab-Import")
            );
            let selected = if parent == InspectorTab::Import {
                InspectorTab::Templates
            } else {
                parent
            };
            for (tab, label) in [
                ("Properties", "Properties"),
                ("Predict", "Predict"),
                ("Templates", "Add"),
                ("Export", "Export"),
            ] {
                let node = snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == format!("inspector-tab-{tab}"))
                    .expect("every parent tab remains available");
                assert_eq!(
                    node.checked,
                    Some(node.id == format!("inspector-tab-{selected:?}"))
                );
                assert_eq!(node.name, format!("{label} tab"));
                assert!(node.visible_bounds.is_some());
            }
            for id in ["inspector-page-back", "inspector-page-close"] {
                let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                assert_eq!(node.role, reshiki::accessibility::Role::Button);
                assert!(node.enabled && node.visible_bounds.is_some());
                assert!(node.name.starts_with(if id.ends_with("back") {
                    "Back to "
                } else {
                    "Close "
                }));
            }
            if parent == InspectorTab::Templates {
                assert!(
                    snapshot
                        .nodes
                        .iter()
                        .any(|node| node.id == "inspector-page-back" && node.name == "Back to Add")
                );
            }
            ui.focus_control(&mut app, control).await;
            assert_eq!(ui.focused(&app), [control]);
            ui.key(
                &mut app,
                Key::Named(Named::Enter),
                Code::Enter,
                Modifiers::empty(),
                None,
            )
            .await;
            assert_eq!(app.inspector_tab, parent);
            assert!(app.inspector_open);
            ui.assert_unfocused(&app);
            assert!(!app.tab.history.can_undo());
        }
        // The persistent row also supports leaving a page directly by keyboard.
        let mut app = drawing();
        open(&mut app, parent);
        open(&mut app, page);
        ui.focus_control(&mut app, "inspector-tab-Predict").await;
        ui.key(
            &mut app,
            Key::Named(Named::Enter),
            Code::Enter,
            Modifiers::empty(),
            None,
        )
        .await;
        assert_eq!(app.inspector_tab, InspectorTab::Predict);
    }
}

#[tokio::test]
#[ignore = "Opt-in field-owned Escape before inspector page dismissal"]
async fn inspector_page_escape_unfocuses_the_field_before_returning() {
    let mut ui = Ui::new().await;
    for (page, field) in [
        (InspectorTab::Names, "naming.name"),
        (InspectorTab::Assistant, "assistant-input"),
    ] {
        let mut app = drawing();
        // Name → structure first keeps its input visible in this focus test.
        app.tab.selected.clear();
        open(&mut app, InspectorTab::Export);
        open(&mut app, page);
        let bounds = ui.input(&app, field).0;
        assert!(ui.viewport.contains(bounds.center()));
        ui.click(&mut app, bounds.center());
        assert!(ui.input(&app, field).1);
        ui.key(
            &mut app,
            Key::Character("x".into()),
            Code::KeyX,
            Modifiers::empty(),
            Some("x"),
        )
        .await;
        ui.key(
            &mut app,
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        )
        .await;
        assert_eq!(app.inspector_tab, page, "the field owns the first Escape");
        assert!(!ui.input(&app, field).1);
        ui.key(
            &mut app,
            Key::Named(Named::Escape),
            Code::Escape,
            Modifiers::empty(),
            None,
        )
        .await;
        assert_eq!(app.inspector_tab, InspectorTab::Export);
        assert!(app.inspector_open);
        ui.assert_unfocused(&app);
        if page == InspectorTab::Names {
            assert_eq!(app.tab.naming.name, "x");
        } else {
            assert_eq!(app.assistant.input_text(), "x");
        }
        assert!(!app.tab.history.can_undo());
    }
}

#[tokio::test]
#[ignore = "Opt-in Chemical names section order from its actual entry point"]
async fn chemical_names_structure_entry_places_structure_to_name_first() {
    let mut ui = Ui::new().await;
    let mut app = drawing();
    let atoms = app.tab.selected.clone();
    app.tab.selected.clear();
    open(&mut app, InspectorTab::Names);
    let name = ui.text(&app, "NAME → STRUCTURE");
    let structure = ui.text(&app, "STRUCTURE → NAME");
    assert!(name.y < structure.y);
    let _ = app.update(Message::Naming(naming::Action::OpenPanel(atoms)));
    let name = ui.text(&app, "NAME → STRUCTURE");
    let structure = ui.text(&app, "STRUCTURE → NAME");
    assert!(structure.y < name.y);
    let snapshot = ui.semantics(&app);
    let generate = snapshot
        .nodes
        .iter()
        .find(|node| node.id == "naming.generate")
        .unwrap();
    assert!(generate.visible_bounds.is_some());
    open(&mut app, InspectorTab::Assistant);
    let task = app.update(Message::InspectorAction(inspector::Action::BackPage));
    ui.run_task(&mut app, task).await;
    assert_eq!(app.inspector_tab, InspectorTab::Names);
    assert!(ui.text(&app, "STRUCTURE → NAME").y < ui.text(&app, "NAME → STRUCTURE").y);
}
