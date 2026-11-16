//! Pins how the modal gates and draft predicates treat the cleanup, view-aid and graphic-input messages.
use super::preview_passthrough;
use crate::app::{
    App, Message, atom_text, cleanup, color_popover, context_menu, graphics, inline_text,
    inspector, joining, nmr, tabs, updates, view_settings,
};
use crate::canvas::{Edit, Tool, guides::Unit};
use reshiki::cleanup::Scope;
use reshiki::graphics::{BracketSides, GraphicChange, GraphicKind, LinePattern};
use reshiki::scientific::{Phase, SymbolKind};

fn open_transient_menus(inspector_menu: inspector::Action) -> App {
    let (mut app, _) = App::new();
    app.tabs.menu = true;
    app.imports.menu = true;
    app.imports.examples_menu = true;
    app.tab.inspector_ui.update(inspector_menu);
    app.context_menu = Some(context_menu::State::new(
        iced::Point::new(180., 240.),
        context_menu::Page::Main,
    ));
    app
}

#[test]
fn viewport_resize_keeps_transient_menus_open() {
    for menu in [
        inspector::Action::FigureMenu(true),
        inspector::Action::ChemicalMenu(true),
    ] {
        let mut app = open_transient_menus(menu);
        let _ = app.update(Message::Viewport(iced::Size::new(1040., 720.)));

        assert!(app.tabs.menu);
        assert!(app.context_menu.is_some());
        assert!(app.tab.inspector_ui.menu_open());
        assert!(app.imports.menu);
        assert!(app.imports.examples_menu);
    }
}

#[test]
fn nmr_panel_size_keeps_transient_menus_open() {
    for menu in [
        inspector::Action::FigureMenu(true),
        inspector::Action::ChemicalMenu(true),
    ] {
        let mut app = open_transient_menus(menu);
        let _ = app.update(Message::Nmr(nmr::Action::RenderedSize(iced::Size::new(
            360., 440.,
        ))));

        assert!(app.tabs.menu);
        assert!(app.context_menu.is_some());
        assert!(app.tab.inspector_ui.menu_open());
        assert!(app.imports.menu);
        assert!(app.imports.examples_menu);
    }
}

fn open_style_menu(actions: &[color_popover::Action]) -> App {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.history = reshiki::document::History::default();
    for action in actions {
        let _ = app.update(Message::StyleMenu(*action));
    }
    app
}

#[test]
fn resizes_keep_the_style_menus_and_a_hue_edit_open() {
    use color_popover::{Action, Menu};
    for resize in [
        Message::Viewport(iced::Size::new(1040., 720.)),
        Message::Nmr(nmr::Action::RenderedSize(iced::Size::new(360., 440.))),
    ] {
        let context = format!("{resize:?}");
        let mut app = open_style_menu(&[Action::Align]);
        let _ = app.update(resize.clone());
        assert!(matches!(app.style_menu, Some(Menu::Align)), "{context}");

        let mut app = open_style_menu(&[Action::Color]);
        let _ = app.update(resize.clone());
        assert!(
            matches!(app.style_menu, Some(Menu::Color { edit: None, .. })),
            "{context}"
        );

        let mut app = open_style_menu(&[Action::Color, Action::EditHues]);
        let original = app.tab.doc.clone();
        let _ = app.update(Message::StyleMenu(Action::Chip(-3)));
        assert_ne!(app.tab.doc, original, "{context}: the hues preview live");
        let previewed = app.tab.doc.clone();
        let _ = app.update(resize);
        assert!(
            matches!(app.style_menu, Some(Menu::Color { edit: Some(_), .. })),
            "{context}: the hue edit stays open"
        );
        assert_eq!(app.tab.doc, previewed, "{context}: the preview is kept");
        assert!(
            !app.tab.history.can_undo(),
            "{context}: nothing is committed"
        );
    }
}

#[test]
fn ordinary_tool_change_closes_transient_menus() {
    let mut app = open_transient_menus(inspector::Action::FigureMenu(true));
    let _ = app.update(Message::Tool(Tool::Select));

    assert!(!app.tabs.menu);
    assert!(app.context_menu.is_none());
    assert!(!app.tab.inspector_ui.menu_open());
    assert!(!app.imports.menu);
    assert!(!app.imports.examples_menu);
}

/// Columns: (message, preview_passthrough, commits_draft).
fn cases() -> Vec<(Message, bool, bool)> {
    vec![
        (
            Message::Canvas(Edit::Navigate(
                crate::canvas::Navigation::Pan(iced::Vector::new(42., -18.)),
                iced::Vector::new(100., -40.),
            )),
            true,
            false,
        ),
        (
            Message::Canvas(Edit::Navigate(
                crate::canvas::Navigation::Zoom(2.),
                iced::Vector::new(100., -40.),
            )),
            true,
            false,
        ),
        (Message::Cleanup(cleanup::Action::Begin), false, true),
        (Message::Cleanup(cleanup::Action::ToWedge), false, true),
        (Message::Cleanup(cleanup::Action::ToFischer), false, true),
        (Message::Cleanup(cleanup::Action::Menu(true)), false, false),
        (
            Message::Cleanup(cleanup::Action::Scope(Scope::SelectedMolecules)),
            false,
            false,
        ),
        (
            Message::Cleanup(cleanup::Action::Orientation(
                reshiki::cleanup::Orientation::Keep,
            )),
            false,
            false,
        ),
        (Message::View(view_settings::Action::Grid), true, false),
        (
            Message::View(view_settings::Action::SmartGuides(false)),
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::TwoDecimalWeight(true)),
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::ExportEditableDrawing(false)),
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::ExportSvgLiveText(true)),
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::Rulers(true)),
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::Crosshair(true)),
            true,
            false,
        ),
        (
            Message::View(view_settings::Action::RulerUnit(Unit::Inches)),
            true,
            false,
        ),
        (Message::View(view_settings::Action::Toggle), true, false),
        (
            Message::Graphics(graphics::Action::Style(GraphicChange::Pattern(
                LinePattern::Dashed,
            ))),
            false,
            true,
        ),
        (
            Message::Graphics(graphics::Action::Width("2".into())),
            false,
            false,
        ),
        (
            Message::Graphics(graphics::Action::ApplyWidth),
            false,
            false,
        ),
        (
            Message::Graphics(graphics::Action::Stroke("#117E6C".into())),
            false,
            false,
        ),
        (
            Message::Graphics(graphics::Action::ApplyStroke),
            false,
            false,
        ),
        (
            Message::Graphics(graphics::Action::Fill("#DCEFE9".into())),
            false,
            false,
        ),
        (Message::Graphics(graphics::Action::ApplyFill), false, false),
        (
            Message::Graphics(graphics::Action::Sides(BracketSides::Left)),
            false,
            false,
        ),
        (
            Message::Graphics(graphics::Action::ScientificKind(GraphicKind::Symbol(
                SymbolKind::CircleMinus,
            ))),
            false,
            false,
        ),
        (
            Message::Graphics(graphics::Action::OrbitalPhase(Phase::Shaded)),
            false,
            false,
        ),
        (
            Message::Graphics(graphics::Action::FlipPhase(true)),
            false,
            false,
        ),
        (
            Message::Graphics(graphics::Action::AttachSymbols(false)),
            false,
            false,
        ),
    ]
}

#[test]
fn clustered_messages_keep_their_draft_and_background_predicates() {
    let (mut app, _) = App::new();
    for (message, passthrough, commits) in cases() {
        let context = format!("{message:?}");
        assert_eq!(preview_passthrough(&message), passthrough, "{context}");
        assert_eq!(inline_text::commits_draft(&message), commits, "{context}");
        assert_eq!(joining::cancels_draft(&message), commits, "{context}");
        assert!(!color_popover::keeps_open(&message), "{context}");
        assert!(!atom_text::background(&message), "{context}");
        assert!(!updates::background(&message), "{context}");
        assert!(!tabs::document_result(&message), "{context}");
        assert!(!app.reveals_inspector(&message), "{context}");
        app.tabs.menu = true;
        app.imports.menu = true;
        app.imports.examples_menu = true;
        app.cleanup_menu = true;
        let menu = matches!(message, Message::Cleanup(cleanup::Action::Menu(_)));
        app.close_transient_menus(&message);
        assert!(!app.tabs.menu, "{context}");
        assert!(!app.imports.menu, "{context}");
        assert!(!app.imports.examples_menu, "{context}");
        assert_eq!(app.cleanup_menu, menu, "Choosing an option closes the menu");
    }
}

#[test]
fn native_pointer_hover_preserves_the_existing_hover_gates_and_transient_menus() {
    for message in [
        Message::Canvas(Edit::PointerHover(Some(iced::Vector::new(100., -40.)))),
        Message::Canvas(Edit::PointerHover(None)),
    ] {
        assert!(preview_passthrough(&message));
        assert!(!inline_text::commits_draft(&message));
        assert!(!joining::cancels_draft(&message));
        assert!(color_popover::keeps_open(&message));
        let mut app = open_transient_menus(inspector::Action::FigureMenu(true));
        app.close_transient_menus(&message);
        assert!(app.context_menu.is_some());
        assert!(app.tabs.menu);
        assert!(app.imports.menu);
        assert!(app.imports.examples_menu);
    }
}
