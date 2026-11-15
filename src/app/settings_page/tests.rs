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
    assert_eq!(app.appearance.copy_background, Background::Custom);
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

/// Picking a saved color, as in the approved design, copies the next
/// pictures on it; the export background stays.
#[test]
fn picking_a_saved_color_copies_pictures_on_it() {
    let mut app = app();
    assert_eq!(app.copy_pictures(None).background, Paper::Transparent);
    let navy = [30, 42, 68];
    app.appearance.background_colors = vec![navy];
    let _ = app.update(Message::Settings(Action::Custom(navy)));
    assert_eq!(app.copy_pictures(None).background, Paper::Color(navy));
    assert_eq!(app.appearance.export_background, Background::Canvas);
}

/// The picker adds a palette color without touching the drawing, and closes.
#[test]
fn the_color_picker_saves_a_palette_color_without_changing_the_drawing() {
    let mut app = app();
    let drawing = app.tab.doc.clone();
    let _ = app.update(Message::Settings(Action::AddColor));
    assert!(app.settings.color.is_some());
    let _ = app.update(Message::Settings(Action::CloseColor));
    assert!(
        app.settings.color.is_none(),
        "Escape or a click outside closes it"
    );
    let _ = app.update(Message::Settings(Action::AddColor));
    let _ = app.update(Message::Settings(Action::PickColor([40, 99, 171])));
    assert!(app.settings.color.is_none());
    assert_eq!(app.appearance.background_colors, [[40, 99, 171]]);
    assert_eq!(
        app.copy_pictures(None).background,
        Paper::Color([40, 99, 171])
    );
    assert_eq!(app.tab.doc, drawing);
}

/// Settings shows a failed save of the preferences that save on their own.
#[test]
fn failed_update_and_assistant_saves_show_in_their_sections() {
    let mut app = app();
    assert_eq!(app.section_error(Section::Updates), None);
    assert_eq!(app.section_error(Section::Assistant), None);
    let _ = app.update(Message::Updates(super::super::updates::Action::Saved(Err(
        "disk full".into(),
    ))));
    let _ = app.update(Message::Assistant(
        super::super::assistant::Action::PreferencesSaved(Err("read-only".into())),
    ));
    assert_eq!(
        app.section_error(Section::Updates),
        Some("Could not save update preferences: disk full")
    );
    assert_eq!(
        app.section_error(Section::Assistant),
        Some("Could not save assistant preferences: read-only")
    );
    let _ = app.update(Message::Assistant(
        super::super::assistant::Action::PreferencesSaved(Ok(())),
    ));
    assert_eq!(app.section_error(Section::Assistant), None);
}

/// A failed save of the other preferences shows in the section of the
/// setting, whichever control changed it, until a save works.
#[test]
fn failed_preference_saves_show_in_their_section_until_a_save_works() {
    let mut app = app();
    app.preference_saved(Section::CopyExport, "background", Err("disk full".into()));
    let shown = "Could not save the background preference: disk full";
    assert_eq!(app.section_error(Section::CopyExport), Some(shown));
    assert_eq!(app.status, shown);
    assert_eq!(app.section_error(Section::Drawing), None);
    // The Updates section keeps its own failure.
    assert_eq!(app.section_error(Section::Updates), None);
    // A control shared with the workspace saves the same file.
    let _ = app.update(Message::View(
        super::super::view_settings::Action::SmartGuides(false),
    ));
    assert_eq!(app.section_error(Section::CopyExport), None);
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
    assert_eq!(pictures.dpi, 300);
    let _ = app.update(Message::Settings(Action::CopyDpi(Dpi::ALL[0])));
    assert_eq!(app.copy_pictures(None).dpi, 150);
}

#[test]
fn png_resolution_rows_and_the_export_tab_change_the_preferences() {
    let mut app = app();
    let _ = app.update(Message::Settings(Action::CopyDpi(Dpi::ALL[2])));
    assert_eq!(app.appearance.copy_dpi.get(), 600);
    assert_eq!(app.appearance.export_dpi.get(), 1200, "only copies");
    let _ = app.update(Message::InspectorAction(
        super::super::inspector::Action::ResolutionMenu(true),
    ));
    assert!(app.tab.inspector_ui.menu_open());
    let _ = app.update(Message::Settings(Action::ExportDpi(Dpi::ALL[1])));
    assert_eq!(app.appearance.export_dpi.get(), 300);
    assert_eq!(app.appearance.copy_dpi.get(), 600);
    assert!(
        !app.tab.inspector_ui.menu_open(),
        "a choice closes the menu"
    );
    assert!(!app.error, "{}", app.status);
}

/// Close-ups render once per drawing revision, and only while Settings or
/// the Export tab's PNG resolution shows them.
#[test]
fn close_ups_render_once_per_drawing_revision() {
    let mut app = app();
    let key = |app: &App| {
        let tab = &app.tab;
        (tab.id, tab.file_epoch, tab.revision, tab.label_generation)
    };
    let _ = app.update(Message::Settings(Action::Query(String::new())));
    assert_eq!(app.settings.rendering, None, "hidden: nothing renders");
    let _ = app.update(Message::Settings(Action::Open(Some(Section::CopyExport))));
    let first = key(&app);
    assert_eq!(app.settings.rendering, Some(first));
    let preview = Preview::render(&app.tab.doc);
    let _ = app.update(Message::Settings(Action::Preview(first, preview)));
    assert_eq!(app.settings.rendering, None);
    let Some((shown, Ok(preview))) = &app.settings.preview else {
        panic!("close-ups are kept");
    };
    assert_eq!(*shown, first);
    assert!(preview.sample, "an empty drawing shows the sample");
    assert_eq!(preview.tiles.len(), 4);
    // The same revision does not render again.
    let _ = app.update(Message::Settings(Action::Query("dpi".into())));
    assert_eq!(app.settings.rendering, None);

    app.tab
        .doc
        .add_atom("N", reshiki::document::Point::default());
    app.tab.revision += 1;
    let _ = app.update(Message::Settings(Action::Query(String::new())));
    let second = key(&app);
    assert_eq!(app.settings.rendering, Some(second));
    // A result for an older revision is dropped, and does not end the newer job.
    let stale = Preview::render(&app.tab.doc);
    let _ = app.update(Message::Settings(Action::Preview(first, stale)));
    assert_eq!(app.settings.rendering, Some(second));
    let _ = app.update(Message::Settings(Action::Preview(
        second,
        Preview::render(&app.tab.doc),
    )));
    let Some((shown, Ok(preview))) = &app.settings.preview else {
        panic!("close-ups are kept");
    };
    assert_eq!(*shown, second);
    assert!(!preview.sample);
    // The older job finishing last neither replaces the close-ups nor starts
    // another render.
    let late = Preview::render(&Document::default());
    let _ = app.update(Message::Settings(Action::Preview(first, late)));
    let Some((shown, Ok(preview))) = &app.settings.preview else {
        panic!("close-ups are kept");
    };
    assert_eq!(*shown, second);
    assert!(!preview.sample);
    assert_eq!(app.settings.rendering, None);
    // Labels from the chemistry engine change the drawing without a revision.
    app.tab.label_generation += 1;
    let _ = app.update(Message::Settings(Action::Query(String::new())));
    let third = key(&app);
    assert_eq!(app.settings.rendering, Some(third));
    let _ = app.update(Message::Settings(Action::Preview(
        third,
        Preview::render(&app.tab.doc),
    )));

    // The Export tab's PNG resolution also needs the drawing's size.
    let _ = app.update(Message::Settings(Action::Close));
    app.tab.revision += 1;
    let _ = app.update(Message::Inspector(super::super::InspectorTab::Export));
    assert_eq!(app.settings.rendering, None, "only for PNG");
    let _ = app.update(Message::InspectorAction(
        super::super::inspector::Action::Figure(super::super::inspector::FigureFormat::Png),
    ));
    assert_eq!(app.settings.rendering, Some(key(&app)));
}

#[test]
fn close_ups_show_the_same_square_at_every_resolution_and_describe_the_whole_drawing() {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", reshiki::document::Point::default());
    let oxygen = doc.add_atom("O", reshiki::document::Point::new(42., 0.));
    doc.add_bond(carbon, oxygen, 1, "plain");
    let preview = Preview::render(&doc).unwrap();
    assert!(!preview.sample);
    let sides: Vec<_> = preview
        .tiles
        .iter()
        .map(|tile| match tile {
            image::Handle::Rgba { width, height, .. } => {
                assert_eq!(width, height);
                *width
            }
            _ => panic!("close-ups are pixels"),
        })
        .collect();
    assert_eq!(sides, [25, 50, 100, 200]);
    let (width, height) = reshiki::export::png_pixels(preview.size, 1200);
    let cm = |css: f32| css / 96. * 2.54;
    assert_eq!(
        preview.describe(Dpi::ALL[3]),
        format!(
            "1200 dpi · {:.1} × {:.1} cm · {width} × {height} px",
            cm(preview.size.0),
            cm(preview.size.1)
        )
    );
    assert!(Preview::render(&sample_structure()).unwrap().size.0 > 0.);
}

/// The real view's texts and accessible controls, popovers included.
fn rendered(
    app: &App,
    renderer: &mut iced::Renderer,
) -> (Vec<String>, Vec<reshiki::accessibility::Node>) {
    use iced::advanced::widget::{Id, Operation, operation};
    use iced_runtime::{UserInterface, user_interface::Cache};
    struct Texts(Vec<String>);
    impl Operation for Texts {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn text(&mut self, _: Option<&Id>, _: Rectangle, text: &str) {
            self.0.push(text.into());
        }
    }
    let size = Size::new(1280., 820.);
    let mut ui = UserInterface::build(app.view(), size, Cache::new(), renderer);
    let mut texts = Texts(Vec::new());
    ui.operate(renderer, &mut texts);
    let mut collect = reshiki::accessibility::Collect::new(Rectangle::with_size(size));
    ui.operate(renderer, &mut operation::black_box(&mut collect));
    (texts.0, collect.snapshot().nodes.clone())
}

/// Save failures show in search results too; the unknown-configuration row
/// waits for a running name; the color picker is reachable by keyboard.
#[tokio::test]
#[ignore = "Opt-in real App widget tree check"]
async fn settings_rows_show_failures_wait_for_naming_and_reach_the_picker() {
    use iced::advanced::renderer::Headless;
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let mut app = app();
    let _ = app.update(Message::Updates(super::super::updates::Action::Saved(Err(
        "disk full".into(),
    ))));
    let _ = app.update(Message::Assistant(
        super::super::assistant::Action::PreferencesSaved(Err("read-only".into())),
    ));
    open(&mut app, Section::Appearance);
    for (query, shown) in [
        ("channel", "Could not save update preferences: disk full"),
        ("accept", "Could not save assistant preferences: read-only"),
    ] {
        let _ = app.update(Message::Settings(Action::Query(query.into())));
        let (texts, _) = rendered(&app, &mut renderer);
        assert!(texts.iter().any(|text| text == shown), "{query}: {texts:?}");
    }

    let unknown = |app: &App, renderer: &mut iced::Renderer| {
        rendered(app, renderer)
            .1
            .into_iter()
            .find(|node| node.id == "settings-unknown-stereo")
            .expect("the unknown-configuration row")
            .enabled
    };
    let mut app = self::app();
    evidence_app(&mut app);
    open(&mut app, Section::Names);
    assert!(unknown(&app, &mut renderer));
    let _ = app.update(Message::Naming(super::super::naming::Action::Generate));
    assert!(!app.naming_idle());
    assert!(!unknown(&app, &mut renderer), "waits for the running name");

    let mut app = self::app();
    open(&mut app, Section::CopyExport);
    let _ = app.update(Message::Settings(Action::AddColor));
    let (_, nodes) = rendered(&app, &mut renderer);
    for id in [COLOR, "settings-save-color"] {
        assert!(nodes.iter().any(|node| node.id == id), "missing {id}");
    }
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

/// Delivers the close-ups of the drawing as it is now.
fn show_close_ups(app: &mut App) {
    let tab = &app.tab;
    let key = (tab.id, tab.file_epoch, tab.revision, tab.label_generation);
    let preview = Preview::render(&app.tab.doc);
    let _ = app.update(Message::Settings(Action::Preview(key, preview)));
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
    let states: [(&str, Setup); 16] = [
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
        ("settings-copy-export-picker", |app| {
            open(app, Section::CopyExport);
            let _ = app.update(Message::Settings(Action::AddColor));
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
        ("settings-png-resolution", |app| {
            app.appearance.copy_dpi = Dpi::ALL[0];
            open(app, Section::CopyExport);
            let _ = app.update(Message::Settings(Action::Query("resolution".into())));
            show_close_ups(app);
        }),
        ("settings-png-resolution-empty", |app| {
            app.tab.doc = Document::default();
            app.tab.revision += 1;
            open(app, Section::CopyExport);
            let _ = app.update(Message::Settings(Action::Query("resolution".into())));
            show_close_ups(app);
        }),
        ("export-tab-png", |app| {
            let _ = app.update(Message::Inspector(crate::app::InspectorTab::Export));
            let _ = app.update(Message::InspectorAction(
                crate::app::inspector::Action::Figure(crate::app::inspector::FigureFormat::Png),
            ));
            show_close_ups(app);
            let _ = app.update(Message::InspectorAction(
                crate::app::inspector::Action::ResolutionMenu(true),
            ));
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
