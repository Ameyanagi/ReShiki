use super::*;
use reshiki::canvas_theme::ColorTheme;
use reshiki::document::{Document, Point};

fn ready() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.fit_to_view = false;
    app
}

#[test]
fn opening_and_dismissing_the_style_chooser_keeps_the_drawing_and_caption_draft() {
    let mut app = ready();
    let atom = app.tab.doc.add_atom("C", Point::new(120., 35.));
    app.tab.selected = vec![atom];
    app.tab.camera.center = Point::new(80., -20.);
    app.tab.camera.zoom = 1.75;
    let _ = app.update(Message::InlineText(
        super::super::inline_text::Action::Begin(None, Point::default()),
    ));
    let _ = app.update(Message::InlineText(
        super::super::inline_text::Action::ReplaceText("note".into()),
    ));
    let doc = app.tab.doc.clone();
    let selection = app.tab.selected.clone();
    let camera = app.tab.camera;
    let revision = app.tab.revision;
    let history = app.tab.history.frames();
    let dirty = app.tab.dirty();
    for close in [Message::StatusStyle(Action::Close), Message::Escape] {
        let _ = app.update(Message::StatusStyle(Action::Toggle));
        assert!(app.status_style_open);
        assert!(app.tab.inline_text.is_some());
        let _ = app.update(close);
        assert!(!app.status_style_open);
        assert!(app.tab.inline_text.is_some());
        assert_eq!(app.tab.doc, doc);
        assert_eq!(app.tab.selected, selection);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.history.frames(), history);
        assert_eq!(app.tab.dirty(), dirty);
        assert_eq!(app.tab.camera.center, camera.center);
        assert_eq!(app.tab.camera.zoom, camera.zoom);
    }
}

#[test]
fn chooser_choices_keep_independent_style_and_theme_undo_and_manager_routes() {
    let mut app = ready();
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let original = app.tab.doc.clone();
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    let _ = app.update(Message::QuickDrawingStyle(
        document_styles::Choice::Journal(Preset::Nature),
    ));
    assert!(app.status_style_open, "Both lists stay available");
    assert_eq!(app.tab.doc.drawing_style, Preset::Nature.style());
    assert_eq!(app.tab.doc.color_theme, original.color_theme);
    let journal = app.tab.doc.clone();
    let _ = app.update(Message::ThemeFile(theme_files::Action::Choose(
        theme_files::Choice::Builtin(ColorTheme::Pastel),
    )));
    assert!(app.status_style_open);
    assert_eq!(app.tab.doc.drawing_style, journal.drawing_style);
    assert_eq!(app.tab.doc.atoms, journal.atoms);
    let _ = app.update(Message::Undo);
    assert!(!app.status_style_open);
    assert_eq!(app.tab.doc, journal);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, original);
    assert!(!app.tab.history.can_undo());

    let _ = app.update(Message::StatusStyle(Action::Toggle));
    let _ = app.update(Message::QuickDrawingStyle(document_styles::Choice::Details));
    assert!(!app.status_style_open);
    assert!(app.tab.styles.editor.is_some());
    assert_eq!(app.tab.doc, original);
    let _ = app.update(Message::DrawingStyle(document_styles::Action::Cancel));
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    let _ = app.update(Message::ThemeFile(theme_files::Action::Choose(
        theme_files::Choice::Manage,
    )));
    assert!(!app.status_style_open);
    assert_eq!(
        app.inspector_tab,
        super::super::InspectorTab::ThemeGenerator
    );
    assert_eq!(app.tab.doc, original);
}

#[test]
fn document_custom_styles_and_embedded_themes_keep_their_full_identity() {
    let mut app = ready();
    app.tab.doc.drawing_style.font_size_pt += 0.25;
    assert_eq!(app.status_journal(), document_styles::Choice::Custom);
    let mut theme = reshiki::theme_files::bundled().unwrap().remove(0);
    theme.id = "deleted-theme".into();
    theme.name = "A long embedded theme with 日本語 and a family 👩‍👩‍👧‍👦".into();
    app.tab.doc.custom_theme = Some(Box::new(theme.clone()));
    let (choices, selected) = app.theme_choices();
    assert!(
        !choices.contains(&selected),
        "Do not recreate a deleted library entry"
    );
    assert_eq!(selected.to_string(), theme.name);
    let label = compact(&format!("Custom · {selected}"));
    assert!(label.ends_with('…'));
    assert!(super::super::workspace::text_width(&label, 11.) <= 184.);
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    let _ = app.update(Message::QuickDrawingStyle(
        document_styles::Choice::Journal(Preset::Jacs),
    ));
    assert_eq!(app.tab.doc.custom_theme.as_deref(), Some(&theme));
    let _ = app.update(Message::ThemeFile(theme_files::Action::Choose(selected)));
    assert_eq!(app.tab.doc.custom_theme.as_deref(), Some(&theme));
    let _ = app.update(Message::ThemeFile(theme_files::Action::Choose(
        theme_files::Choice::Builtin(ColorTheme::Publication),
    )));
    assert!(app.tab.doc.custom_theme.is_none());
}

#[test]
fn style_chooser_survives_background_work_and_closes_for_front_replacements() {
    let mut app = ready();
    let origin = app.tab.id;
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    app.add_tab();
    assert!(!app.status_style_open);
    let front = app.tab.id;
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    app.in_tab(origin, |app| app.begin_file_epoch()).unwrap();
    assert!(app.status_style_open);
    assert_eq!(app.tab.id, front);
    let refresh = app.property_request_key();
    let _ = app.update(Message::InspectorAction(
        super::super::inspector::Action::RefreshPropertiesFor(refresh),
    ));
    assert!(app.status_style_open);
    let _ = app.update(Message::Tick);
    assert!(app.status_style_open);
    app.begin_file_epoch();
    assert!(!app.status_style_open);
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    app.reset_tab();
    assert!(!app.status_style_open);
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    app.show_opened(Document::default(), Some("opened.rsk".into()));
    assert!(!app.status_style_open);
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    let _ = app.update(Message::Settings(
        super::super::settings_page::Action::Open(None),
    ));
    assert!(!app.status_style_open);
    assert!(app.settings.open);
}

#[test]
fn opening_the_chooser_is_a_view_action_but_choosing_a_style_remains_an_edit() {
    for action in [Action::Toggle, Action::Close] {
        let message = Message::StatusStyle(action);
        assert!(super::super::gates::preview_passthrough(&message));
        assert!(!super::super::inline_text::commits_draft(&message));
    }
    for message in [
        Message::QuickDrawingStyle(document_styles::Choice::Journal(Preset::Nature)),
        Message::ThemeFile(theme_files::Action::Choose(theme_files::Choice::Builtin(
            ColorTheme::Pastel,
        ))),
        Message::ColorTheme(ColorTheme::Pastel),
    ] {
        assert!(!super::super::gates::preview_passthrough(&message));
    }
}

#[test]
fn a_rejected_manager_choice_keeps_the_live_chooser_during_a_3d_preview() {
    let mut app = ready();
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    let _ = app.update(Message::Optimization(
        super::super::optimization::Action::Begin,
    ));
    assert!(app.tab.optimization.is_some());
    let drawing = app.tab.doc.clone();
    let _ = app.update(Message::StatusStyle(Action::Toggle));
    for message in [
        Message::QuickDrawingStyle(document_styles::Choice::Details),
        Message::ThemeFile(theme_files::Action::Choose(theme_files::Choice::Manage)),
    ] {
        let _ = app.update(message);
        assert!(app.status_style_open);
        assert!(app.tab.optimization.is_some());
        assert!(app.tab.styles.editor.is_none());
        assert!(app.theme_library.editor.is_none());
        assert_eq!(app.tab.doc, drawing);
    }
}
