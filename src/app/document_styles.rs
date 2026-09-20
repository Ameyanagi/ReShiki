use super::{App, InspectorTab, Message};
use crate::canvas::{DrawingThumbnail, layered::canvas};
use iced::widget::{
    button, checkbox, column, combo_box, container, pick_list, row, scrollable, text, text_input,
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
        let _ = app.update(Message::DrawingStyle(Action::Preset(Preset::Presentation)));
        let directory = std::path::Path::new("artifacts/document-style-qa");
        std::fs::create_dir_all(directory).unwrap();
        for (name, width, height) in [("desktop", 1280, 820), ("compact", 1040, 680)] {
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
                iced::Color::WHITE,
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
                width as f32 - 170.,
                height as f32 - 73.,
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
                "Apply stays visible and clickable at {width}×{height}"
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
#[derive(Debug, Clone)]
pub enum Action {
    Open,
    Cancel,
    Apply,
    Preset(Preset),
    Name(String),
    Font(String),
    Input(Field, String),
    Advanced(bool),
    Matching(bool),
    Scale(bool),
    Load,
    Save,
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
                                "ReShiki drawing style",
                                &["reshiki-style", "moruno-style", "json"],
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
            Action::Save => {
                let Some(editor) = &self.styles.editor else {
                    return Task::none();
                };
                let result = editor
                    .candidate()
                    .and_then(|style| serde_json::to_vec_pretty(&style).map_err(|e| e.to_string()));
                match result {
                    Ok(bytes) => {
                        return Task::perform(
                            async move {
                                let Some(file) = rfd::AsyncFileDialog::new()
                                    .set_title("Save drawing style")
                                    .set_file_name("Drawing.reshiki-style")
                                    .add_filter("ReShiki drawing style", &["reshiki-style"])
                                    .save_file()
                                    .await
                                else {
                                    return Ok(false);
                                };
                                let path = file.path().to_path_buf();
                                tokio::task::spawn_blocking(move || {
                                    reshiki::storage::write_atomic(&path, &bytes)
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
                        Action::Name(name) => editor.name = name,
                        Action::Font(font) => {
                            editor.font = font;
                            if editor.name == "JACS / ACS" || editor.name == "Presentation" {
                                editor.name = "Custom".into();
                            }
                        }
                        Action::Input(field, value) => {
                            if let Some((_, input)) =
                                editor.inputs.iter_mut().find(|(f, _)| *f == field)
                            {
                                *input = value;
                            }
                            if editor.name == "JACS / ACS" || editor.name == "Presentation" {
                                editor.name = "Custom".into();
                            }
                        }
                        Action::Advanced(value) => editor.advanced = value,
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
        let preset = candidate.as_ref().ok().and_then(|style| {
            [Preset::Jacs, Preset::Presentation]
                .into_iter()
                .find(|p| p.style() == *style)
        });
        let mut body = column![
            command("‹ Properties")
                .on_press(action(Action::Cancel))
                .style(button::text),
            text("Drawing style").size(19),
            text("Physical sizes for this document")
                .size(12)
                .color(super::workspace::muted()),
            pick_list([Preset::Jacs, Preset::Presentation], preset, move |p| {
                action(Action::Preset(p))
            })
            .placeholder("Custom style")
            .width(Length::Fill)
            .text_size(13),
        ]
        .spacing(10);
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
            text_input("Style name", &editor.name)
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
                    text_input("", value)
                        .on_input(move |s| action(Action::Input(field, s)))
                        .size(13)
                        .padding(6)
                        .width(74)
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
                    .color(super::workspace::muted()),
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
                    .color(super::workspace::muted()),
            );
        let mut footer = column![super::workspace::horizontal_line()].spacing(8);
        if let Err(error) = &candidate {
            footer = footer.push(
                text(error.clone())
                    .size(12)
                    .color(iced::Color::from_rgb8(164, 54, 47)),
            );
        }
        footer = footer
            .push(
                row![
                    command("Load…")
                        .on_press(action(Action::Load))
                        .style(button::text),
                    command("Save style…")
                        .on_press_maybe(candidate.is_ok().then_some(action(Action::Save)))
                        .style(button::text)
                ]
                .spacing(6),
            )
            .push(
                row![
                    command("Cancel")
                        .on_press(action(Action::Cancel))
                        .style(button::secondary),
                    command("Apply to document")
                        .on_press_maybe(candidate.is_ok().then_some(action(Action::Apply)))
                        .style(button::primary)
                ]
                .spacing(8),
            );
        container(
            column![
                scrollable(container(body).padding(iced::Padding {
                    right: 12.,
                    ..Default::default()
                }))
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
