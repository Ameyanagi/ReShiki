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
            assert_eq!(app.tab.naming.input_name(), "ethanol");
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
        if page == InspectorTab::Names {
            assert!(
                !app.inspector_pages.structure_first,
                "An empty drawing starts with name input"
            );
        }
        let _ = app.update(Message::Tabs(crate::app::tabs::Action::Select(original)));
        assert_eq!(app.inspector_tab, page);
        if page == InspectorTab::Names {
            assert!(
                app.inspector_pages.structure_first,
                "The selected structure belongs to this drawing"
            );
        }
    }
}

#[test]
fn file_replacement_rebinds_page_context_without_losing_its_return_route() {
    for page in [InspectorTab::Names, InspectorTab::Assistant] {
        let mut app = drawing();
        let _ = app.update(Message::Inspector(InspectorTab::Import));
        let _ = app.update(Message::Inspector(page));
        let (tab, epoch) = (app.tab.id, app.tab.file_epoch);
        let mut replacement = reshiki::document::Document::default();
        replacement.add_atom("N", Point::default());
        app.show_opened(replacement.clone(), Some("replacement.rsk".into()));
        // show_opened runs inside a message; its normal final page sync must
        // bind the empty selection to this file epoch, even if atom IDs recur.
        let _ = app.sync_inspector_page_selection();
        assert_eq!(app.tab.id, tab, "The existing tab receives the file");
        assert_ne!(app.tab.file_epoch, epoch);
        assert_eq!(app.tab.doc, replacement);
        assert!(app.tab.selected.is_empty());
        assert_eq!(app.inspector_tab, page);
        assert_eq!(app.inspector_parent_tab(), InspectorTab::Import);
        if page == InspectorTab::Names {
            assert!(
                !app.inspector_pages.structure_first,
                "The replacement file has no selected structure"
            );
        }
        let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
        assert_eq!(app.inspector_tab, InspectorTab::Import);
        // Changing selection within this same replacement file is the other
        // rule: both pages return to Properties, as they did before replacement.
        let _ = app.update(Message::Inspector(page));
        let _ = app.update(Message::SelectAll);
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        assert!(!app.tab.selected.is_empty());
    }
}

#[test]
fn explicit_analysis_owns_properties_while_page_back_keeps_its_requested_scope() {
    for (direct, close) in [false, true].into_iter().flat_map(|direct| {
        [
            Message::Escape,
            Message::InspectorAction(inspector::Action::DismissMolecule),
        ]
        .map(|close| (direct, close))
    }) {
        let mut app = drawing();
        let before = (app.tab.doc.clone(), app.tab.selected.clone());
        let _ = app.update(Message::Inspector(InspectorTab::Import));
        let _ = app.update(Message::Inspector(InspectorTab::Assistant));
        let _ = app.update(Message::Inspector(InspectorTab::Names));
        if direct {
            let _ = app.show_molecular_properties();
        } else {
            let _ = app.update(Message::ContextMenu(
                crate::app::context_menu::Action::Properties(true),
            ));
        }
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        assert!(app.requested_molecular_properties());
        assert!(
            app.inspector_pages.previous.is_empty(),
            "Explicit analysis starts at Properties without an old page return target"
        );
        let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        // A new page visit returns to the requested Properties context, not
        // to the earlier advanced Import/Assistant/Names stack.
        let _ = app.update(Message::Inspector(InspectorTab::Assistant));
        let _ = app.update(Message::Inspector(InspectorTab::Names));
        let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
        assert_eq!(app.inspector_tab, InspectorTab::Assistant);
        assert!(app.requested_molecular_properties());
        let _ = app.update(Message::Escape);
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        assert!(app.requested_molecular_properties());
        let _ = app.update(close);
        assert_eq!(app.inspector_tab, InspectorTab::Properties);
        assert!(!app.requested_molecular_properties());
        assert_eq!((app.tab.doc.clone(), app.tab.selected.clone()), before);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn new_selection_clears_requested_analysis_and_nested_page_return_history() {
    let mut app = drawing();
    let carbon = app.tab.selected[0];
    let before = app.tab.doc.clone();
    let _ = app.show_molecular_properties();
    let _ = app.update(Message::Inspector(InspectorTab::Assistant));
    let _ = app.update(Message::Inspector(InspectorTab::Names));
    assert!(app.requested_molecular_properties());
    let _ = app.update(Message::Canvas(Edit::Select(vec![carbon])));
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert!(!app.requested_molecular_properties());
    let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    let _ = app.update(Message::Inspector(InspectorTab::Names));
    let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    assert!(!app.requested_molecular_properties());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, vec![carbon]);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn closing_commands_or_keys_keeps_page_history_and_requested_analysis() {
    for keys in [false, true] {
        let mut app = drawing();
        let before = (app.tab.doc.clone(), app.tab.selected.clone());
        let _ = app.show_molecular_properties();
        let _ = app.update(Message::Inspector(InspectorTab::Assistant));
        let _ = app.update(Message::Inspector(InspectorTab::Names));
        for page in [InspectorTab::Names, InspectorTab::Properties] {
            let _ = app.update(if keys {
                Message::ToggleHelp
            } else {
                Message::Commands(crate::app::command_palette::Action::Open)
            });
            assert_eq!((app.help_open, app.commands.open), (keys, !keys));
            let _ = app.update(Message::Escape);
            assert!(!app.help_open && !app.commands.open);
            assert_eq!(app.inspector_tab, page);
            assert!(app.requested_molecular_properties());
            if page == InspectorTab::Names {
                let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
                assert_eq!(app.inspector_tab, InspectorTab::Assistant);
                let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
                assert_eq!(app.inspector_tab, InspectorTab::Properties);
            }
        }
        let _ = app.update(Message::Escape);
        assert!(!app.requested_molecular_properties());
        assert_eq!((app.tab.doc.clone(), app.tab.selected.clone()), before);
        assert!(!app.tab.history.can_undo());
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

fn names_from_catalog(app: &mut App) {
    use crate::app::command_palette::Action;
    let _ = app.update(Message::Commands(Action::Open));
    let _ = app.update(Message::Commands(Action::Query(
        "Chemical names panel".into(),
    )));
    assert_eq!(app.commands.current().unwrap().name, "Chemical names panel");
    let _ = app.update(Message::Commands(Action::Submit));
    assert_eq!(app.inspector_tab, InspectorTab::Names);
}

fn open_style_draft(app: &mut App, tab: InspectorTab) {
    use crate::app::{document_styles, theme_generator};
    let (open, edit) = match tab {
        InspectorTab::DrawingStyle => (
            Message::DrawingStyle(document_styles::Action::Open),
            Message::DrawingStyle(document_styles::Action::Name("Unfinished style".into())),
        ),
        InspectorTab::ThemeGenerator => (
            Message::ThemeGenerator(theme_generator::Action::Open),
            Message::ThemeGenerator(theme_generator::Action::Name("Unfinished theme".into())),
        ),
        _ => unreachable!(),
    };
    let _ = app.update(open);
    let _ = app.update(edit);
    assert_eq!(app.inspector_tab, tab);
}

#[test]
fn nested_inspector_pages_keep_deliberate_draft_discard_routes() {
    use crate::app::{context_menu, document_styles, pages, theme_generator};

    for editor in [InspectorTab::DrawingStyle, InspectorTab::ThemeGenerator] {
        for route in 0..6 {
            let mut app = drawing();
            app.tab.selected.clear();
            let drawing = app.tab.doc.clone();
            open_style_draft(&mut app, editor);
            names_from_catalog(&mut app);
            let _ = app.update(Message::Inspector(InspectorTab::Assistant));
            let abandon = match route {
                0 => Message::Inspector(InspectorTab::Predict),
                1 => Message::SelectAll,
                2 => Message::ContextMenu(context_menu::Action::Properties(true)),
                3 => Message::Pages(pages::Action::Open),
                _ => {
                    for expected in [InspectorTab::Names, editor] {
                        let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
                        assert_eq!(app.inspector_tab, expected);
                    }
                    if route == 4 {
                        Message::Escape
                    } else if editor == InspectorTab::DrawingStyle {
                        Message::DrawingStyle(document_styles::Action::Cancel)
                    } else {
                        Message::ThemeGenerator(theme_generator::Action::Back)
                    }
                }
            };
            let _ = app.update(abandon);
            assert!(app.tab.styles.editor.is_none(), "{editor:?}, route {route}");
            assert!(
                app.theme_library.editor.is_none(),
                "{editor:?}, route {route}"
            );
            assert!(app.inspector_pages.previous.is_empty());
            let destination = app.inspector_tab;
            let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
            assert_eq!(
                app.inspector_tab, destination,
                "abandoned pages cannot return"
            );
            assert_eq!(app.tab.doc, drawing);
            assert!(!app.tab.history.can_undo());
        }
    }
}

#[test]
fn nested_inspector_pages_keep_draft_document_ownership() {
    use crate::app::tabs;

    for editor in [InspectorTab::DrawingStyle, InspectorTab::ThemeGenerator] {
        let mut app = drawing();
        let original = app.tab.id;
        let drawing = app.tab.doc.clone();
        open_style_draft(&mut app, editor);
        names_from_catalog(&mut app);
        let _ = app.update(Message::New);
        assert_ne!(app.tab.id, original);
        assert!(app.tab.styles.editor.is_none());
        assert!(app.theme_library.editor.is_none());
        let _ = app.update(Message::Tabs(tabs::Action::Select(original)));
        assert_eq!(app.inspector_tab, InspectorTab::Names);
        let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
        assert_eq!(app.inspector_tab, editor);
        assert_eq!(
            app.tab.styles.editor.is_some(),
            editor == InspectorTab::DrawingStyle,
            "Drawing style drafts belong to their document tab"
        );
        assert!(
            app.theme_library.editor.is_none(),
            "the shared theme editor is discarded on document activation"
        );
        assert_eq!(app.tab.doc, drawing);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn names_from_publication_catalog_returns_to_the_exact_page_and_its_draft() {
    use crate::app::pages;
    for close in [
        Message::InspectorAction(inspector::Action::BackPage),
        Message::Escape,
    ] {
        let mut app = drawing();
        let _ = app.update(Message::Pages(pages::Action::Open));
        let _ = app.update(Message::Pages(pages::Action::Input(
            pages::Field::Width,
            "123".into(),
        )));
        let draft = app.print_document().unwrap();
        let before = app.tab.doc.clone();
        names_from_catalog(&mut app);
        assert_eq!(app.inspector_parent_tab(), InspectorTab::Properties);
        let _ = app.update(close);
        assert_eq!(app.inspector_tab, InspectorTab::Pages);
        assert_eq!(app.inspector_parent_tab(), InspectorTab::Properties);
        assert!(app.tab.pages.editor.is_some());
        assert_eq!(app.print_document().unwrap(), draft);
        assert_eq!(app.tab.doc, before);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn names_from_assistant_catalog_returns_to_assistant_then_its_previous_page() {
    for close in [
        Message::InspectorAction(inspector::Action::BackPage),
        Message::Escape,
    ] {
        let mut app = drawing();
        let _ = app.update(Message::Inspector(InspectorTab::Export));
        let _ = app.update(Message::Inspector(InspectorTab::Assistant));
        names_from_catalog(&mut app);
        assert_eq!(app.inspector_parent_tab(), InspectorTab::Export);
        let _ = app.update(close);
        assert_eq!(app.inspector_tab, InspectorTab::Assistant);
        assert_eq!(app.inspector_parent_tab(), InspectorTab::Export);
        let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
        assert_eq!(app.inspector_tab, InspectorTab::Export);
        assert!(!app.tab.history.can_undo());
    }
}

#[test]
fn nested_page_visits_unwind_to_advanced_import_without_a_back_loop() {
    let mut app = drawing();
    let _ = app.update(Message::Inspector(InspectorTab::Import));
    names_from_catalog(&mut app);
    let _ = app.update(Message::Inspector(InspectorTab::Assistant));
    names_from_catalog(&mut app);
    for expected in [
        InspectorTab::Assistant,
        InspectorTab::Names,
        InspectorTab::Import,
    ] {
        assert_eq!(app.inspector_parent_tab(), InspectorTab::Import);
        let _ = app.update(Message::InspectorAction(inspector::Action::BackPage));
        assert_eq!(app.inspector_tab, expected);
    }
    assert!(app.inspector_open);
    assert!(!app.tab.history.can_undo());
}
