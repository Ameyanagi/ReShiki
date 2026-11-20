use super::super::color_popover::{Action, Menu};
use super::super::{App, Message, inline_text};
use iced::widget::text_editor::{Action as Input, Edit};
use reshiki::document::{Annotation, Point};
use reshiki::typography::Script;

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app
}
fn keys(app: &mut App, typed: &str) {
    for c in typed.chars() {
        let _ = app.update(Message::CaptionAction(Input::Edit(Edit::Insert(c))));
    }
}
fn begin(app: &mut App) {
    let _ = app.update(Message::InlineText(inline_text::Action::Begin(
        None,
        Point::new(10., 20.),
    )));
}

#[test]
fn typed_shortcuts_become_symbols_and_undo_brings_the_shortcut_back() {
    let mut app = app();
    begin(&mut app);
    keys(&mut app, "A <=> B, h\\nu");
    assert_eq!(app.tab.caption, "A ⇌ B, hν");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.caption, "A ⇌ B, h\\nu");
    keys(&mut app, " 2-3");
    assert_eq!(app.tab.caption, "A ⇌ B, h\\nu 2-3", "a hyphen stays");
}

#[test]
fn a_shortcut_takes_the_style_around_it_and_keeps_its_neighbours() {
    use iced::widget::text_editor::Motion;
    let mut app = app();
    begin(&mut app);
    keys(&mut app, "ab");
    let _ = app.update(Message::CaptionAction(Input::Select(Motion::Left)));
    let _ = app.update(Message::TextStyle(
        reshiki::typography::StyleChange::Script(Script::Superscript),
    ));
    let _ = app.update(Message::CaptionAction(Input::Move(Motion::Left)));
    keys(&mut app, "+");
    let typed = app.tab.caption_format.at(1).script;
    keys(&mut app, "-");
    assert_eq!(app.tab.caption, "a±b");
    let format = &app.tab.caption_format;
    assert_eq!(format.at(0).script, Script::Normal);
    assert_eq!(format.at(1).script, typed, "± keeps the typed style");
    assert_eq!(format.at("a±".len()).script, Script::Superscript, "b");
}

#[test]
fn a_shortcut_expands_on_later_lines_and_in_right_to_left_text() {
    let mut app = app();
    begin(&mut app);
    let _ = app.update(Message::CaptionAction(Input::Edit(Edit::Paste(
        std::sync::Arc::new("first\nאב -".into()),
    ))));
    keys(&mut app, ">");
    assert_eq!(app.tab.caption, "first\nאב →");
}

#[test]
fn picker_inserts_at_the_cursor_and_remembers_recents() {
    let mut app = app();
    begin(&mut app);
    keys(&mut app, "25 C");
    let _ = app.update(Message::CaptionAction(Input::Move(
        iced::widget::text_editor::Motion::Left,
    )));
    let _ = app.update(Message::StyleMenu(Action::Symbols));
    assert!(matches!(app.style_menu, Some(Menu::Symbols { .. })));
    let _ = app.update(Message::StyleMenu(Action::Insert("°")));
    let _ = app.update(Message::StyleMenu(Action::Insert("α")));
    let _ = app.update(Message::StyleMenu(Action::Insert("°")));
    assert_eq!(app.tab.caption, "25 °α°C");
    assert!(
        matches!(app.style_menu, Some(Menu::Symbols { .. })),
        "stays open"
    );
    assert_eq!(app.appearance.recent_symbols, ["°", "α"]);
    let _ = app.update(Message::Escape);
    assert!(app.style_menu.is_none());
    assert!(
        app.tab.inline_text.is_some(),
        "Escape closes only the picker"
    );
    assert!(app.finish_inline(true));
    assert_eq!(app.tab.doc.annotations[0].text, "25 °α°C");
}

#[test]
fn picker_without_a_draft_edits_the_selected_text_box_or_starts_one() {
    let mut app = app();
    app.tab.camera.center = Point::new(120., 80.);
    let _ = app.update(Message::StyleMenu(Action::Symbols));
    let _ = app.update(Message::StyleMenu(Action::Insert("→")));
    assert_eq!(app.tab.caption, "→");
    assert!(app.style_menu.is_none(), "closes to show the new caption");
    assert!(app.finish_inline(true));
    assert_eq!(app.tab.doc.annotations.len(), 1);
    assert_eq!(app.tab.doc.annotations[0].position, Point::new(120., 80.));
    let id = app.tab.doc.next_id();
    app.tab.doc.annotations.push(Annotation {
        angle: 0.,
        id,
        position: Point::new(50., 60.),
        text: "heat".into(),
        format: Default::default(),
    });
    app.tab.selected = vec![id];
    let before = app.tab.doc.clone();
    let _ = app.update(Message::StyleMenu(Action::Insert("Δ")));
    assert!(app.finish_inline(true));
    let label = app.tab.doc.annotations.iter().find(|a| a.id == id).unwrap();
    assert_eq!(label.text, "heatΔ");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before, "one Undo step");
}

#[test]
fn insert_symbol_from_the_palette_keeps_the_draft_and_its_cursor() {
    use super::super::command_palette::Action as Palette;
    use iced::widget::text_editor::Motion;
    let mut app = app();
    begin(&mut app);
    keys(&mut app, "AB");
    let _ = app.update(Message::CaptionAction(Input::Move(Motion::Left)));
    let _ = app.update(Message::Commands(Palette::Open));
    let _ = app.update(Message::Commands(Palette::Query("insert symbol".into())));
    assert_eq!(
        app.commands.current().map(|c| c.name.as_str()),
        Some("Insert symbol…")
    );
    let _ = app.update(Message::Commands(Palette::Submit));
    assert!(matches!(app.style_menu, Some(Menu::Symbols { .. })));
    assert!(app.tab.inline_text.is_some(), "the draft stays open");
    let _ = app.update(Message::StyleMenu(Action::Insert("α")));
    assert_eq!(app.tab.caption, "AαB");
    assert!(app.tab.doc.annotations.is_empty(), "nothing committed yet");
}
