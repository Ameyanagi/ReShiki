use super::*;
use crate::app::{assistant, naming};
use crate::canvas::Edit;
use reshiki::document::Point;

fn drawing() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let carbon = app.tab.doc.add_atom("C", Point::default());
    let oxygen = app.tab.doc.add_atom("O", Point::new(42., 0.));
    app.tab.doc.add_bond(carbon, oxygen, 1, "plain");
    app.tab.selected = vec![carbon, oxygen];
    app
}

#[test]
fn names_back_and_escape_preserve_the_drawing_and_parent_tab() {
    for previous in [
        InspectorTab::Properties,
        InspectorTab::Templates,
        InspectorTab::Import,
    ] {
        for close in [
            Message::InspectorAction(inspector::Action::BackPage),
            Message::Escape,
        ] {
            let mut app = drawing();
            let _ = app.update(Message::Inspector(previous));
            let before = (
                app.tab.doc.clone(),
                app.tab.selected.clone(),
                app.tab.revision,
                app.tab.camera.center,
                app.tab.camera.zoom,
                app.viewport,
                app.inspector_width(),
            );
            let _ = app.update(Message::Inspector(InspectorTab::Names));
            assert_eq!(app.inspector_parent_tab(), previous);
            assert!(app.inspector_pages.structure_first);
            let _ = app.update(Message::Naming(naming::Action::Name("ethanol".into())));
            let _ = app.update(close);
            assert_eq!(app.inspector_tab, previous);
            assert!(app.inspector_open);
            assert_eq!(app.tab.naming.name, "ethanol");
            assert_eq!(
                (
                    app.tab.doc.clone(),
                    app.tab.selected.clone(),
                    app.tab.revision,
                    app.tab.camera.center,
                    app.tab.camera.zoom,
                    app.viewport,
                    app.inspector_width()
                ),
                before
            );
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[test]
fn assistant_returns_to_the_preceding_page_and_keeps_its_parent_tab() {
    for (previous, parent) in [
        (InspectorTab::Predict, InspectorTab::Predict),
        (InspectorTab::Templates, InspectorTab::Templates),
        (InspectorTab::Import, InspectorTab::Import),
        (InspectorTab::Export, InspectorTab::Export),
        (InspectorTab::Names, InspectorTab::Properties),
        (InspectorTab::Pages, InspectorTab::Properties),
    ] {
        for close in [
            Message::InspectorAction(inspector::Action::BackPage),
            Message::Escape,
        ] {
            let mut app = drawing();
            let _ = app.update(Message::Inspector(previous));
            if previous == InspectorTab::Templates {
                app.templates.add_menu_open = true;
            }
            let _ = app.update(Message::Assistant(assistant::Action::Open));
            assert!(
                !app.templates.add_menu_open,
                "a hidden Add menu cannot swallow page Escape"
            );
            assert_eq!(app.inspector_parent_tab(), parent);
            // Reopening the same page must not replace its return address.
            let _ = app.update(Message::Assistant(assistant::Action::Open));
            let _ = app.update(close);
            assert!(app.inspector_open);
            assert_eq!(app.inspector_tab, previous);
            assert_eq!(app.inspector_parent_tab(), parent);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[test]
fn new_selection_returns_from_either_page_through_canvas_and_context_routes() {
    for page in [InspectorTab::Names, InspectorTab::Assistant] {
        for route in 0..3 {
            let mut app = drawing();
            let carbon = app.tab.selected[0];
            app.tab.selected.clear();
            let _ = app.update(Message::Inspector(page));
            let document = app.tab.doc.clone();
            let message = match route {
                0 => Message::Canvas(Edit::Select(vec![carbon])),
                1 => Message::Canvas(Edit::ContextMenu {
                    position: iced::Point::new(30., 40.),
                    selected: vec![carbon],
                    hit: vec![carbon],
                }),
                _ => Message::SelectAll,
            };
            let _ = app.update(message);
            assert_eq!(
                app.inspector_tab,
                InspectorTab::Properties,
                "{page:?}, route {route}"
            );
            assert!(app.inspector_open);
            assert!(!app.tab.selected.is_empty());
            assert_eq!(app.tab.doc, document);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[test]
fn selection_order_and_document_switch_do_not_discard_the_open_page() {
    for page in [InspectorTab::Names, InspectorTab::Assistant] {
        let mut app = drawing();
        let original = app.tab.id;
        let _ = app.update(Message::Inspector(page));
        let mut selected = app.tab.selected.clone();
        selected.reverse();
        let _ = app.update(Message::Canvas(Edit::Select(selected)));
        assert_eq!(app.inspector_tab, page);
        // Keep Assistant's existing "Go to drawing" route when another
        // document comes forward; changing document is not a canvas selection.
        let _ = app.update(Message::New);
        assert_ne!(app.tab.id, original);
        assert_eq!(app.inspector_tab, page);
        let _ = app.update(Message::Tabs(crate::app::tabs::Action::Select(original)));
        assert_eq!(app.inspector_tab, page);
    }
}

#[test]
fn chemical_names_entry_point_sets_order_after_selecting_its_structure() {
    let mut app = drawing();
    let atoms = app.tab.selected.clone();
    app.tab.selected.clear();
    let _ = app.update(Message::Inspector(InspectorTab::Names));
    assert!(!app.inspector_pages.structure_first);
    let _ = app.update(Message::Naming(naming::Action::OpenPanel(atoms.clone())));
    assert_eq!(app.inspector_tab, InspectorTab::Names);
    assert_eq!(app.tab.selected, atoms);
    assert!(app.inspector_pages.structure_first);
    let _ = app.update(Message::Assistant(assistant::Action::Open));
    let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
    assert_eq!(app.inspector_tab, InspectorTab::Names);
    assert!(app.inspector_pages.structure_first);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn escape_dismisses_assistant_menu_before_its_page() {
    let mut app = drawing();
    let _ = app.update(Message::Inspector(InspectorTab::Predict));
    let _ = app.update(Message::Assistant(assistant::Action::Open));
    app.assistant.menu = Some(assistant::Menu::Models);
    let _ = app.update(Message::Escape);
    assert_eq!(app.assistant.menu, None);
    assert_eq!(app.inspector_tab, InspectorTab::Assistant);
    let _ = app.update(Message::Escape);
    assert_eq!(app.inspector_tab, InspectorTab::Predict);
}

#[test]
fn publication_keeps_its_existing_selection_route() {
    let mut app = drawing();
    let _ = app.update(Message::Inspector(InspectorTab::Pages));
    let _ = app.update(Message::Canvas(Edit::Select(vec![])));
    assert_eq!(app.inspector_tab, InspectorTab::Pages);
    assert!(app.inspector_page_header().is_none());
}
