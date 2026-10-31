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
