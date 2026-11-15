use super::*;
use crate::app::template_library::{Action, Filter};
use iced::advanced::widget::Tree;

#[test]
fn template_navigation_keeps_browse_rows_in_place_after_returning_to_categories() {
    let (mut app, _) = App::new();
    let rows = |app: &App| {
        let controls: Element<'_, Message> =
            app.template_browse_controls(iced::widget::column![]).into();
        Tree::new(controls.as_widget())
            .children
            .iter()
            .map(|row| (row.tag, row.children.len()))
            .collect::<Vec<_>>()
    };
    let initial = rows(&app);
    assert_eq!(initial.len(), 4);
    assert_eq!(initial[1].1, 2, "Both navigation buttons share one row");
    assert!(!app.templates.can_forward());

    let _ = app.update(Message::Templates(Action::Collection("Aromatics".into())));
    assert_eq!(rows(&app), initial);
    let _ = app.update(Message::Templates(Action::Browse));
    assert_eq!(app.templates.collection, "All collections");
    assert!(app.templates.can_forward());
    assert_eq!(rows(&app), initial);
}

#[test]
fn template_search_scope_row_only_appears_with_an_open_category_and_query() {
    let (mut app, _) = App::new();
    for (collection, query, search_rows) in [
        ("All collections", "", 2),
        ("Aromatics", "", 2),
        ("Aromatics", "   ", 2),
        ("Aromatics", "Phe", 3),
        ("All collections", "Phe", 2),
    ] {
        app.templates.collection = collection.into();
        app.templates.query = query.into();
        app.templates.filter = Filter::Favorites;
        let controls: Element<'_, Message> =
            app.template_browse_controls(iced::widget::column![]).into();
        let tree = Tree::new(controls.as_widget());
        assert_eq!(tree.children.len(), 4);
        assert_eq!(tree.children[0].children.len(), search_rows);
        assert_eq!(tree.children[1].children.len(), 2);
    }
}

/// Template thumbnails keep their geometry between frames: a second frame of
/// the same panel builds none and shows the same pixels.
#[tokio::test]
#[ignore = "Opt-in renderer check of template thumbnails"]
async fn template_thumbnails_are_built_once_until_the_panel_changes() {
    use crate::canvas::PREVIEWS_BUILT;
    use iced::advanced::renderer::Headless;
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let (mut app, _) = App::new();
    app.appearance.mode = crate::appearance::Mode::Light;
    let _ = app.update(Message::Inspector(InspectorTab::Templates));
    let _ = app.update(Message::Templates(Action::Collection("Aromatics".into())));
    let directory = tempfile::tempdir().unwrap();
    let mut tree = Tree::empty();
    let mut frame = |app: &App, name: &str| {
        let output = directory.path().join(name);
        PREVIEWS_BUILT.set(0);
        layout_snapshots::pass(
            app,
            &mut renderer,
            &mut tree,
            iced::Size::new(1280., 820.),
            Some(&output),
        );
        (PREVIEWS_BUILT.get(), std::fs::read(output).unwrap())
    };
    let (built, first) = frame(&app, "first.png");
    assert!(built >= 8, "the first frame builds each thumbnail: {built}");
    let (built, second) = frame(&app, "second.png");
    assert_eq!(built, 0, "an unchanged panel rebuilt thumbnails");
    assert!(first == second, "the kept thumbnails changed the frame");
    // Another collection reuses the tiles' widget state for other templates.
    let _ = app.update(Message::Templates(Action::Collection(
        "Carbohydrates".into(),
    )));
    let (built, third) = frame(&app, "third.png");
    assert!(built > 0, "new templates must be drawn");
    assert!(first != third);
    let (built, _) = frame(&app, "fourth.png");
    assert_eq!(built, 0);
}
