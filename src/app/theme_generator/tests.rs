use super::*;
#[test]
fn imported_inherited_ring_fills_preview_like_the_canvas() {
    let mut theme = ThemeFile::capture(&Document::default());
    theme.light.ring_fills.clear();
    theme.dark.ring_fills.clear();
    let mut editor = Editor::new(&Document::default(), 0, &[]);
    editor.load_theme(theme.clone(), Selection::Draft);
    editor.preview = Preview::Rings;
    for mode in CanvasTheme::ALL {
        let mut doc = preview_molecule();
        doc.canvas_theme = mode;
        theme.clone().apply(&mut doc).unwrap();
        for hue in Hue::ALL {
            editor.fill = hue;
            let ids = doc.all_ids();
            reshiki::ring_fills::apply(&mut doc, &ids, Some(Paint::Palette(hue, Row::Tint)));
            let expected = Palette::of(&doc).rgb(doc.ring_fills[0].color);
            assert_eq!(editor.colors("N", mode).1, expected);
            assert_eq!(editor.fill_color(hue, mode), expected);
        }
    }
}
#[test]
fn imports_preserve_authored_palettes_until_controls_change_and_back_discards() {
    let (mut app, _) = App::new();
    let original = app.tab.doc.clone();
    let _ = app.theme_generator_action(Action::Open);
    let template = theme_files::bundled().unwrap().remove(0);
    let mut imported = Recipe::PRESENTATION.generate(&template).unwrap();
    // Explicit palette values are authoritative even if the recipe differs.
    imported
        .light
        .elements
        .insert("N".into(), theme_files::ColorValue::Rgb([50, 110, 65]));
    app.review_imported_theme(imported.clone());
    assert_eq!(
        app.theme_library
            .editor
            .as_ref()
            .unwrap()
            .candidate()
            .unwrap(),
        imported
    );
    let _ = app.theme_generator_action(Action::Bold(false));
    let _ = app.theme_generator_action(Action::Preview(Preview::Tiles));
    assert_eq!(
        app.theme_library
            .editor
            .as_ref()
            .unwrap()
            .candidate()
            .unwrap(),
        imported
    );
    assert_eq!(app.tab.doc, original);
    assert!(app.theme_library.themes().is_empty());
    let _ = app.theme_generator_action(Action::Back);
    assert_eq!(app.tab.doc, original);
    assert!(app.theme_library.themes().is_empty());
    let _ = app.theme_generator_action(Action::Open);
    app.review_imported_theme(imported.clone());
    let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Light, 80.));
    assert_ne!(
        app.theme_library
            .editor
            .as_ref()
            .unwrap()
            .candidate()
            .unwrap()
            .light,
        imported.light
    );
    assert_eq!(app.tab.doc, original);
}
#[test]
fn manager_protects_defaults_updates_customs_and_deletes_without_recoloring() {
    let (mut app, _) = App::new();
    let original = app.tab.doc.clone();
    let _ = app.theme_generator_action(Action::Open);
    for theme in ColorTheme::ALL {
        let _ = app.theme_generator_action(Action::Select(Selection::Builtin(theme)));
        let _ = app.theme_generator_action(Action::Delete);
        assert_eq!(app.tab.doc, original);
        assert!(app.theme_library.themes().is_empty());
    }
    let _ =
        app.theme_generator_action(Action::Select(Selection::Builtin(ColorTheme::Presentation)));
    let _ = app.theme_generator_action(Action::Apply);
    let saved = app.theme_library.themes()[0].clone();
    assert_ne!(saved.id, "presentation");
    let _ = app.theme_generator_action(Action::Name("Renamed custom".into()));
    let _ = app.theme_generator_action(Action::Apply);
    assert_eq!(app.theme_library.themes().len(), 1);
    assert_eq!(app.theme_library.themes()[0].id, saved.id);
    assert_eq!(app.theme_library.themes()[0].name, "Renamed custom");
    let themed = app.tab.doc.clone();
    let _ = app.theme_generator_action(Action::Delete);
    assert!(app.theme_library.themes().is_empty());
    assert_eq!(app.tab.doc, themed);
    let (choices, _) = app.theme_choices();
    assert_eq!(choices.len(), 5); // Four protected defaults and Manage themes.
    assert_eq!(
        choices.last(),
        Some(&super::super::theme_files::Choice::Manage)
    );
    assert!(
        !choices
            .iter()
            .any(|c| matches!(c, super::super::theme_files::Choice::Library { .. }))
    );
}
#[test]
fn new_drafts_get_distinct_names_and_imports_cannot_replace_a_newer_draft() {
    let (mut app, _) = App::new();
    let _ = app.theme_generator_action(Action::Open);
    let _ = app.theme_generator_action(Action::Select(Selection::New));
    let _ = app.theme_generator_action(Action::Apply);
    let _ = app.theme_generator_action(Action::Select(Selection::New));
    assert_eq!(
        app.theme_library.editor.as_ref().unwrap().name,
        "My theme 2"
    );
    let serial = app.theme_library.serial;
    let theme = theme_files::bundled().unwrap().remove(0);
    let _ = app.theme_generator_action(Action::Name("Newer draft".into()));
    let _ = app.theme_file_action(super::super::theme_files::Action::Loaded(
        app.tab.file_epoch,
        serial,
        Ok(Some(Box::new(theme))),
    ));
    assert_eq!(
        app.theme_library.editor.as_ref().unwrap().name,
        "Newer draft"
    );
}
#[test]
fn reference_choice_changes_both_modes_and_presets_restore_jmol() {
    let (mut app, _) = App::new();
    let _ = app.theme_generator_action(Action::Open);
    let editor = app.theme_library.editor.as_ref().unwrap();
    let reference = editor
        .references
        .iter()
        .find(|r| r.name == "Pastel")
        .unwrap()
        .clone();
    let before = editor.palette.clone();
    let _ = app.theme_generator_action(Action::Reference((&reference).into()));
    let editor = app.theme_library.editor.as_ref().unwrap();
    assert_eq!(editor.recipe.reference, Some(reference.clone()));
    assert_ne!(editor.palette.light.elements, before.light.elements);
    assert_ne!(editor.palette.dark.elements, before.dark.elements);
    let _ = app.theme_generator_action(Action::Apply);
    let _ = app.theme_generator_action(Action::Open);
    assert_eq!(
        app.theme_library.editor.as_ref().unwrap().recipe.reference,
        Some(reference)
    );
    let _ = app.theme_generator_action(Action::Preset(ColorTheme::Presentation));
    assert_eq!(
        app.theme_library.editor.as_ref().unwrap().recipe,
        Recipe::PRESENTATION
    );
}
#[test]
fn preview_nitrogen_has_a_chemically_correct_visible_hydrogen() {
    let doc = preview_molecule();
    let molecule = reshiki::chemistry::document::prepare(&doc).unwrap();
    let nitrogen = doc.atoms.iter().position(|a| a.element == "N").unwrap();
    assert_eq!(molecule.state.valences[nitrogen].implicit_hydrogens, 1);
    assert_eq!(doc.atoms[nitrogen].label_h, 1);
    let svg = reshiki::scene::svg(&doc);
    let tree = roxmltree::Document::parse(&svg).unwrap();
    assert!(
        tree.descendants()
            .any(|n| n.is_text() && n.text() == Some("H")),
        "{svg}"
    );
}
#[test]
fn color_tiles_keep_exact_element_colors_and_readable_symbols() {
    let mut editor = Editor::new(&Document::default(), 0, &[]);
    editor.preview = Preview::Colors;
    for lightness in [0., 0.52, 1.] {
        for chroma in [0., 0.65, 1., 1.5, 2.] {
            editor.recipe.light = reshiki::theme_generator::Tone { lightness, chroma };
            editor.recipe.dark = editor.recipe.light;
            editor.refresh();
            for mode in CanvasTheme::ALL {
                for &element in reshiki::editing::ELEMENTS {
                    let (ink, background) = editor.colors(element, mode);
                    assert_eq!(background, editor.palette.element_color(element, mode));
                    assert!(ink == [0; 3] || ink == [255; 3]);
                    assert!(
                        color_contrast::contrast(ink, background) >= color_contrast::TEXT_MIN,
                        "{element}/{mode}/{lightness}/{chroma}"
                    );
                }
            }
        }
    }
}
#[test]
fn drafts_cancel_without_edits_and_saved_recipes_reopen_with_undo() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Regular.document(42., false);
    let before = app.tab.doc.clone();
    let _ = app.theme_generator_action(Action::Open);
    let _ = app.theme_generator_action(Action::Lightness(CanvasTheme::Light, 43.));
    let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Dark, 150.));
    let recipe = app.theme_library.editor.as_ref().unwrap().recipe.clone();
    assert_eq!(recipe.dark.chroma, 1.5);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Escape);
    assert!(app.theme_library.editor.is_none());
    assert_eq!(app.tab.doc, before);
    let _ = app.theme_generator_action(Action::Open);
    let _ = app.theme_generator_action(Action::Name("My palette".into()));
    let _ = app.theme_generator_action(Action::Lightness(CanvasTheme::Light, 43.));
    let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Dark, 150.));
    let _ = app.theme_generator_action(Action::Apply);
    assert!(!app.error, "{}", app.status);
    let applied = app.tab.doc.clone();
    assert_eq!(
        applied.custom_theme.as_ref().unwrap().generator,
        Some(recipe.clone())
    );
    assert_eq!(applied.drawing_style, before.drawing_style);
    assert_eq!(applied.atoms, before.atoms);
    assert_eq!(applied.canvas_theme, before.canvas_theme);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, applied);
    let _ = app.theme_generator_action(Action::Open);
    let editor = app.theme_library.editor.as_ref().unwrap();
    assert_eq!(editor.recipe, recipe);
    assert_eq!(editor.name, "My palette");
    assert_eq!(editor.palette.id, applied.custom_theme.unwrap().id);
}
#[test]
fn stale_and_invalid_drafts_cannot_apply_and_tables_cover_every_element() {
    let (mut app, _) = App::new();
    let before = app.tab.doc.clone();
    let _ = app.theme_generator_action(Action::Open);
    let _ = app.theme_generator_action(Action::Name(" ".into()));
    let _ = app.theme_generator_action(Action::Apply);
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    let _ = app.theme_generator_action(Action::Name("Valid".into()));
    app.tab.file_epoch += 1;
    let _ = app.theme_generator_action(Action::Apply);
    assert!(app.error);
    assert_eq!(app.tab.doc, before);
    let symbols: std::collections::HashSet<_> = ROWS
        .iter()
        .flat_map(|row| row.split_whitespace())
        .filter(|s| *s != ".")
        .collect();
    assert_eq!(symbols.len(), 118);
    assert!(
        reshiki::editing::ELEMENTS
            .iter()
            .all(|e| symbols.contains(e))
    );
}
#[tokio::test]
#[ignore = "Manual theme workbench snapshots in a temporary directory"]
async fn theme_generator_headless_snapshot() {
    use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let _ = app.theme_generator_action(Action::Open);
    let _ =
        app.theme_generator_action(Action::Select(Selection::Builtin(ColorTheme::Presentation)));
    let directory = std::env::temp_dir().join("reshiki-theme-generator-qa");
    std::fs::create_dir_all(&directory).unwrap();
    for (name, width, height, dark, preview) in [
        ("light", 1280, 820, false, Preview::Labels),
        ("regular", 1280, 820, false, Preview::Labels),
        ("dark", 1280, 820, true, Preview::Tiles),
        ("compact", 1040, 680, false, Preview::Rings),
        ("color-tiles-light", 1280, 820, false, Preview::Colors),
        ("color-tiles-dark", 1280, 820, true, Preview::Colors),
        ("high-intensity", 1280, 820, true, Preview::Colors),
        ("reference-formula", 1280, 820, true, Preview::Labels),
    ] {
        app.appearance.mode = if dark {
            crate::appearance::Mode::Dark
        } else {
            crate::appearance::Mode::Light
        };
        let _ = app.theme_generator_action(Action::Preview(preview));
        let _ = app.theme_generator_action(Action::Bold(name != "regular"));
        if name == "high-intensity" {
            let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Light, 150.));
            let _ = app.theme_generator_action(Action::Chroma(CanvasTheme::Dark, 200.));
        }
        if name == "reference-formula" {
            let reference = app
                .theme_library
                .editor
                .as_ref()
                .unwrap()
                .references
                .iter()
                .find(|r| r.name == "Pastel")
                .unwrap()
                .clone();
            let _ = app.theme_generator_action(Action::Reference((&reference).into()));
        }
        let mut renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .unwrap();
        let size = iced::Size::new(width as f32, height as f32);
        let theme = app.theme();
        let mut view = app.view();
        let mut tree = Tree::new(view.as_widget());
        let node =
            view.as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let mut messages = Vec::new();
        view.as_widget_mut().update(
            &mut tree,
            &iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &iced::Rectangle::with_size(size),
        );
        if name == "reference-formula" {
            for x in [300., width as f32 - 40.] {
                view.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Pixels { x: 0., y: -2000. },
                    }),
                    iced::advanced::Layout::new(&node),
                    mouse::Cursor::Available(iced::Point::new(x, 350.)),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut iced::advanced::Shell::new(&mut messages),
                    &iced::Rectangle::with_size(size),
                );
            }
        }
        view.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &iced::advanced::renderer::Style::default(),
            iced::advanced::Layout::new(&node),
            mouse::Cursor::Unavailable,
            &iced::Rectangle::with_size(size),
        );
        let pixels = Headless::screenshot(
            &mut renderer,
            iced::Size::new(width, height),
            1.,
            theme.palette().background,
        );
        image::save_buffer(
            directory.join(format!("{name}.png")),
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    eprintln!("Theme generator screenshots: {}", directory.display());
}
