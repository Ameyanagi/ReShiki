use super::*;
use reshiki::document::{Document, Point};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.fit_to_view = false;
    app.appearance = crate::appearance::Settings::default();
    app.guides = Default::default();
    app
}

#[test]
fn view_menu_choices_keep_the_drawing_camera_and_history() {
    let mut app = ready();
    let id = app.tab.doc.add_atom("C", Point::new(120., 35.));
    app.tab.selected = vec![id];
    app.tab.camera.center = Point::new(80., -20.);
    app.tab.camera.zoom = 1.75;
    let doc = app.tab.doc.clone();
    let camera = app.tab.camera;
    let revision = app.tab.revision;
    let frames = app.tab.history.frames();
    let dirty = app.tab.dirty();
    for item in [
        Item::Grid,
        Item::Rulers,
        Item::Crosshair,
        Item::SmartGuides,
        Item::ArrangeControls,
    ] {
        let before = item.checked(&app).unwrap();
        let _ = app.update(Message::View(Action::Choose(item)));
        assert_eq!(
            item.checked(&app),
            Some(before),
            "A closed menu has no live rows"
        );
        let _ = app.update(Message::View(Action::Toggle));
        assert!(app.view_open);
        let _ = app.update(Message::View(Action::Choose(item)));
        assert!(!app.view_open);
        assert_eq!(item.checked(&app), Some(!before));
        assert_eq!(app.tab.doc, doc);
        assert_eq!(app.tab.selected, [id]);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.history.frames(), frames);
        assert_eq!(app.tab.dirty(), dirty);
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);
    }
}

#[test]
fn view_menu_escape_owns_dismissal_and_foreground_commands_close_it() {
    let mut app = ready();
    app.tool = crate::canvas::Tool::Text;
    let _ = app.update(Message::View(Action::Toggle));
    for message in [
        Message::Tick,
        Message::Viewport(iced::Size::new(800., 600.)),
        Message::Canvas(crate::canvas::Edit::Hover(Some(Point::default()))),
        Message::Nmr(super::super::nmr::Action::RenderedSize(iced::Size::new(
            500., 200.,
        ))),
        Message::Updates(super::super::updates::Action::Poll),
        Message::Updates(super::super::updates::Action::Check(false)),
    ] {
        let _ = app.update(message);
        assert!(app.view_open);
    }
    let _ = app.update(Message::Escape);
    assert!(!app.view_open);
    assert_eq!(
        app.tool,
        crate::canvas::Tool::Text,
        "Escape only closes View"
    );
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::View(Action::Toggle));
    let _ = app.update(Message::Settings(
        super::super::settings_page::Action::Open(None),
    ));
    assert!(!app.view_open);
    assert!(app.settings.open);
}

#[test]
fn view_menu_routes_page_and_canvas_choices_through_caption_commits() {
    use super::super::inline_text;
    for item in [Item::DarkCanvas, Item::PageSetup] {
        let mut app = ready();
        app.tab.doc = Document::default();
        let _ = app.update(Message::InlineText(inline_text::Action::Begin(
            None,
            Point::default(),
        )));
        let _ = app.update(Message::InlineText(inline_text::Action::ReplaceText(
            "note".into(),
        )));
        let _ = app.update(Message::View(Action::Toggle));
        assert!(app.tab.inline_text.is_some());
        assert!(app.tab.doc.annotations.is_empty());
        let _ = app.update(Message::View(Action::Choose(item)));
        assert!(app.tab.inline_text.is_none());
        assert!(!app.view_open);
        assert_eq!(app.tab.doc.annotations.len(), 1);
        assert_eq!(app.tab.doc.annotations[0].text, "note");
        let dark = app.tab.doc.canvas_theme;
        let _ = app.update(Message::Undo);
        assert!(app.tab.doc.annotations.is_empty());
        assert!(!app.tab.history.can_undo(), "The caption is one edit");
        assert_eq!(
            app.tab.doc.canvas_theme, dark,
            "Canvas view stays out of Undo"
        );
    }
}

#[test]
fn view_menu_closes_for_front_replacements_but_not_background_epochs() {
    let mut app = ready();
    let origin = app.tab.id;
    let _ = app.update(Message::View(Action::Toggle));
    app.add_tab();
    assert!(!app.view_open);
    let front = app.tab.id;
    let _ = app.update(Message::View(Action::Toggle));
    app.in_tab(origin, |app| app.begin_file_epoch()).unwrap();
    assert!(app.view_open);
    assert_eq!(app.tab.id, front);
    app.begin_file_epoch();
    assert!(!app.view_open);
    let _ = app.update(Message::View(Action::Toggle));
    app.reset_tab();
    assert!(!app.view_open);
}
