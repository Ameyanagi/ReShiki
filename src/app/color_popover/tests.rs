use super::*;
use reshiki::document::History;

fn app() -> App {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.history = History::default();
    app
}
fn act(app: &mut App, action: Action) {
    let _ = app.update(Message::StyleMenu(action));
}
fn editing(app: &App) -> Option<&HueEdit> {
    match &app.style_menu {
        Some(Menu::Color { edit, .. }) => edit.as_ref(),
        _ => None,
    }
}

#[test]
fn menus_toggle_close_on_other_commands_and_picks_keep_the_popover_open() {
    let mut app = app();
    act(&mut app, Action::Color);
    assert!(matches!(app.style_menu, Some(Menu::Color { .. })));
    act(&mut app, Action::Color);
    assert!(app.style_menu.is_none(), "The color button closes it again");
    act(&mut app, Action::Align);
    act(&mut app, Action::Color);
    assert!(
        matches!(app.style_menu, Some(Menu::Color { .. })),
        "One menu at a time"
    );
    let blue = Paint::Palette(Hue::Blue, Row::Strong);
    let _ = app.update(Message::ColorScope(ColorScope::Bonds));
    let _ = app.update(Message::TextStyle(StyleChange::Color(blue)));
    assert!(app.style_menu.is_some(), "Picking keeps the popover open");
    assert!(app.tab.doc.bonds.iter().all(|b| b.color == blue));
    // The edit restarts the delayed property refresh, which must not close it.
    let _ = app.update(Message::InspectorAction(
        super::super::inspector::Action::RefreshProperties,
    ));
    assert!(app.style_menu.is_some());
    let _ = app.update(Message::Escape);
    assert!(app.style_menu.is_none());
    act(&mut app, Action::Align);
    let _ = app.update(Message::TextAlign(TextAlign::Center));
    assert!(
        app.style_menu.is_none(),
        "Choosing an alignment closes its menu"
    );
    act(&mut app, Action::Color);
    let _ = app.update(Message::SelectAll);
    assert!(app.style_menu.is_none(), "Other commands close it");
    act(&mut app, Action::RingColor);
    assert_eq!(app.tab.color_scope, ColorScope::Rings);
    assert!(matches!(app.style_menu, Some(Menu::Color { .. })));
    assert!(app.rings_unfilled(), "No fill is the current swatch");
    let teal = Paint::Palette(Hue::Teal, Row::Tint);
    let _ = app.update(Message::TextStyle(StyleChange::Color(teal)));
    assert_eq!(app.current_selection_color(), Some(teal));
    assert!(!app.rings_unfilled() && app.style_menu.is_some());
    // The atom label editor ignores other messages, so it closes the popover.
    let atom = app.tab.doc.atoms.first().map(|a| a.id);
    let _ = app.update(Message::AtomText(super::super::atom_text::Action::Begin(
        atom,
    )));
    assert!(app.tab.atom_text.is_some() && app.style_menu.is_none());
}

#[test]
fn typed_colors_accept_every_format_and_invalid_text_shows_the_hint() {
    let mut app = app();
    act(&mut app, Action::Color);
    let invalid = |app: &App| matches!(app.style_menu, Some(Menu::Color { invalid: true, .. }));
    let _ = app.update(Message::TextColor("blue-ish".into()));
    let _ = app.update(Message::ApplyTextColor);
    assert!(invalid(&app));
    assert_eq!(app.status, HINT);
    let _ = app.update(Message::TextColor("31, 78, 121".into()));
    assert!(!invalid(&app), "Editing clears the hint");
    let _ = app.update(Message::ApplyTextColor);
    assert_eq!(
        app.current_selection_color(),
        Some(Paint::Custom([31, 78, 121]))
    );
    let _ = app.update(Message::TextColor("oklch(0.62 0.2 30)".into()));
    let _ = app.update(Message::ApplyTextColor);
    let Some(Paint::Custom(rgb)) = app.current_selection_color() else {
        panic!("custom color");
    };
    assert_eq!(app.tab.doc.recent_colors, [rgb, [31, 78, 121]]);
    assert_eq!(app.tab.text_color_input, reshiki::palette::hex(rgb));
    assert!(app.style_menu.is_some());
}

#[test]
fn a_hue_session_recolors_live_and_done_is_one_undo_step() {
    let mut app = app();
    let blue = Paint::Palette(Hue::Blue, Row::Strong);
    let _ = app.update(Message::TextStyle(StyleChange::Color(blue)));
    let before = app.tab.doc.clone();
    let shown = Palette::of(&app.tab.doc).rgb(blue);
    act(&mut app, Action::Color);
    act(&mut app, Action::EditHues);
    assert_eq!(editing(&app).map(|e| e.slot), Some(Hue::Blue));
    act(&mut app, Action::Chip(-3));
    assert_eq!(Hues::of(&app.tab.doc).get(Hue::Blue), 225);
    assert_ne!(Palette::of(&app.tab.doc).rgb(blue), shown, "Live recolor");
    assert_eq!(
        app.tab.doc.custom_theme.as_ref().map(|t| t.name.as_str()),
        Some("Publication · custom hues")
    );
    // The strip stays in place: the clicked chip is now the current hue.
    assert_eq!(editing(&app).map(|e| e.shift), Some(3));
    act(&mut app, Action::Step(1));
    assert_eq!(Hues::of(&app.tab.doc).get(Hue::Blue), 235);
    act(&mut app, Action::Slot(Hue::Red));
    act(&mut app, Action::Step(-1));
    assert_eq!(Hues::of(&app.tab.doc).get(Hue::Red), 15);
    act(&mut app, Action::Done);
    assert!(editing(&app).is_none() && app.style_menu.is_some());
    let after = app.tab.doc.clone();
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before, "The whole session is one step");
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, after);
}

#[test]
fn cancel_and_escape_restore_the_theme_and_closing_keeps_edits() {
    let mut app = app();
    let before = app.tab.doc.clone();
    act(&mut app, Action::Color);
    act(&mut app, Action::EditHues);
    act(&mut app, Action::Chip(4));
    assert_ne!(app.tab.doc, before);
    act(&mut app, Action::Cancel);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    act(&mut app, Action::EditHues);
    act(&mut app, Action::Chip(4));
    let _ = app.update(Message::Escape);
    assert_eq!(app.tab.doc, before, "Escape cancels the session");
    assert!(app.style_menu.is_some(), "and returns to picking");
    // Default hues on a built-in theme leave the drawing unchanged.
    act(&mut app, Action::EditHues);
    act(&mut app, Action::Chip(4));
    act(&mut app, Action::RestoreHues);
    act(&mut app, Action::Done);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    // A click outside keeps the edits, as Done does.
    act(&mut app, Action::EditHues);
    act(&mut app, Action::ResetHue);
    act(&mut app, Action::Chip(-6));
    act(&mut app, Action::Close);
    assert!(app.style_menu.is_none());
    assert_eq!(Hues::of(&app.tab.doc).get(Hue::Blue), 195);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn stepping_past_the_strip_ends_shows_the_next_six_chips() {
    let mut app = app();
    act(&mut app, Action::Color);
    act(&mut app, Action::EditHues);
    for _ in 0..7 {
        act(&mut app, Action::Step(1));
    }
    let edit = editing(&app).map(|e| (e.hues.get(Hue::Blue), e.shift));
    // 255° + 70°; the current chip stays within the strip.
    assert_eq!(edit, Some((325, -1)));
    act(&mut app, Action::Page(-6));
    assert_eq!(editing(&app).map(|e| e.shift), Some(-7));
    act(&mut app, Action::Slot(Hue::Red));
    assert_eq!(editing(&app).map(|e| e.shift), Some(0));
    for _ in 0..3 {
        act(&mut app, Action::Step(-1));
    }
    assert_eq!(Hues::of(&app.tab.doc).get(Hue::Red), 355, "Hues wrap at 0°");
}
