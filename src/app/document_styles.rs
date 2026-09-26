use super::{
    App, InspectorTab, Message,
    icons::{Glyph, Icon},
};
use crate::canvas::{DrawingThumbnail, layered::canvas};
use iced::widget::{
    Space, button, checkbox, column, combo_box, container, row, scrollable, text, tooltip,
};
use iced::{Alignment, Element, Length, Task};
use reshiki::{document_styles::Preset, style::DrawingStyle};

fn command(label: &str) -> iced::widget::Button<'_, Message> {
    button(text(label).size(12)).padding([7, 9])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_tile_contrast_covers_all_themes_modes_and_states() {
        use iced::{Background, widget::button};
        use reshiki::{
            canvas_theme::{CanvasTheme, ColorTheme},
            color_contrast::{OUTLINE_TARGET, TEXT_TARGET, contrast},
        };
        let (mut app, _) = super::super::App::new();
        let mut pairs = 0;
        let mut text_min = 21_f64;
        let mut outline_min = 21_f64;
        for canvas in CanvasTheme::ALL {
            app.doc.canvas_theme = canvas;
            for mode in crate::appearance::Mode::ALL {
                app.appearance.mode = mode;
                let theme = app.theme();
                let palettes = ColorTheme::ALL
                    .into_iter()
                    .map(|p| (p.to_string(), p, None))
                    .chain(
                        reshiki::theme_files::bundled()
                            .unwrap()
                            .into_iter()
                            .map(|t| (t.name.clone(), t.base, Some(t))),
                    );
                for (palette, base, custom) in palettes {
                    base.apply(&mut app.doc);
                    if let Some(custom) = custom {
                        custom.apply(&mut app.doc).unwrap();
                    }
                    for element in reshiki::editing::ELEMENTS {
                        for selected in [false, true] {
                            for state in [
                                button::Status::Active,
                                button::Status::Hovered,
                                button::Status::Pressed,
                            ] {
                                let style = super::super::workspace::element_control(
                                    selected, &app.doc, element,
                                )(&theme, state);
                                let outer = theme.palette().background;
                                let inner = match style.background {
                                    Some(Background::Color(c)) if c.a == 1. => c,
                                    _ => outer,
                                };
                                let ratio = contrast(
                                    crate::appearance::rgb(style.text_color),
                                    crate::appearance::rgb(inner),
                                );
                                text_min = text_min.min(ratio);
                                pairs += 1;
                                assert!(
                                    ratio >= TEXT_TARGET,
                                    "{palette}/{canvas}/{mode}/{element}/{selected}/{state:?}: {ratio}"
                                );
                                if selected {
                                    for background in [inner, outer] {
                                        let ratio = contrast(
                                            crate::appearance::rgb(style.border.color),
                                            crate::appearance::rgb(background),
                                        );
                                        outline_min = outline_min.min(ratio);
                                        assert!(
                                            ratio >= OUTLINE_TARGET,
                                            "outline {palette}/{canvas}/{mode}/{element}: {ratio}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        eprintln!(
            "Tiles: {pairs} text pairs, minimum {text_min:.4}:1; selected outline minimum {outline_min:.4}:1"
        );
    }

    #[test]
    fn custom_keeps_current_dimensions_and_can_be_saved() {
        let (mut app, _) = App::new();
        let _ = app.update(Message::DrawingStyle(Action::Open));
        let _ = app.update(Message::DrawingStyle(Action::Preset(Preset::Nature)));
        let _ = app.update(Message::DrawingStyle(Action::Custom));
        let custom = app.styles.editor.as_ref().unwrap().candidate().unwrap();
        let mut expected = Preset::Nature.style();
        expected.name = "Custom".into();
        assert_eq!(custom, expected);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("custom.reshiki-style");
        std::fs::write(&path, serde_json::to_vec(&custom).unwrap()).unwrap();
        assert_eq!(reshiki::document_styles::load(&path).unwrap(), custom);
        let _ = app.update(Message::DrawingStyle(Action::Apply));
        assert_eq!(app.doc.drawing_style, custom);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc.drawing_style, DrawingStyle::default());
    }

    #[test]
    fn interface_mode_never_changes_canvas_or_exports() {
        use reshiki::canvas_theme::CanvasTheme;
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        for (canvas, palette) in CanvasTheme::ALL.into_iter().flat_map(|canvas| {
            reshiki::canvas_theme::ColorTheme::ALL.map(|palette| (canvas, palette))
        }) {
            app.doc.canvas_theme = canvas;
            app.doc.color_theme = palette;
            for preset in Preset::ALL {
                app.doc.drawing_style = preset.style();
                let _ = app.update(Message::DrawingStyle(Action::Open));
                let before = app.doc.clone();
                let saved = app.saved.clone();
                let export = reshiki::export::figure(&app.doc, "svg").unwrap().bytes;
                for mode in crate::appearance::Mode::ALL {
                    let _ = app.update(Message::Appearance(mode));
                    assert_eq!(
                        crate::appearance::is_dark(&app.theme()),
                        mode.is_dark(canvas)
                    );
                    assert_eq!(app.doc, before);
                    assert_eq!(app.saved, saved);
                    assert_eq!(
                        app.styles.editor.as_ref().unwrap().candidate().unwrap(),
                        preset.style()
                    );
                    assert_eq!(
                        reshiki::export::figure(&app.doc, "svg").unwrap().bytes,
                        export
                    );
                }
            }
        }
    }

    #[test]
    fn canvas_colors_and_quick_presets_are_independent_undo_steps() {
        use reshiki::canvas_theme::CanvasTheme;
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        let original = app.doc.clone();
        let _ = app.update(Message::ColorTheme(
            reshiki::canvas_theme::ColorTheme::Presentation,
        ));
        let colored = app.doc.clone();
        let _ = app.update(Message::CanvasTheme(CanvasTheme::Dark));
        assert_eq!(app.doc.drawing_style, original.drawing_style);
        assert_eq!(app.doc.atoms, original.atoms);
        let dark = app.doc.clone();
        let _ = app.update(Message::QuickDrawingStyle(Choice::Journal(Preset::Nature)));
        assert_eq!(app.doc.canvas_theme, CanvasTheme::Dark);
        assert_eq!(app.doc.drawing_style, Preset::Nature.style());
        assert!(app.styles.editor.is_none());
        assert_ne!(
            app.doc.atoms, dark.atoms,
            "quick switching also scales bond geometry"
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, dark);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, colored);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, colored);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, dark);
        let _ = app.update(Message::QuickDrawingStyle(Choice::Details));
        assert!(app.styles.editor.is_some());
        assert_eq!(app.doc, dark);
    }

    #[test]
    fn every_preset_survives_editor_fields_without_becoming_custom() {
        for preset in Preset::ALL {
            let style = preset.style();
            let mut editor = Editor::new(&DrawingStyle::default(), 0);
            editor.set(&style);
            assert_eq!(editor.candidate().unwrap(), style, "{preset}");
        }
    }

    #[test]
    fn preview_cancel_apply_and_history_preserve_the_document() {
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        app.busy = false;
        app.history = Default::default();
        let before = app.doc.clone();
        app.saved = before.clone();
        let send = |app: &mut App, action| {
            let _ = app.update(Message::DrawingStyle(action));
        };
        send(&mut app, Action::Open);
        send(&mut app, Action::Preset(Preset::Presentation));
        assert_eq!(app.doc, before);
        send(&mut app, Action::Cancel);
        assert_eq!(app.doc, before);
        send(&mut app, Action::Open);
        send(&mut app, Action::Preset(Preset::Presentation));
        send(&mut app, Action::Input(Field::Line, "NaN".into()));
        send(&mut app, Action::Apply);
        assert_eq!(app.doc, before);
        assert!(app.error);
        send(&mut app, Action::Preset(Preset::Presentation));
        send(&mut app, Action::Apply);
        assert!(!app.error);
        let after = app.doc.clone();
        assert!(app.dirty(), "A style-only edit must be saved");
        assert_eq!(app.drawing_length_input, "24");
        assert_eq!(app.caption_format.style.size_pt, 16.);
        assert_eq!(app.doc.atoms, before.atoms);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        assert_eq!(app.drawing_length_input, "14.4");
        assert!(!app.dirty());
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, after);
        let _ = app.update(Message::ArrowStyle(reshiki::arrows::Preset::Fishhook));
        assert_eq!(app.arrows.style.width_pt, 1.);
        assert_eq!(app.arrows.numbers.first().map(String::as_str), Some("1"));
        send(&mut app, Action::Open);
        app.file_epoch = app.file_epoch.wrapping_add(1);
        send(&mut app, Action::Preset(Preset::Jacs));
        send(&mut app, Action::Apply);
        assert_eq!(app.doc, after);
        assert!(app.error);
        let _ = app.perform(super::super::Pending::New);
        assert!(app.doc.drawing_style.is_default());
        assert_eq!(app.caption_format.style.size_pt, 10.);
        assert_eq!(app.drawing_length_input, "14.4");
    }

    #[tokio::test]
    #[ignore = "Manual GPU snapshots without opening or controlling desktop windows"]
    async fn drawing_style_headless_snapshot() {
        use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
        let (mut app, _) = App::new();
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        app.busy = false;
        app.status = "Ready".into();
        let _ = app.update(Message::DrawingStyle(Action::Open));
        let _ = app.update(Message::DrawingStyle(Action::Preset(Preset::Nature)));
        let carbon = app.doc.atoms[0].id;
        let p = app.doc.atoms[0].position;
        let oxygen = app.doc.add_atom("O", p.offset(42., 0.));
        app.doc.add_bond(carbon, oxygen, 2, "plain");
        app.doc.atoms[3].element = "N".into();
        let directory = std::env::temp_dir().join("reshiki-document-style-qa");
        std::fs::create_dir_all(&directory).unwrap();
        for (name, width, height, dark, mode) in [
            (
                "desktop",
                1280,
                820,
                false,
                crate::appearance::Mode::MatchCanvas,
            ),
            (
                "compact",
                1040,
                680,
                false,
                crate::appearance::Mode::MatchCanvas,
            ),
            (
                "dark-desktop",
                1280,
                820,
                true,
                crate::appearance::Mode::MatchCanvas,
            ),
            (
                "dark-compact",
                1040,
                680,
                true,
                crate::appearance::Mode::MatchCanvas,
            ),
            (
                "dark-canvas-light-ui",
                1280,
                820,
                true,
                crate::appearance::Mode::Light,
            ),
            (
                "light-canvas-dark-ui",
                1280,
                820,
                false,
                crate::appearance::Mode::Dark,
            ),
            (
                "presentation-light",
                1280,
                820,
                false,
                crate::appearance::Mode::Light,
            ),
            (
                "presentation-dark",
                1280,
                820,
                true,
                crate::appearance::Mode::Dark,
            ),
            (
                "pastel-light",
                1280,
                820,
                false,
                crate::appearance::Mode::Dark,
            ),
            (
                "pastel-dark",
                1280,
                820,
                true,
                crate::appearance::Mode::Light,
            ),
            (
                "selected-compact",
                1040,
                680,
                true,
                crate::appearance::Mode::MatchCanvas,
            ),
        ] {
            app.doc.canvas_theme = if dark {
                reshiki::canvas_theme::CanvasTheme::Dark
            } else {
                reshiki::canvas_theme::CanvasTheme::Light
            };
            app.doc.color_theme = if name.starts_with("presentation") {
                reshiki::canvas_theme::ColorTheme::Presentation
            } else if name.starts_with("pastel") {
                reshiki::canvas_theme::ColorTheme::Pastel
            } else {
                reshiki::canvas_theme::ColorTheme::Publication
            };
            app.appearance.mode = mode;
            app.sync_color_input();
            app.selected = if name == "selected-compact" {
                app.doc.all_ids()
            } else {
                vec![]
            };
            app.grid = dark;
            app.guides.rulers = dark;
            app.view_open = dark;
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
            let cursor = mouse::Cursor::Available(iced::Point::new(
                width as f32 - 36.,
                height as f32 - 73. - if dark { 44. } else { 0. },
            ));
            for event in [
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ] {
                view.as_widget_mut().update(
                    &mut tree,
                    &event,
                    iced::advanced::Layout::new(&node),
                    cursor,
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut iced::advanced::Shell::new(&mut messages),
                    &iced::Rectangle::with_size(size),
                );
            }
            assert!(
                messages
                    .iter()
                    .any(|m| matches!(m, Message::DrawingStyle(Action::Apply))),
                "Save icon stays visible and clickable at {width}×{height}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Bond,
    FontSize,
    Line,
    Bold,
    Margin,
    Hash,
    Spacing,
}
impl Field {
    fn label(self) -> &'static str {
        match self {
            Self::Bond => "Bond length (pt)",
            Self::FontSize => "Label size (pt)",
            Self::Line => "Line width (pt)",
            Self::Bold => "Bold width (pt)",
            Self::Margin => "Label margin (pt)",
            Self::Hash => "Hash spacing (pt)",
            Self::Spacing => "Bond spacing (%)",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Journal(Preset),
    Custom,
    Details,
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Journal(preset) => preset.fmt(f),
            Self::Custom => f.write_str("Custom"),
            Self::Details => f.write_str("Manage styles…"),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFormat {
    Native,
    Cds,
}
impl SaveFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Native => "reshiki-style",
            Self::Cds => "cds",
        }
    }
}
impl std::fmt::Display for SaveFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Native => "ReShiki style",
            Self::Cds => "ChemDraw CDS",
        })
    }
}
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Cancel,
    Apply,
    Preset(Preset),
    Custom,
    Source(Preset),
    Name(String),
    Font(String),
    Input(Field, String),
    Advanced(bool),
    Matching(bool),
    Scale(bool),
    Load,
    Save(SaveFormat),
    ExportMenu(bool),
    Loaded(u64, u64, Result<Option<DrawingStyle>, String>),
    Saved(Result<bool, String>),
}
#[derive(Default)]
pub struct State {
    pub editor: Option<Editor>,
    serial: u64,
}
pub struct Editor {
    font_options: iced::widget::combo_box::State<String>,
    original: DrawingStyle,
    epoch: u64,
    name: String,
    font: String,
    inputs: Vec<(Field, String)>,
    advanced: bool,
    export_menu: bool,
    matching: bool,
    scale: bool,
}
impl Editor {
    fn new(style: &DrawingStyle, epoch: u64) -> Self {
        let mut editor = Self {
            font_options: iced::widget::combo_box::State::new(
                reshiki::style::font_families()
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            ),
            original: style.clone(),
            epoch,
            name: String::new(),
            font: String::new(),
            inputs: vec![],
            advanced: false,
            export_menu: false,
            matching: true,
            scale: false,
        };
        editor.set(style);
        editor
    }
    fn set(&mut self, style: &DrawingStyle) {
        self.name = style.name.clone();
        self.font = style.font_family.clone();
        self.inputs = [
            (Field::FontSize, style.font_size_pt),
            (Field::Bond, style.bond_length_pt),
            (Field::Line, style.line_width_pt),
            (Field::Bold, style.bold_width_pt),
            (Field::Margin, style.margin_width_pt),
            (Field::Hash, style.hash_spacing_pt),
            (Field::Spacing, style.bond_spacing_ratio * 100.),
        ]
        .into_iter()
        .map(|(field, n)| (field, n.to_string()))
        .collect();
    }
    fn candidate(&self) -> Result<DrawingStyle, String> {
        let mut style = self.original.clone();
        style.name = self.name.trim().into();
        style.font_family = self.font.trim().into();
        for (field, input) in &self.inputs {
            let value = input
                .trim()
                .parse::<f32>()
                .map_err(|_| format!("Enter a number for {}.", field.label().to_lowercase()))?;
            match field {
                Field::Bond => style.set_bond_length(value),
                Field::FontSize => style.font_size_pt = value,
                Field::Line => style.line_width_pt = value,
                Field::Bold => style.bold_width_pt = value,
                Field::Margin => style.margin_width_pt = value,
                Field::Hash => style.hash_spacing_pt = value,
                Field::Spacing => style.bond_spacing_ratio = value / 100.,
            }
        }
        style.validate()?;
        Ok(style)
    }
}

impl App {
    pub(super) fn quick_drawing_style(&mut self, choice: Choice) -> Task<Message> {
        match choice {
            Choice::Details | Choice::Custom => {
                let task = self.drawing_style_action(Action::Open);
                if choice == Choice::Custom
                    && let Some(editor) = &mut self.styles.editor
                {
                    editor.name = "Custom".into();
                }
                task
            }
            Choice::Journal(preset) => {
                self.cancel_join();
                if self.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                match reshiki::document_styles::apply(&self.doc, preset.style(), true, true) {
                    Ok(doc) => {
                        let before = self.doc.clone();
                        self.doc = doc;
                        self.changed(before);
                        self.styles.editor = None;
                        if self.inspector_tab == InspectorTab::DrawingStyle {
                            self.inspector_tab = InspectorTab::Properties;
                        }
                        if !self.error {
                            self.status = format!(
                                "{preset} applied · Undo restores the previous style and layout"
                            );
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
                Task::none()
            }
        }
    }

    pub(super) fn sync_drawing_defaults(&mut self) {
        let style = &self.doc.drawing_style;
        self.bond_drawing.length = style.bond_length_world;
        self.drawing_length_input = style.bond_length_pt.to_string();
        self.caption_format = reshiki::typography::TextFormat {
            style: style.text_style(),
            ..Default::default()
        };
        self.caption_target = None;
        self.graphic_style.width_pt = style.line_width_pt;
        self.graphic_width_input = style.line_width_pt.to_string();
        self.arrows.style.width_pt = style.line_width_pt;
        self.arrows.refresh_inputs();
        self.sync_style_inputs();
    }

    pub(super) fn drawing_style_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Source(preset) => {
                if let Some(url) = preset.source_url()
                    && let Err(error) = open::that(url)
                {
                    self.status = format!("Could not open publisher instructions: {error}");
                    self.error = true;
                }
            }
            Action::Open => {
                self.cancel_join();
                if self.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                self.styles.serial = self.styles.serial.wrapping_add(1);
                self.styles.editor = Some(Editor::new(&self.doc.drawing_style, self.file_epoch));
                self.inspector_tab = InspectorTab::DrawingStyle;
                self.inspector_open = true;
                self.palette = None;
            }
            Action::Cancel => {
                self.styles.editor = None;
                self.inspector_tab = InspectorTab::Properties;
            }
            Action::Apply => {
                let Some(editor) = &self.styles.editor else {
                    return Task::none();
                };
                let result = if editor.epoch != self.file_epoch
                    || editor.original != self.doc.drawing_style
                {
                    Err("The document style changed. Reopen Drawing style before applying.".into())
                } else {
                    editor.candidate().and_then(|style| {
                        reshiki::document_styles::apply(
                            &self.doc,
                            style,
                            editor.matching,
                            editor.scale,
                        )
                    })
                };
                match result {
                    Ok(doc) => {
                        let before = self.doc.clone();
                        self.doc = doc;
                        self.changed(before);
                        if !self.error {
                            self.styles.editor = None;
                            self.inspector_tab = InspectorTab::Properties;
                            self.status = "Drawing style applied · Undo restores the previous style and layout".into();
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::Load => {
                let serial = self.styles.serial;
                let epoch = self.file_epoch;
                return Task::perform(
                    async {
                        let Some(file) = rfd::AsyncFileDialog::new()
                            .set_title("Load drawing style")
                            .add_filter(
                                "Drawing styles and ChemDraw stationery",
                                &[
                                    "reshiki-style",
                                    "moruno-style",
                                    "json",
                                    "cds",
                                    "cdx",
                                    "cdxml",
                                ],
                            )
                            .pick_file()
                            .await
                        else {
                            return Ok(None);
                        };
                        let path = file.path().to_path_buf();
                        tokio::task::spawn_blocking(move || reshiki::document_styles::load(&path))
                            .await
                            .map_err(|e| e.to_string())?
                            .map(Some)
                    },
                    move |result| Message::DrawingStyle(Action::Loaded(serial, epoch, result)),
                );
            }
            Action::Loaded(serial, epoch, result) => {
                if serial != self.styles.serial
                    || epoch != self.file_epoch
                    || self.styles.editor.is_none()
                {
                    return Task::none();
                }
                match result {
                    Ok(Some(style)) => {
                        if let Some(editor) = &mut self.styles.editor {
                            editor.set(&style);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::Save(format) => {
                let Some(editor) = &mut self.styles.editor else {
                    return Task::none();
                };
                editor.export_menu = false;
                let result = editor.candidate();
                match result {
                    Ok(style) => {
                        return Task::perform(
                            async move {
                                let Some(path) = super::files::save_path(
                                    "Save drawing style",
                                    &format!("Drawing.{}", format.extension()),
                                    format.extension(),
                                )
                                .await
                                else {
                                    return Ok(false);
                                };
                                tokio::task::spawn_blocking(move || {
                                    reshiki::document_styles::save(&path, &style)
                                })
                                .await
                                .map_err(|e| e.to_string())??;
                                Ok(true)
                            },
                            |result| Message::DrawingStyle(Action::Saved(result)),
                        );
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::Saved(result) => match result {
                Ok(true) => {
                    self.status = "Drawing style saved".into();
                    self.error = false;
                }
                Ok(false) => {}
                Err(error) => {
                    self.status = error;
                    self.error = true;
                }
            },
            action => {
                if let Some(editor) = &mut self.styles.editor {
                    match action {
                        Action::Preset(preset) => editor.set(&preset.style()),
                        Action::Custom => editor.name = "Custom".into(),
                        Action::Name(name) => editor.name = name,
                        Action::Font(font) => {
                            editor.font = font;
                            if Preset::ALL.iter().any(|p| editor.name == p.to_string())
                                || editor.name == "Presentation"
                            {
                                editor.name = "Custom".into();
                            }
                        }
                        Action::Input(field, value) => {
                            if let Some((_, input)) =
                                editor.inputs.iter_mut().find(|(f, _)| *f == field)
                            {
                                *input = value;
                            }
                            if Preset::ALL.iter().any(|p| editor.name == p.to_string())
                                || editor.name == "Presentation"
                            {
                                editor.name = "Custom".into();
                            }
                        }
                        Action::Advanced(value) => editor.advanced = value,
                        Action::ExportMenu(value) => editor.export_menu = value,
                        Action::Matching(value) => editor.matching = value,
                        Action::Scale(value) => editor.scale = value,
                        _ => {}
                    }
                }
            }
        }
        Task::none()
    }

    pub(super) fn drawing_style_panel(&self) -> Element<'_, Message> {
        let action = Message::DrawingStyle;
        let Some(editor) = &self.styles.editor else {
            return command("Edit drawing style")
                .on_press(action(Action::Open))
                .into();
        };
        let candidate = editor.candidate();
        let preset = candidate
            .as_ref()
            .ok()
            .and_then(|style| Preset::ALL.into_iter().find(|p| p.style() == *style));
        let mut body = column![
            command("‹ Properties")
                .on_press(action(Action::Cancel))
                .style(button::text),
            text("Drawing style").size(19),
            text("Physical sizes for this document")
                .size(12)
                .style(super::workspace::muted_text),
            crate::appearance::pick_list(
                Preset::ALL
                    .into_iter()
                    .map(Choice::Journal)
                    .collect::<Vec<_>>(),
                Some(preset.map(Choice::Journal).unwrap_or(Choice::Custom)),
                move |choice| action(match choice {
                    Choice::Journal(p) => Action::Preset(p),
                    Choice::Custom => Action::Custom,
                    Choice::Details => Action::Open,
                }),
            )
            .width(Length::Fill)
            .text_size(13),
        ]
        .spacing(10);
        if let Some(preset) = preset {
            body = body
                .push(
                    text(preset.description())
                        .size(12)
                        .style(super::workspace::muted_text),
                )
                .push(
                    command("Publisher instructions ↗")
                        .on_press(action(Action::Source(preset)))
                        .style(button::text),
                );
        } else {
            body = body.push(
                text("Customize the current settings, then save a reusable style file.")
                    .size(12)
                    .style(super::workspace::muted_text),
            );
        }
        if let Ok(style) = &candidate {
            let mut preview =
                reshiki::rings::Preset::Regular.document(style.bond_length_world, false);
            if let Some(atom) = preview.atoms.first() {
                let (id, p) = (atom.id, atom.position);
                let oxygen = preview.add_atom("O", p.offset(0., -style.bond_length_world));
                preview.add_bond(id, oxygen, 2, "plain");
            }
            let ids = preview.all_ids();
            reshiki::editing::transform_about(
                &mut preview,
                &ids,
                reshiki::document::Point::default(),
                1.,
                90.,
            );
            preview.drawing_style = style.clone();
            preview.canvas_theme = self.doc.canvas_theme;
            preview.color_theme = self.doc.color_theme;
            preview.custom_theme = self.doc.custom_theme.clone();
            body = body.push(
                container(
                    canvas(DrawingThumbnail(preview))
                        .height(85)
                        .width(Length::Fill),
                )
                .style(super::workspace::panel),
            );
        }
        body = body.push(
            crate::appearance::text_input("Style name", &editor.name)
                .on_input(move |s| action(Action::Name(s)))
                .size(13)
                .padding(7),
        );
        body = body.push(text("Label font").size(12));
        body = body.push(
            combo_box(
                &editor.font_options,
                "Search fonts…",
                Some(&editor.font),
                move |s| action(Action::Font(s)),
            )
            .input_style(crate::appearance::input_style)
            .menu_style(crate::appearance::dropdown_menu)
            .size(13)
            .padding(7)
            .width(Length::Fill),
        );
        for (field, value) in &editor.inputs {
            if !editor.advanced && !matches!(field, Field::FontSize | Field::Bond | Field::Line) {
                continue;
            }
            let field = *field;
            body = body.push(
                row![
                    text(field.label()).size(12).width(Length::Fill),
                    crate::appearance::text_input("", value)
                        .on_input(move |s| action(Action::Input(field, s)))
                        .size(13)
                        .padding(6)
                        .width(104)
                ]
                .align_y(Alignment::Center)
                .spacing(8),
            );
        }
        body = body.push(
            command(if editor.advanced {
                "▾ Advanced stroke settings"
            } else {
                "▸ Advanced stroke settings"
            })
            .on_press(action(Action::Advanced(!editor.advanced)))
            .style(button::text),
        );
        body = body
            .push(super::workspace::horizontal_line())
            .push(
                checkbox(editor.matching)
                    .label("Update matching text and strokes")
                    .on_toggle(move |v| action(Action::Matching(v)))
                    .text_size(12),
            )
            .push(
                text("Preserve custom fonts, sizes and widths.")
                    .size(11)
                    .style(super::workspace::muted_text),
            )
            .push(
                checkbox(editor.scale)
                    .label("Scale layout with bond length")
                    .on_toggle(move |v| action(Action::Scale(v)))
                    .text_size(12),
            )
            .push(
                text("Off: keep positions. On: scale object geometry; keep paper size.")
                    .size(11)
                    .style(super::workspace::muted_text),
            );
        body = body.push(text("Load .cds, .cdx or .cdxml for label fonts and bond settings. Template artwork and page layout are not imported.")
            .size(11).style(super::workspace::muted_text));
        let mut footer = column![super::workspace::horizontal_line()].spacing(8);
        if let Err(error) = &candidate {
            footer = footer.push(text(error.clone()).size(12).style(
                crate::appearance::text_color(iced::Color::from_rgb8(164, 54, 47)),
            ));
        }
        if editor.export_menu {
            footer = footer.push(
                container(
                    column![
                        command("ReShiki style (.reshiki-style)")
                            .on_press(action(Action::Save(SaveFormat::Native)))
                            .style(crate::appearance::secondary)
                            .width(Length::Fill),
                        command("ChemDraw stationery (.cds)")
                            .on_press(action(Action::Save(SaveFormat::Cds)))
                            .style(crate::appearance::secondary)
                            .width(Length::Fill),
                    ]
                    .spacing(5),
                )
                .padding(5),
            );
        }
        let icon = |glyph, hint, message, enabled, active| {
            super::workspace::hover_hint(
                button(canvas(Glyph(glyph, enabled)).width(24).height(24))
                    .padding(7)
                    .width(40)
                    .height(38)
                    .style(super::workspace::control(active))
                    .on_press_maybe(enabled.then_some(action(message))),
                hint,
                tooltip::Position::Top,
            )
        };
        footer = footer.push(
            row![
                icon(
                    Icon::Import,
                    "Import drawing style…",
                    Action::Load,
                    true,
                    false
                ),
                icon(
                    Icon::Export,
                    "Export as ReShiki style or ChemDraw CDS",
                    Action::ExportMenu(!editor.export_menu),
                    candidate.is_ok(),
                    editor.export_menu
                ),
                Space::new().width(Length::Fill),
                icon(
                    Icon::Save,
                    "Save style to this drawing",
                    Action::Apply,
                    candidate.is_ok(),
                    true
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        container(
            column![
                scrollable(container(body).padding(iced::Padding {
                    right: 12.,
                    ..Default::default()
                }))
                .id("inspector-content")
                .height(Length::Fill),
                footer
            ]
            .spacing(12),
        )
        .padding(16)
        .height(Length::Fill)
        .into()
    }
}
