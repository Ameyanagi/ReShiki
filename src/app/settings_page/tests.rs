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
        [
            "Copied pictures",
            "Exported files",
            "Preview",
            "Your colors"
        ]
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
    let once = app.copy_pictures(Some((Paper::Canvas, true)));
    assert_eq!(once.background, Paper::Canvas);
    assert!(once.editable, "one copy can include the editable drawing");
    assert!(!app.appearance.export_editable_drawing);
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

/// Pictures render once per drawing and choice while Settings or the Export
/// tab shows them; close-ups once per drawing, only while Settings does.
#[test]
fn close_ups_render_once_per_drawing_revision() {
    let mut app = app();
    let key = App::preview_key;
    let pictures = |app: &mut App, key: PreviewKey, doc: &Document| {
        let preview = Preview::render(doc, key.1);
        let _ = app.update(Message::Settings(Action::Preview(key, preview)));
    };
    let deliver_tiles = |app: &mut App, doc: &Document| {
        let key = app.preview_key().0;
        let _ = app.update(Message::Settings(Action::CloseUps(key, close_ups(doc))));
    };
    let _ = app.update(Message::Settings(Action::Query(String::new())));
    assert_eq!(app.settings.rendering, None, "hidden: nothing renders");
    assert_eq!(app.settings.rendering_close_ups, None);
    let _ = app.update(Message::Settings(Action::Open(Some(Section::CopyExport))));
    let first = key(&app);
    assert_eq!(app.settings.rendering, Some(first));
    assert_eq!(app.settings.rendering_close_ups, Some(first.0));
    let doc = app.tab.doc.clone();
    pictures(&mut app, first, &doc);
    deliver_tiles(&mut app, &doc);
    assert_eq!(app.settings.rendering, None);
    assert_eq!(app.settings.rendering_close_ups, None);
    let Some((shown, Ok(preview))) = &app.settings.preview else {
        panic!("pictures are kept");
    };
    assert_eq!(*shown, first);
    assert!(preview.sample, "an empty drawing shows the sample");
    let Some((_, Ok(tiles))) = &app.settings.close_ups else {
        panic!("close-ups are kept");
    };
    assert_eq!(tiles.len(), 4);
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
    pictures(&mut app, first, &doc);
    assert_eq!(app.settings.rendering, Some(second));
    let doc = app.tab.doc.clone();
    pictures(&mut app, second, &doc);
    let Some((shown, Ok(preview))) = &app.settings.preview else {
        panic!("pictures are kept");
    };
    assert_eq!(*shown, second);
    assert!(!preview.sample);
    // The older job finishing last neither replaces the pictures nor starts
    // another render.
    pictures(&mut app, first, &Document::default());
    let Some((shown, Ok(preview))) = &app.settings.preview else {
        panic!("pictures are kept");
    };
    assert_eq!(*shown, second);
    assert!(!preview.sample);
    assert_eq!(app.settings.rendering, None);
    deliver_tiles(&mut app, &doc);
    // Labels from the chemistry engine change the drawing without a revision.
    app.tab.label_generation += 1;
    let _ = app.update(Message::Settings(Action::Query(String::new())));
    let third = key(&app);
    assert_eq!(app.settings.rendering, Some(third));
    assert_eq!(app.settings.rendering_close_ups, Some(third.0));
    pictures(&mut app, third, &doc);
    deliver_tiles(&mut app, &doc);
    // Another background renders the pictures again, not the close-ups.
    let _ = app.update(Message::Settings(Action::CopyBackground(Background::White)));
    let white = key(&app);
    assert_eq!(white.0, third.0);
    assert_eq!(white.1[0], (Paper::Color([255; 3]), None));
    assert_eq!(app.settings.rendering, Some(white));
    assert_eq!(app.settings.rendering_close_ups, None);
    pictures(&mut app, white, &doc);
    // White, then Black, then White again before Black renders: the late
    // Black picture does not replace the White one.
    let _ = app.update(Message::Settings(Action::CopyBackground(Background::Black)));
    let black = key(&app);
    assert_eq!(app.settings.rendering, Some(black));
    let _ = app.update(Message::Settings(Action::CopyBackground(Background::White)));
    assert_eq!(app.settings.rendering, None);
    pictures(&mut app, black, &doc);
    assert_eq!(
        app.settings.preview.as_ref().map(|(key, _)| *key),
        Some(white)
    );
    // Black again renders anew.
    let _ = app.update(Message::Settings(Action::CopyBackground(Background::Black)));
    assert_eq!(app.settings.rendering, Some(black));
    // Results arrive while a dialog takes the keys, and end their render.
    app.tab.revision += 1;
    let _ = app.update(Message::Settings(Action::Query(String::new())));
    let fifth = key(&app);
    assert_eq!(app.settings.rendering_close_ups, Some(fifth.0));
    app.updates.open = true;
    pictures(&mut app, fifth, &doc);
    deliver_tiles(&mut app, &doc);
    assert_eq!(app.settings.rendering, None);
    assert_eq!(app.settings.rendering_close_ups, None);
    assert_eq!(
        app.settings.close_ups.as_ref().map(|(key, _)| *key),
        Some(fifth.0)
    );
    app.updates.open = false;

    // The Export tab shows the exported picture in every format, and on PNG
    // at its resolution. It has no close-ups.
    let _ = app.update(Message::Settings(Action::Close));
    app.tab.revision += 1;
    let _ = app.update(Message::Inspector(super::super::InspectorTab::Export));
    assert_eq!(app.settings.rendering, Some(key(&app)));
    assert_eq!(app.settings.rendering_close_ups, None);
    assert_eq!(key(&app).1[1], (Paper::Canvas, None));
    let _ = app.update(Message::InspectorAction(
        super::super::inspector::Action::Figure(super::super::inspector::FigureFormat::Png),
    ));
    assert_eq!(key(&app).1[1], (Paper::Canvas, Some(1200)));
    assert_eq!(app.settings.rendering, Some(key(&app)));
    let _ = app.update(Message::InspectorAction(
        super::super::inspector::Action::ResolutionMenu(true),
    ));
    assert_eq!(app.settings.rendering_close_ups, None);
}

/// The pictures are the exporter's, on each background: transparent shows
/// through, a color fills the picture, and the drawing is drawn on both.
#[test]
fn pictures_show_the_drawing_on_each_background() {
    let doc = reshiki::rings::Preset::Benzene.document(42., false);
    let preview = Preview::render(
        &doc,
        [
            (Paper::Transparent, None),
            (Paper::Color([30, 42, 68]), Some(150)),
        ],
    )
    .unwrap();
    let pixels = |index: usize| match &preview.pictures[index] {
        image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } => {
            assert!(*width <= 480 && *height <= 280);
            pixels.as_chunks::<4>().0.to_vec()
        }
        _ => panic!("pictures are pixels"),
    };
    let (clear, navy) = (pixels(0), pixels(1));
    assert_eq!(clear[0], [0; 4]);
    assert!(
        clear
            .iter()
            .any(|p| p[3] == 255 && p[..3].iter().all(|&c| c < 64)),
        "black ink"
    );
    assert_eq!(navy[0], [30, 42, 68, 255]);
    assert!(
        navy.iter().any(|p| p[..3].iter().all(|&c| c > 192)),
        "white ink"
    );
}

/// An empty drawing shows a sample on its own canvas.
#[test]
fn the_sample_keeps_the_drawings_canvas() {
    let doc = Document {
        canvas_theme: CanvasTheme::Dark,
        ..Document::default()
    };
    let preview = Preview::render(&doc, [(Paper::Canvas, None); 2]).unwrap();
    assert!(preview.sample);
    let image::Handle::Rgba { pixels, .. } = &preview.pictures[1] else {
        panic!("pictures are pixels");
    };
    let [r, g, b] = CanvasTheme::Dark.background();
    assert_eq!(pixels[..4], [r, g, b, 255]);
}

#[test]
fn close_ups_show_the_same_square_at_every_resolution_and_describe_the_whole_drawing() {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", reshiki::document::Point::default());
    let oxygen = doc.add_atom("O", reshiki::document::Point::new(42., 0.));
    doc.add_bond(carbon, oxygen, 1, "plain");
    let sides: Vec<_> = close_ups(&doc)
        .unwrap()
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
    let preview = Preview::render(&doc, [(Paper::Canvas, None); 2]).unwrap();
    assert!(!preview.sample);
    let (width, height, _) = reshiki::export::png_file_size(preview.size, 1200).unwrap();
    let cm = |css: f32| css / 96. * 2.54;
    assert_eq!(
        preview.describe(Dpi::ALL[3]),
        format!(
            "1200 dpi · {:.1} × {:.1} cm · {width} × {height} px",
            cm(preview.size.0),
            cm(preview.size.1)
        )
    );
    // A 30 × 15 cm scheme is written at 600 dpi, as its label says.
    let wide = Preview {
        size: (30. / 2.54 * 96., 15. / 2.54 * 96.),
        ..preview
    };
    assert_eq!(wide.pixels(Dpi::ALL[3]), "7087 × 3544 px at 600 dpi");
    assert_eq!(close_ups(&Document::default()).unwrap().len(), 4);
}

/// The Export tab's Include editable drawing starts at the default each time
/// the tab opens or the default changes, and changes only its own exports.
#[test]
fn the_export_tab_overrides_the_editable_drawing_default_for_its_exports() {
    use crate::app::{InspectorTab, inspector::Action as Inspector};
    let mut app = app();
    assert!(app.appearance.export_editable_drawing);
    let _ = app.update(Message::Inspector(InspectorTab::Export));
    assert!(app.export_editable());
    let _ = app.update(Message::InspectorAction(Inspector::ExportEditable(Some(
        false,
    ))));
    assert!(!app.export_editable());
    assert!(app.appearance.export_editable_drawing, "the default stays");
    // Restore default.
    let _ = app.update(Message::InspectorAction(Inspector::ExportEditable(None)));
    assert!(app.export_editable());
    // Other exports, and the tab opened again, follow the default.
    let _ = app.update(Message::InspectorAction(Inspector::ExportEditable(Some(
        false,
    ))));
    let _ = app.update(Message::Inspector(InspectorTab::Properties));
    assert!(app.export_editable());
    let _ = app.update(Message::Inspector(InspectorTab::Export));
    assert!(app.export_editable());
    // A new default resets the tab's choice, in every drawing.
    let _ = app.update(Message::InspectorAction(Inspector::ExportEditable(Some(
        true,
    ))));
    let before = app.tab.doc.clone();
    app.tab.doc.add_atom("C", Default::default());
    app.changed(before);
    let _ = app.update(Message::New);
    let _ = app.update(Message::View(
        super::super::view_settings::Action::ExportEditableDrawing(false),
    ));
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Cycle(true)));
    assert_eq!(app.tabs.background.len(), 1);
    assert_eq!(app.inspector_tab, InspectorTab::Export);
    assert!(!app.export_editable());
}

/// Restore defaults puts back only its own section's preferences, keeps the
/// saved colors, and has nothing to do at the defaults.
#[test]
fn restore_defaults_resets_only_its_section_and_keeps_saved_colors() {
    let mut app = app();
    for section in [
        Section::Appearance,
        Section::Drawing,
        Section::CopyExport,
        Section::Names,
    ] {
        assert!(app.at_defaults(section), "{section:?}");
    }
    let a = &mut app.appearance;
    a.copy_background = Background::White;
    a.export_background = Background::Black;
    a.export_editable_drawing = false;
    a.export_svg_live_text = true;
    a.copy_dpi = Dpi::ALL[0];
    a.export_dpi = Dpi::ALL[1];
    a.background_colors = vec![[30, 42, 68]];
    a.custom_background = [30, 42, 68];
    a.smart_guides = false;
    app.export_tab_editable = Some(true);
    assert!(!app.at_defaults(Section::CopyExport));
    let _ = app.update(Message::Settings(Action::Restore(Section::CopyExport)));
    let (a, d) = (&app.appearance, crate::appearance::Settings::default());
    assert_eq!(
        (
            a.copy_background,
            a.export_background,
            a.export_editable_drawing,
            a.export_svg_live_text,
            a.copy_dpi,
            a.export_dpi,
        ),
        (
            Background::Transparent,
            Background::Canvas,
            true,
            false,
            d.copy_dpi,
            d.export_dpi,
        )
    );
    assert_eq!((a.copy_dpi.get(), a.export_dpi.get()), (300, 1200));
    assert_eq!(a.background_colors, [[30, 42, 68]], "saved colors stay");
    assert_eq!(a.custom_background, [30, 42, 68]);
    assert!(!a.smart_guides, "other sections stay");
    assert_eq!(app.export_tab_editable, None);
    assert_eq!(app.status, "Copy & export restored to defaults");
    assert!(app.at_defaults(Section::CopyExport));
    assert!(!app.at_defaults(Section::Drawing));
    let _ = app.update(Message::Settings(Action::Restore(Section::Drawing)));
    assert!(app.appearance.smart_guides);
    assert!(app.at_defaults(Section::Drawing));
}

/// Restore defaults in Assistant and Updates puts back their own preferences.
#[test]
fn restore_defaults_resets_the_assistant_and_update_preferences() {
    use super::super::{assistant::Action as Assistant, updates::Action as Updates};
    use reshiki::updates::{Channel, Preferences};
    let mut app = app();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = app.update(Message::Assistant(Assistant::Model(Some("gpt".into()))));
        let _ = app.update(Message::Assistant(Assistant::AutoApply(true)));
        assert!(!app.at_defaults(Section::Assistant));
        let _ = app.update(Message::Settings(Action::Restore(Section::Assistant)));
        let choices = app.assistant_choices();
        assert_eq!((choices.model, choices.auto_apply), (None, false));
        assert!(app.at_defaults(Section::Assistant));
    }
    let _ = app.update(Message::Updates(Updates::Channel(Channel::Nightly)));
    let _ = app.update(Message::Updates(Updates::Automatic(false)));
    let _ = app.update(Message::Updates(Updates::Saved(Ok(()))));
    assert!(!app.at_defaults(Section::Updates));
    let _ = app.update(Message::Settings(Action::Restore(Section::Updates)));
    assert_eq!(app.update_preferences(), Preferences::default());
    assert!(app.at_defaults(Section::Updates));
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

/// Screen readers hear what each picture tile shows; it takes no focus.
#[tokio::test]
#[ignore = "Opt-in real App widget tree check"]
async fn picture_tiles_are_named_for_screen_readers() {
    use iced::advanced::renderer::Headless;
    use reshiki::accessibility::Role;
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let named = |app: &App, renderer: &mut iced::Renderer| {
        rendered(app, renderer)
            .1
            .into_iter()
            .filter(|node| node.role == Role::Image)
            .map(|node| (node.id, node.name, node.enabled))
            .collect::<Vec<_>>()
    };
    let mut app = app();
    evidence_app(&mut app);
    open(&mut app, Section::CopyExport);
    show_close_ups(&mut app);
    assert_eq!(
        named(&app, &mut renderer),
        [
            (
                "settings-copy-picture".into(),
                "Preview of the copied picture".into(),
                true
            ),
            (
                "settings-export-picture".into(),
                "Preview of the exported picture".into(),
                true
            ),
        ]
    );
    let _ = app.update(Message::Settings(Action::Close));
    let _ = app.update(Message::Inspector(crate::app::InspectorTab::Export));
    show_close_ups(&mut app);
    assert_eq!(
        named(&app, &mut renderer),
        [(
            "export-picture".into(),
            "Preview of the exported picture".into(),
            true
        )]
    );
}

/// Restore defaults is disabled at the defaults; the Export tab offers
/// Restore default only while it differs; Copy image on names a choice that
/// is not the default.
#[tokio::test]
#[ignore = "Opt-in real App widget tree check"]
async fn restore_buttons_and_the_not_default_hint_show_only_when_they_differ() {
    use crate::app::{InspectorTab, context_menu, inspector::Action as Inspector};
    use iced::advanced::renderer::Headless;
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let node = |app: &App, renderer: &mut iced::Renderer, id: &str| {
        rendered(app, renderer)
            .1
            .into_iter()
            .find(|node| node.id == id)
    };
    let mut app = app();
    open(&mut app, Section::CopyExport);
    let restore = "settings-restore-copyexport";
    assert!(!node(&app, &mut renderer, restore).unwrap().enabled);
    let _ = app.update(Message::Settings(Action::CopyDpi(Dpi::ALL[0])));
    assert!(node(&app, &mut renderer, restore).unwrap().enabled);
    let _ = app.update(Message::Settings(Action::Close));

    let _ = app.update(Message::Inspector(InspectorTab::Export));
    let (texts, _) = rendered(&app, &mut renderer);
    assert!(
        texts
            .iter()
            .any(|t| t == "Default: on · Settings → Copy & export")
    );
    assert!(node(&app, &mut renderer, "export-editable-restore").is_none());
    let _ = app.update(Message::InspectorAction(Inspector::ExportEditable(Some(
        false,
    ))));
    assert!(node(&app, &mut renderer, "export-editable-restore").is_some());
    // The Export button carries the tab's choice; other exports send
    // Message::Export, which follows the default.
    {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            iced::Size::new(1280., 820.),
            iced_runtime::user_interface::Cache::new(),
            &mut renderer,
        );
        let mut activate = reshiki::accessibility::Activate::<Message>::new("export-figure");
        ui.operate(
            &renderer,
            &mut iced::advanced::widget::operation::black_box(&mut activate),
        );
        assert!(matches!(
            activate.message(),
            Some(Message::ExportFigure("pdf", false))
        ));
    }

    app.context_menu = Some(context_menu::State::new(
        iced::Point::new(36., 24.),
        context_menu::Page::Main,
    ));
    let _ = app.context_action(context_menu::Action::Page(context_menu::Page::CopyImageOn));
    let editable = |app: &App, renderer: &mut iced::Renderer| {
        rendered(app, renderer)
            .1
            .into_iter()
            .find(|node| node.name.starts_with("Editable drawing"))
            .map(|node| (node.name, node.checked))
    };
    assert_eq!(
        editable(&app, &mut renderer),
        Some(("Editable drawing".into(), Some(true)))
    );
    let _ = app.context_action(context_menu::Action::Editable);
    assert_eq!(
        editable(&app, &mut renderer),
        Some(("Editable drawing (not default)".into(), Some(false)))
    );
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

/// Delivers the pictures and close-ups of the drawing as it is now.
fn show_close_ups(app: &mut App) {
    let key = app.preview_key();
    let preview = Preview::render(&app.tab.doc, key.1);
    let tiles = close_ups(&app.tab.doc);
    let _ = app.update(Message::Settings(Action::Preview(key, preview)));
    let _ = app.update(Message::Settings(Action::CloseUps(key.0, tiles)));
}

fn open(app: &mut App, section: Section) {
    let _ = app.update(Message::Settings(Action::Open(Some(section))));
}

/// Opt-in screenshots of Settings, the Export tab and Copy image on, in the
/// light and dark interface. Set RESHIKI_UI_QA_SCALE=2 for 2× images.
#[tokio::test]
#[ignore = "Opt-in GPU snapshots; set RESHIKI_UI_QA_DIR and RESHIKI_DATA_DIR"]
async fn manual_settings_snapshots() {
    use iced::advanced::{renderer::Headless, widget::Tree};
    type Setup = fn(&mut App);
    let states: [(&str, Setup); 24] = [
        ("toolbar", |_| {}),
        ("settings-appearance", |app| open(app, Section::Appearance)),
        ("settings-drawing", |app| open(app, Section::Drawing)),
        ("settings-copy-export", |app| {
            app.appearance.copy_background = Background::Custom;
            open(app, Section::CopyExport);
            show_close_ups(app);
        }),
        ("settings-copy-export-transparent", |app| {
            open(app, Section::CopyExport);
            show_close_ups(app);
        }),
        ("settings-copy-export-white-black", |app| {
            app.appearance.copy_background = Background::White;
            app.appearance.export_background = Background::Black;
            open(app, Section::CopyExport);
            show_close_ups(app);
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
            show_close_ups(app);
        }),
        ("export-tab-transparent", |app| {
            app.appearance.export_background = Background::Transparent;
            let _ = app.update(Message::Inspector(crate::app::InspectorTab::Export));
            show_close_ups(app);
        }),
        ("export-tab-white", |app| {
            app.appearance.export_background = Background::White;
            let _ = app.update(Message::Inspector(crate::app::InspectorTab::Export));
            show_close_ups(app);
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
            show_close_ups(app);
        }),
        ("export-tab-custom", |app| {
            app.appearance.export_background = Background::Custom;
            let _ = app.update(Message::Inspector(crate::app::InspectorTab::Export));
            show_close_ups(app);
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
        ("copy-image-on-not-default", |app| {
            app.context_menu = Some(crate::app::context_menu::State::new(
                iced::Point::new(36., 24.),
                crate::app::context_menu::Page::Main,
            ));
            let _ = app.context_action(crate::app::context_menu::Action::Page(
                crate::app::context_menu::Page::CopyImageOn,
            ));
            let _ = app.context_action(crate::app::context_menu::Action::Editable);
        }),
        ("export-tab-editable-override", |app| {
            let _ = app.update(Message::Inspector(crate::app::InspectorTab::Export));
            let _ = app.update(Message::InspectorAction(
                crate::app::inspector::Action::ExportEditable(Some(false)),
            ));
            show_close_ups(app);
        }),
        ("settings-copy-export-changed", |app| {
            app.appearance.copy_dpi = Dpi::ALL[0];
            open(app, Section::CopyExport);
            show_close_ups(app);
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
