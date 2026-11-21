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
        (InspectorTab::Names, InspectorTab::Assistant),
        (InspectorTab::Assistant, InspectorTab::Export),
        (InspectorTab::Assistant, InspectorTab::Templates),
        (InspectorTab::Assistant, InspectorTab::Import),
    ] {
        let controls: &[&str] = if page == InspectorTab::Names {
            &["inspector-page-back"]
        } else {
            &["inspector-page-back", "inspector-page-close"]
        };
        for &control in controls {
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
            let selected = match parent {
                InspectorTab::Import => InspectorTab::Templates,
                InspectorTab::Assistant => InspectorTab::Properties,
                _ => parent,
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
            for &id in controls {
                let node = snapshot.nodes.iter().find(|node| node.id == id).unwrap();
                assert_eq!(node.role, reshiki::accessibility::Role::Button);
                assert!(node.enabled && node.visible_bounds.is_some());
                assert!(node.name.starts_with(if id.ends_with("back") {
                    "Back to "
                } else {
                    "Close "
                }));
            }
            if page == InspectorTab::Names {
                assert!(
                    !snapshot
                        .nodes
                        .iter()
                        .any(|node| node.id == "inspector-page-close")
                );
                let import = snapshot
                    .nodes
                    .iter()
                    .find(|node| node.id == "naming.import")
                    .unwrap();
                assert_eq!(import.name, "Open SMILES import");
                assert!(import.enabled && import.visible_bounds.is_some());
            } else {
                let title = ui.text(&app, "Assistant");
                let bounds = |id| {
                    snapshot
                        .nodes
                        .iter()
                        .find(|node| node.id == id)
                        .unwrap()
                        .bounds
                };
                let back = bounds("inspector-page-back");
                let new = bounds("assistant-new-conversation");
                let close = bounds("inspector-page-close");
                assert!(back.x < title.x && title.x < new.x && new.x < close.x);
                for bounds in [back, new, close] {
                    assert!((bounds.center_y() - title.center_y()).abs() < 0.01);
                }
            }
            if parent == InspectorTab::Assistant {
                assert!(snapshot.nodes.iter().any(
                    |node| node.id == "inspector-page-back" && node.name == "Back to Assistant"
                ));
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
#[ignore = "Opt-in original Chemical names header and scrolling body"]
async fn chemical_names_keeps_its_fixed_header_import_and_body_scroll() {
    let mut ui = Ui::new().await;
    ui.viewport = Rectangle::with_size(Size::new(940., 620.));
    let mut app = drawing();
    app.tab.selected.clear();
    open(&mut app, InspectorTab::Names);
    let title = ui.text(&app, "Chemical names");
    let intro = ui.text(&app, "Parse names locally, or confirm an online lookup.");
    let name = ui.text(&app, "NAME → STRUCTURE");
    let header = |ui: &mut Ui, app: &App| {
        let snapshot = ui.semantics(app);
        ["inspector-page-back", "naming.import"].map(|id| {
            snapshot
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap()
                .bounds
        })
    };
    let controls = header(&mut ui, &app);
    let parse = |ui: &mut Ui, app: &App| {
        ui.semantics(app)
            .nodes
            .iter()
            .find(|node| node.id == "naming.resolve")
            .unwrap()
            .bounds
    };
    let parse_before = parse(&mut ui, &app);
    assert_eq!(controls[0].y, controls[1].y);
    assert!(controls[0].y < title.y && title.y < intro.y && intro.y < name.y);
    ui.run_task(
        &mut app,
        iced::widget::operation::scroll_to(
            "inspector-content",
            iced::widget::operation::AbsoluteOffset {
                x: None,
                y: Some(120.),
            },
        ),
    )
    .await;
    assert_eq!(ui.text(&app, "Chemical names"), title);
    assert_eq!(
        ui.text(&app, "Parse names locally, or confirm an online lookup."),
        intro
    );
    assert_eq!(header(&mut ui, &app), controls);
    assert!(parse(&mut ui, &app).y < parse_before.y);
    ui.focus_control(&mut app, "naming.import").await;
    ui.key(
        &mut app,
        Key::Named(Named::Enter),
        Code::Enter,
        Modifiers::empty(),
        None,
    )
    .await;
    assert_eq!(app.inspector_tab, InspectorTab::Import);
}

#[tokio::test]
#[ignore = "Opt-in field-owned Esc before inspector page dismissal"]
async fn inspector_page_escape_unfocuses_the_field_before_returning() {
    let mut ui = Ui::new().await;
    for (page, field) in [
        (InspectorTab::Names, "naming.name"),
        (InspectorTab::Assistant, "assistant-input"),
    ] {
        let mut app = drawing();
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
        assert_eq!(app.inspector_tab, page, "the field owns the first Esc");
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
            assert_eq!(app.tab.naming.input_name(), "x");
        } else {
            assert_eq!(app.assistant.input_text(), "x");
        }
        assert!(!app.tab.history.can_undo());
    }
}

#[tokio::test]
#[ignore = "Opt-in original Chemical names layout and selected structure target"]
async fn chemical_names_structure_entry_keeps_original_order_and_selected_target() {
    let mut ui = Ui::new().await;
    let mut app = drawing();
    let atoms = app.tab.selected.clone();
    app.tab.selected.clear();
    open(&mut app, InspectorTab::Names);
    let name = ui.text(&app, "NAME → STRUCTURE");
    let structure = ui.text(&app, "STRUCTURE → NAME");
    assert!(name.y < structure.y);
    let _ = app.update(Message::Naming(naming::Action::OpenPanel(atoms.clone())));
    let name = ui.text(&app, "NAME → STRUCTURE");
    let structure = ui.text(&app, "STRUCTURE → NAME");
    assert!(name.y < structure.y);
    assert_eq!(app.tab.selected, atoms);
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
    assert!(ui.text(&app, "NAME → STRUCTURE").y < ui.text(&app, "STRUCTURE → NAME").y);
    assert_eq!(app.tab.selected, atoms);
}
