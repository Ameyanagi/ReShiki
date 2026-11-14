use super::*;

fn app() -> App {
    let (app, _) = App::new();
    app
}

#[test]
fn command_comma_opens_settings_and_escape_or_the_shortcut_closes_it() {
    let comma = Key::Character(",".into());
    assert!(matches!(
        super::super::shortcuts::key_message(&comma, &comma, Modifiers::COMMAND),
        Some(Message::Settings(Action::Open(None)))
    ));
    assert_eq!(
        super::super::shortcuts::label(&Message::Settings(Action::Open(None))).as_deref(),
        Some(super::super::shortcuts::keys(Modifiers::COMMAND, ",").as_str())
    );
    for (key, modifiers) in [
        (Key::Named(Named::Escape), Modifiers::empty()),
        (comma.clone(), Modifiers::COMMAND),
    ] {
        assert!(matches!(
            modal_key(&key, modifiers),
            Some(Message::Settings(Action::Close))
        ));
    }
    assert!(modal_key(&comma, Modifiers::empty()).is_none());
    assert!(modal_key(&Key::Character("a".into()), Modifiers::COMMAND).is_none());
}

#[test]
fn opening_keeps_the_last_section_and_closes_other_overlays() {
    let mut app = app();
    app.help_open = true;
    let _ = app.update(Message::Settings(Action::Open(Some(Section::Names))));
    assert!(app.settings.open && !app.help_open);
    assert_eq!(app.settings.section, Section::Names);
    let _ = app.update(Message::Settings(Action::Query("units".into())));
    let _ = app.update(Message::Settings(Action::Close));
    assert!(!app.settings.open);
    let _ = app.update(Message::Settings(Action::Open(None)));
    assert_eq!(app.settings.section, Section::Names);
    assert!(
        app.settings.query.is_empty(),
        "a new visit starts unfiltered"
    );
}

/// Each row changes the same preference as the control it mirrors.
#[test]
fn rows_change_the_same_preferences_as_the_existing_controls() {
    let mut app = app();
    let defaults = crate::appearance::Settings::default();
    assert!(
        Section::ALL
            .into_iter()
            .all(|section| !app.settings_groups(section).is_empty())
    );
    let _ = app.update(Message::Settings(Action::NewCanvas(CanvasTheme::Dark)));
    assert_eq!(app.appearance.canvas_theme, CanvasTheme::Dark);
    assert_eq!(
        app.tab.doc.canvas_theme, defaults.canvas_theme,
        "only new drawings"
    );
    let _ = app.update(Message::Settings(Action::DrawingUnit(Unit::Millimetres)));
    assert_eq!(app.appearance.drawing_style_unit, Unit::Millimetres);
    let _ = app.update(Message::Settings(Action::CopyBackground(Background::Black)));
    let _ = app.update(Message::Settings(Action::ExportBackground(
        Background::White,
    )));
    assert_eq!(app.appearance.copy_background, Background::Black);
    assert_eq!(app.appearance.export_background, Background::White);
    assert!(!app.error, "{}", app.status);
}

#[test]
fn search_filters_settings_by_name_across_sections() {
    let mut app = app();
    let _ = app.update(Message::Settings(Action::Open(Some(Section::Appearance))));
    let count = |app: &App, query: &str| {
        let words: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut names = Vec::new();
        for section in Section::ALL.into_iter().filter(|section| section.shown()) {
            for (title, items) in app.settings_groups(section) {
                names.extend(
                    items
                        .iter()
                        .filter(|item| found(&words, section, title, item))
                        .map(|item| item.name),
                );
            }
        }
        names
    };
    assert_eq!(count(&app, "smart"), ["Smart guides"]);
    assert_eq!(
        count(&app, "background"),
        ["Copied pictures", "Exported files", "Your colors"]
    );
    assert_eq!(
        count(&app, "editable pictures"),
        ["Put the editable drawing in pictures"]
    );
    assert!(count(&app, "no such setting").is_empty());
}

#[test]
fn saved_colors_are_parsed_once_deduplicated_and_become_custom() {
    let mut app = app();
    let _ = app.update(Message::Settings(Action::AddColor));
    let _ = app.update(Message::Settings(Action::ColorInput("not a color".into())));
    let _ = app.update(Message::Settings(Action::SaveColor));
    assert_eq!(app.settings.color, Some(("not a color".into(), true)));
    assert!(app.appearance.background_colors.is_empty());
    for input in ["#1E2A44", "#F4EFE6", "30, 42, 68"] {
        let _ = app.update(Message::Settings(Action::AddColor));
        let _ = app.update(Message::Settings(Action::AddColor));
        let _ = app.update(Message::Settings(Action::ColorInput(input.into())));
        let _ = app.update(Message::Settings(Action::SaveColor));
        assert_eq!(app.settings.color, None);
    }
    assert_eq!(
        app.appearance.background_colors,
        [[30, 42, 68], [244, 239, 230]],
        "newest first, without repeats"
    );
    assert_eq!(app.appearance.custom_background, [30, 42, 68]);
    let _ = app.update(Message::Settings(Action::Custom([244, 239, 230])));
    assert_eq!(app.appearance.custom_background, [244, 239, 230]);
    for value in 0..SAVED_COLORS as u8 + 2 {
        let _ = app.update(Message::Settings(Action::AddColor));
        let _ = app.update(Message::Settings(Action::ColorInput(format!(
            "{value}, {value}, {value}"
        ))));
        let _ = app.update(Message::Settings(Action::SaveColor));
    }
    assert_eq!(app.appearance.background_colors.len(), SAVED_COLORS);
}

#[test]
fn copies_use_the_copy_default_unless_one_copy_picks_a_background() {
    let mut app = app();
    app.appearance.export_editable_drawing = false;
    app.appearance.copy_background = Background::Custom;
    app.appearance.custom_background = [30, 42, 68];
    let pictures = app.copy_pictures(None);
    assert_eq!(pictures.background, Paper::Color([30, 42, 68]));
    assert!(!pictures.editable);
    let once = app.copy_pictures(Some(Paper::Canvas));
    assert_eq!(once.background, Paper::Canvas);
    assert_eq!(app.appearance.copy_background, Background::Custom);
}

/// A selected benzene ring, with two saved colors and a navy Custom.
fn evidence_app(app: &mut App) {
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.saved = app.tab.doc.clone();
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Select(
        app.tab.doc.all_ids(),
    )));
    app.appearance.background_colors = vec![[30, 42, 68], [244, 239, 230]];
    app.appearance.custom_background = [30, 42, 68];
}

fn open(app: &mut App, section: Section) {
    let _ = app.update(Message::Settings(Action::Open(Some(section))));
}

/// Opt-in screenshots of Settings, the Export tab and Copy image on, in the
/// light and dark interface. Set RESHIKI_UI_QA_SCALE=2 for 2× images.
#[tokio::test]
#[ignore = "Opt-in GPU snapshots; set RESHIKI_UI_QA_DIR and RESHIKI_DATA_DIR"]
async fn settings_snapshots() {
    use iced::advanced::{renderer::Headless, widget::Tree};
    type Setup = fn(&mut App);
    let states: [(&str, Setup); 12] = [
        ("toolbar", |_| {}),
        ("settings-appearance", |app| open(app, Section::Appearance)),
        ("settings-drawing", |app| open(app, Section::Drawing)),
        ("settings-copy-export", |app| {
            app.appearance.copy_background = Background::Custom;
            open(app, Section::CopyExport);
        }),
        ("settings-copy-export-add-color", |app| {
            open(app, Section::CopyExport);
            let _ = app.update(Message::Settings(Action::AddColor));
            let _ = app.update(Message::Settings(Action::ColorInput("teal".into())));
            let _ = app.update(Message::Settings(Action::SaveColor));
        }),
        ("settings-names", |app| open(app, Section::Names)),
        ("settings-assistant", |app| open(app, Section::Assistant)),
        ("settings-updates", |app| open(app, Section::Updates)),
        ("settings-search", |app| {
            open(app, Section::Appearance);
            let _ = app.update(Message::Settings(Action::Query("background".into())));
        }),
        ("export-tab", |app| {
            let _ = app.update(Message::Inspector(crate::app::InspectorTab::Export));
        }),
        ("export-tab-black", |app| {
            app.appearance.export_background = Background::Black;
            let _ = app.update(Message::Inspector(crate::app::InspectorTab::Export));
        }),
        ("copy-image-on", |app| {
            app.context_menu = Some(crate::app::context_menu::State::new(
                iced::Point::new(36., 24.),
                crate::app::context_menu::Page::Main,
            ));
            let _ = app.context_action(crate::app::context_menu::Action::Page(
                crate::app::context_menu::Page::CopyImageOn,
            ));
        }),
    ];
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let directory = std::env::var_os("RESHIKI_UI_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reshiki-settings-qa"));
    std::fs::create_dir_all(&directory).unwrap();
    let size = iced::Size::new(1280., 820.);
    for (name, setup) in states {
        for (suffix, mode) in [
            ("", crate::appearance::Mode::Light),
            ("-dark", crate::appearance::Mode::Dark),
        ] {
            let mut app = app();
            app.appearance.mode = mode;
            evidence_app(&mut app);
            setup(&mut app);
            let mut tree = Tree::empty();
            for _ in 0..3 {
                let messages = crate::app::workspace::layout_snapshots::pass(
                    &app,
                    &mut renderer,
                    &mut tree,
                    size,
                    None,
                );
                if messages.is_empty() {
                    break;
                }
                for message in messages {
                    let _ = app.update(message);
                }
            }
            let output = directory.join(format!("{name}{suffix}.png"));
            let _ = crate::app::workspace::layout_snapshots::pass(
                &app,
                &mut renderer,
                &mut tree,
                size,
                Some(&output),
            );
            eprintln!("{}", output.display());
        }
    }
}
