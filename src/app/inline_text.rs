//! In-place caption drafts. Applying is one document edit; Escape is nonmutating.
use super::{App, Message};
use crate::canvas::layered::canvas;
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, stack, text, text_editor,
};
use iced::{Alignment, Border, Color, Element, Length, Task};
use reshiki::{
    document::{Annotation, Document, Point},
    typography::{StyleChange, TextAlign, TextFormat, TextStyle},
};

#[derive(Debug, Clone)]
pub enum Action {
    Begin(Option<u64>, Point),
    Finish(bool),
    Undo(bool),
}
#[derive(Clone)]
struct Revision {
    text: String,
    format: TextFormat,
    auto_formula: bool,
}
pub struct State {
    original: Option<Annotation>,
    position: Point,
    epoch: u64,
    past: Vec<Revision>,
    future: Vec<Revision>,
    auto_formula: bool,
}
impl App {
    pub(super) fn auto_format_caption(&mut self) {
        if self.inline_text.as_ref().is_some_and(|s| s.auto_formula) {
            let formula = reshiki::typography::is_formula(&self.caption);
            self.caption_format.style.formula = formula;
            for span in &mut self.caption_format.spans {
                span.style.formula = formula;
            }
        }
    }
    pub(super) fn manual_caption_format(&mut self) {
        if let Some(state) = &mut self.inline_text {
            state.auto_formula = false;
        }
    }
    pub(super) fn text_history_available(&self, redo: bool) -> Option<bool> {
        self.inline_text.as_ref().map(|s| {
            if redo {
                !s.future.is_empty()
            } else {
                !s.past.is_empty()
            }
        })
    }
    pub(super) fn inline_label_id(&self) -> Option<u64> {
        self.inline_text.as_ref()?.original.as_ref().map(|a| a.id)
    }
    pub(super) fn inline_changed(&self) -> bool {
        self.inline_text
            .as_ref()
            .is_some_and(|s| match &s.original {
                Some(a) => a.text != self.caption || a.format != self.caption_format,
                None => !self.caption.trim().is_empty(),
            })
    }
    pub(super) fn inline_checkpoint(&mut self) {
        if let Some(state) = &mut self.inline_text {
            state.past.push(Revision {
                text: self.caption.clone(),
                format: self.caption_format.clone(),
                auto_formula: state.auto_formula,
            });
            state.future.clear();
            if state.past.len() > 100 {
                state.past.remove(0);
            }
            self.autosaved_revision = None;
        }
    }
    fn inline_candidate(&self) -> Result<Document, String> {
        let Some(state) = &self.inline_text else {
            return Ok(self.doc.clone());
        };
        if state.epoch != self.file_epoch {
            return Err("The drawing changed. Cancel this text draft before continuing.".into());
        }
        if let Some(original) = &state.original
            && self.doc.annotations.iter().find(|a| a.id == original.id) != Some(original)
        {
            return Err("This label changed elsewhere. Copy your draft or cancel before editing the updated label.".into());
        }
        self.caption_format.validate(&self.caption)?;
        let mut doc = self.doc.clone();
        match (&state.original, self.caption.trim().is_empty()) {
            (Some(original), true) => doc.delete(&[original.id]),
            (Some(original), false) => {
                if let Some(label) = doc.annotations.iter_mut().find(|a| a.id == original.id) {
                    label.text = self.caption.clone();
                    label.format = self.caption_format.clone();
                }
            }
            (None, false) => doc.annotations.push(Annotation {
                id: doc.next_id(),
                position: state.position,
                text: self.caption.clone(),
                format: self.caption_format.clone(),
            }),
            (None, true) => {}
        }
        doc.validate()?;
        Ok(doc)
    }
    pub(super) fn recovery_document(&self) -> Document {
        self.inline_candidate().unwrap_or_else(|_| self.doc.clone())
    }
    pub(super) fn finish_inline(&mut self, apply: bool) -> bool {
        if self.inline_text.is_none() {
            return true;
        }
        if apply {
            let document = match self.inline_candidate() {
                Ok(doc) => doc,
                Err(error) => {
                    self.error = true;
                    self.status = error;
                    return false;
                }
            };
            let id = self.inline_text.as_ref().map(|s| {
                s.original
                    .as_ref()
                    .map(|a| a.id)
                    .unwrap_or_else(|| self.doc.next_id())
            });
            let before = self.doc.clone();
            self.doc = document;
            self.selected = id
                .into_iter()
                .filter(|id| self.doc.annotations.iter().any(|a| a.id == *id))
                .collect();
            self.caption_target = self.selected.first().copied();
            self.inline_text = None;
            self.changed(before);
            if let Some(label) = self
                .doc
                .annotations
                .iter()
                .find(|a| Some(a.id) == self.caption_target)
            {
                let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
                self.camera.center = reveal_label(self.camera, paper.size(), label);
            }
        } else {
            self.inline_text = None;
            self.error = false;
            self.status = "Text edit cancelled".into();
        }
        self.autosaved_revision = None;
        self.tool = crate::canvas::Tool::Select;
        self.selected.retain(|id| self.doc.all_ids().contains(id));
        let inspector = (self.inspector_open, self.inspector_tab);
        self.sync_typography();
        (self.inspector_open, self.inspector_tab) = inspector;
        true
    }
    pub(super) fn inline_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Begin(id, position) => {
                if self.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                let original = id
                    .and_then(|id| self.doc.annotations.iter().find(|a| a.id == id))
                    .cloned();
                if id.is_some() && original.is_none() {
                    return Task::none();
                }
                self.caption = original
                    .as_ref()
                    .map(|a| a.text.clone())
                    .unwrap_or_default();
                self.caption_format =
                    original
                        .as_ref()
                        .map(|a| a.format.clone())
                        .unwrap_or_else(|| TextFormat {
                            style: self.caption_format.style.clone(),
                            ..Default::default()
                        });
                self.caption_target = id;
                self.selected = id.into_iter().collect();
                self.caption_editor = text_editor::Content::with_text(&self.caption);
                self.caption_editor
                    .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
                self.inline_text = Some(State {
                    auto_formula: original.is_none()
                        && self.caption_format.style.script == reshiki::typography::Script::Normal,
                    position: original.as_ref().map(|a| a.position).unwrap_or(position),
                    original,
                    epoch: self.file_epoch,
                    past: vec![],
                    future: vec![],
                });
                self.palette = None;
                self.hover = None;
                self.fit_to_view = false;
                self.error = false;
                self.status = "Editing text · ⌘/Ctrl Enter applies · Escape cancels".into();
                self.sync_style_inputs();
                return iced::widget::operation::focus("inline-caption");
            }
            Action::Finish(apply) => {
                self.finish_inline(apply);
            }
            Action::Undo(redo) => {
                if let Some(state) = &mut self.inline_text {
                    let current = Revision {
                        text: self.caption.clone(),
                        format: self.caption_format.clone(),
                        auto_formula: state.auto_formula,
                    };
                    let revision = if redo {
                        state.future.pop()
                    } else {
                        state.past.pop()
                    };
                    if let Some(revision) = revision {
                        if redo {
                            state.past.push(current);
                        } else {
                            state.future.push(current);
                        }
                        self.caption = revision.text;
                        self.caption_format = revision.format;
                        state.auto_formula = revision.auto_formula;
                        self.caption_editor = text_editor::Content::with_text(&self.caption);
                        self.caption_editor
                            .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
                        self.autosaved_revision = None;
                        self.sync_style_inputs();
                    }
                }
            }
        }
        Task::none()
    }
    pub(super) fn with_inline_text<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(state) = &self.inline_text else {
            return base;
        };
        let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        let position = self.camera.screen(state.position, paper);
        let size = (self.caption_format.style.size() * self.camera.zoom).clamp(12., 56.);
        let natural = reshiki::typography::layout(&self.caption, &self.caption_format);
        let old = state.original.as_ref().map(|a| a.size().0).unwrap_or(0.);
        let complex = complex_format(&self.caption_format) && self.viewport.height >= 240.;
        let extra = if complex { 120. } else { 40. };
        let bounds = editor_bounds(
            self.viewport,
            iced::Point::new(paper.x + position.x - 8., paper.y + position.y - 8.),
            natural.width.max(old) * self.camera.zoom + 36.,
            (natural.height * self.camera.zoom + 20.).max(size * 1.5 + 16.),
            extra,
        );
        let x = bounds.x;
        let y = bounds.y;
        let width = bounds.width;
        let editor_height = (bounds.height - extra).max(24.);
        let editor = text_editor(&self.caption_editor)
            .id("inline-caption")
            .font(iced::Font::with_name(reshiki::style::font_name(
                &self.caption_format.style.family,
            )))
            .size(size)
            .line_height(self.caption_format.line_spacing)
            .padding(7)
            .height(editor_height)
            .placeholder("Type a label…")
            .on_action(Message::CaptionAction)
            .highlight_with::<CaptionHighlighter>(
                {
                    let mut format = self.caption_format.clone();
                    format.style.color = self.doc.canvas_theme.color(format.style.color);
                    for span in &mut format.spans {
                        span.style.color = self.doc.canvas_theme.color(span.style.color);
                    }
                    (self.caption.clone(), format)
                },
                |style, _| {
                    let [r, g, b] = style.color;
                    iced::advanced::text::highlighter::Format {
                        color: Some(Color::from_rgb8(r, g, b)),
                        font: Some(iced::Font {
                            family: iced::font::Family::Name(reshiki::style::font_name(
                                &style.family,
                            )),
                            weight: if style.bold {
                                iced::font::Weight::Bold
                            } else {
                                iced::font::Weight::Normal
                            },
                            style: if style.italic {
                                iced::font::Style::Italic
                            } else {
                                iced::font::Style::Normal
                            },
                            ..Default::default()
                        }),
                    }
                },
            )
            .key_binding(|key| {
                if !matches!(key.status, text_editor::Status::Focused { .. }) {
                    return text_editor::Binding::from_key_press(key);
                }
                use iced::keyboard::{Key, key::Named};
                let custom = match &key.key {
                    Key::Named(Named::Escape) => Some(Message::InlineText(Action::Finish(false))),
                    Key::Named(Named::Enter) if key.modifiers.command() => {
                        Some(Message::InlineText(Action::Finish(true)))
                    }
                    Key::Character(c) if key.modifiers.command() => {
                        match c.to_ascii_lowercase().as_str() {
                            "l" if key.modifiers.shift() => {
                                Some(Message::TextAlign(reshiki::typography::TextAlign::Left))
                            }
                            "c" if key.modifiers.shift() => {
                                Some(Message::TextAlign(reshiki::typography::TextAlign::Center))
                            }
                            "r" if key.modifiers.shift() => {
                                Some(Message::TextAlign(reshiki::typography::TextAlign::Right))
                            }
                            "j" if key.modifiers.shift() => Some(Message::TextAlign(
                                reshiki::typography::TextAlign::Justified,
                            )),
                            "f" => Some(Message::TextStyle(StyleChange::Formula(
                                !self.current_text_style().formula,
                            ))),
                            "-" => Some(Message::TextStyle(StyleChange::Script(
                                reshiki::typography::Script::Subscript,
                            ))),
                            "+" | "=" => Some(Message::TextStyle(StyleChange::Script(
                                reshiki::typography::Script::Superscript,
                            ))),
                            "z" => Some(Message::InlineText(Action::Undo(key.modifiers.shift()))),
                            "b" => Some(Message::TextStyle(StyleChange::Bold(
                                !self.current_text_style().bold,
                            ))),
                            "i" => Some(Message::TextStyle(StyleChange::Italic(
                                !self.current_text_style().italic,
                            ))),
                            "u" => Some(Message::TextStyle(StyleChange::Underline(
                                !self.current_text_style().underline,
                            ))),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                custom
                    .map(text_editor::Binding::Custom)
                    .or_else(|| text_editor::Binding::from_key_press(key))
            })
            .style(move |_, _| text_editor::Style {
                background: crate::appearance::color(self.doc.canvas_theme.is_dark(), Color::WHITE)
                    .into(),
                border: Border::default(),
                placeholder: crate::appearance::color(
                    self.doc.canvas_theme.is_dark(),
                    super::workspace::muted(),
                ),
                value: crate::appearance::color(self.doc.canvas_theme.is_dark(), Color::BLACK),
                selection: crate::appearance::color(
                    self.doc.canvas_theme.is_dark(),
                    Color::from_rgb8(193, 224, 216),
                ),
            });
        let mut body = column![editor].spacing(5);
        if complex {
            let preview = Document {
                canvas_theme: self.doc.canvas_theme,
                color_theme: self.doc.color_theme,
                annotations: vec![Annotation {
                    id: 1,
                    position: Point::default(),
                    text: self.caption.clone(),
                    format: self.caption_format.clone(),
                }],
                ..Default::default()
            };
            body = body
                .push(
                    text("Appearance")
                        .size(10)
                        .style(super::workspace::muted_text),
                )
                .push(
                    canvas(crate::canvas::OwnedDrawingPreview(preview))
                        .width(Length::Fill)
                        .height(62),
                );
        }
        body = body.push(
            row![
                text("Esc to cancel")
                    .size(10)
                    .style(super::workspace::muted_text),
                Space::new().width(Length::Fill),
                iced::widget::tooltip(
                    button(text("Done ↵").size(11))
                        .on_press(Message::InlineText(Action::Finish(true)))
                        .style(super::workspace::control(true)),
                    text(super::platform_shortcut(
                        "Apply · ⌘ Enter",
                        "Apply · Ctrl Enter"
                    ))
                    .size(12),
                    iced::widget::tooltip::Position::Top,
                )
            ]
            .align_y(Alignment::Center)
            .padding([2, 7]),
        );
        let popup = container(body).width(width).padding(2).style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(Color::WHITE.into()),
                    border: Border {
                        color: Color::from_rgb8(93, 158, 140),
                        width: 1.,
                        radius: 5.into(),
                    },
                    shadow: super::workspace::surface_shadow(iced::Shadow {
                        color: Color::from_rgba8(20, 40, 35, 0.1),
                        offset: iced::Vector::new(0., 3.),
                        blur_radius: 12.,
                    }),
                    ..Default::default()
                },
            )
        });
        stack![
            base,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
            )
            .on_press(Message::InlineText(Action::Finish(true))),
            container(opaque(popup)).padding(iced::Padding {
                left: x,
                top: y,
                right: 4.,
                bottom: 4.
            })
        ]
        .into()
    }
}

struct CaptionHighlighter {
    settings: (String, TextFormat),
    line: usize,
}
impl iced::advanced::text::Highlighter for CaptionHighlighter {
    type Settings = (String, TextFormat);
    type Highlight = TextStyle;
    type Iterator<'a> = std::vec::IntoIter<(std::ops::Range<usize>, TextStyle)>;
    fn new(settings: &Self::Settings) -> Self {
        Self {
            settings: settings.clone(),
            line: 0,
        }
    }
    fn update(&mut self, settings: &Self::Settings) {
        self.settings = settings.clone();
        self.line = 0;
    }
    fn change_line(&mut self, line: usize) {
        self.line = line;
    }
    fn current_line(&self) -> usize {
        self.line
    }
    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let offset: usize = self
            .settings
            .0
            .split_inclusive('\n')
            .take(self.line)
            .map(str::len)
            .sum();
        self.line += 1;
        line.char_indices()
            .map(|(index, c)| {
                let mut style = self.settings.1.at(offset + index).clone();
                style.family = reshiki::style::glyph_metrics(c, &style).0.into();
                (index..index + c.len_utf8(), style)
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}

fn complex_format(format: &TextFormat) -> bool {
    format.width_pt.is_some()
        || format.alignment != TextAlign::Left
        || format.style.formula
        || format.style.underline
        || format.style.script != reshiki::typography::Script::Normal
        || format.spans.iter().any(|s| {
            s.style.size_pt != format.style.size_pt
                || s.style.underline
                || s.style.formula
                || s.style.script != reshiki::typography::Script::Normal
        })
}

fn reveal_label(camera: crate::canvas::Camera, viewport: iced::Size, label: &Annotation) -> Point {
    let (width, height) = label.size();
    let reveal = |center: f32, extent: f32, start: f32, size: f32| {
        let available = ((extent - 40.).max(20.) / camera.zoom).max(1.);
        let low = center - available / 2.;
        let high = center + available / 2.;
        if size > available || start < low {
            start + available / 2.
        } else if start + size > high {
            start + size - available / 2.
        } else {
            center
        }
    };
    Point::new(
        reveal(camera.center.x, viewport.width, label.position.x, width),
        reveal(camera.center.y, viewport.height, label.position.y, height),
    )
}
fn editor_bounds(
    viewport: iced::Size,
    anchor: iced::Point,
    width: f32,
    height: f32,
    extra: f32,
) -> iced::Rectangle {
    let width = width.max(240.).min((viewport.width - 16.).max(24.));
    let height = (height.max(60.) + extra).min((viewport.height - 16.).max(24.));
    iced::Rectangle::new(
        iced::Point::new(
            anchor.x.clamp(8., (viewport.width - width - 8.).max(8.)),
            anchor.y.clamp(8., (viewport.height - height - 8.).max(8.)),
        ),
        iced::Size::new(width, height),
    )
}

/// Finish the current label before commands that change its editing context.
/// Background results and view/typography controls keep the draft intact.
pub(super) fn commits_draft(message: &Message) -> bool {
    matches!(
        message,
        Message::Pages(
            super::pages::Action::Show
                | super::pages::Action::Open
                | super::pages::Action::Apply
                | super::pages::Action::Remove
                | super::pages::Action::Center(_)
                | super::pages::Action::Export
        ) | Message::Printing(super::printing::Action::Start(_))
            | Message::Pictures(
                super::pictures::Action::Import
                    | super::pictures::Action::Replace
                    | super::pictures::Action::Resize(_)
                    | super::pictures::Action::RestoreAspect
            )
            | Message::Tool(_)
            | Message::Palette(_)
            | Message::ContextKey(_)
            | Message::Shortcut(
                super::shortcuts::Action::FixedLength
                    | super::shortcuts::Action::FixedAngles
                    | super::shortcuts::Action::Nudge(..)
                    | super::shortcuts::Action::Join
                    | super::shortcuts::Action::CopyText(_)
            )
            | Message::New
            | Message::Open
            | Message::Save
            | Message::SaveAs
            | Message::Close(_)
            | Message::Export(_)
            | Message::Copy(_)
            | Message::CopyImage
            | Message::CopySmiles
            | Message::Paste
            | Message::PastePicture
            | Message::Duplicate
            | Message::Delete
            | Message::SelectAll
            | Message::InvertSelection
            | Message::Group
            | Message::Ungroup
            | Message::IntegralGroup(_)
            | Message::AddFrame(_)
            | Message::Transform(_)
            | Message::Arrange(_)
            | Message::Clean
            | Message::Analyze
            | Message::Import
            | Message::InsertInput
            | Message::Example(_)
            | Message::Restore
            | Message::Element(_)
            | Message::ApplyElement
            | Message::InsertTemplate(_)
            | Message::Templates(_)
            | Message::AromaticDisplay
            | Message::ToggleAromaticRing
            | Message::RingSize(_)
            | Message::AromaticRing(_)
            | Message::ArrowStyle(_)
            | Message::Abbreviations(_)
            | Message::Labels(_)
            | Message::Charge(_)
            | Message::ApplyIsotope
            | Message::AtomRadical(_)
            | Message::RemoveMark(..)
            | Message::RotateMark(..)
            | Message::ApplyBondPreset(_)
            | Message::BondPosition(_)
            | Message::ReverseBonds
            | Message::BondDepth(_)
            | Message::GraphicStyle(_)
            | Message::GraphicLayer(_)
            | Message::ArrowAction(_)
            | Message::Assistant(super::assistant::Action::Send)
    ) || matches!(message, Message::Canvas(edit) if !matches!(edit, crate::canvas::Edit::Hover(_) | crate::canvas::Edit::Pan(..) | crate::canvas::Edit::Zoom(..)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{Edit as CanvasEdit, Tool};
    use iced::widget::text_editor::{Action as Input, Edit, Motion};

    fn app() -> App {
        let (mut app, _) = App::new();
        app.busy = false;
        app
    }
    fn label(app: &mut App, text: &str) -> u64 {
        let id = app.doc.next_id();
        app.doc.annotations.push(Annotation {
            id,
            position: Point::new(20., 30.),
            text: text.into(),
            format: Default::default(),
        });
        id
    }
    fn type_text(app: &mut App, value: &str) {
        let _ = app.update(Message::CaptionAction(Input::Edit(Edit::Paste(
            value.to_owned().into(),
        ))));
    }
    fn begin(app: &mut App, id: Option<u64>) {
        let _ = app.update(Message::InlineText(Action::Begin(id, Point::new(60., 70.))));
    }

    #[test]
    fn new_formula_captions_format_automatically_and_manual_controls_win() {
        let mut app = app();
        begin(&mut app, None);
        type_text(&mut app, "C2H2");
        assert!(app.caption_format.style.formula);
        let preview = reshiki::typography::layout(&app.caption, &app.caption_format);
        assert!(
            preview
                .fragments
                .iter()
                .any(|f| f.text == "2" && f.style.script == reshiki::typography::Script::Subscript)
        );
        assert!(app.finish_inline(true));
        let original = app.doc.clone();
        assert!(
            app.doc.atoms.is_empty(),
            "Caption formatting must not create molecular atoms"
        );
        let _ = app.update(Message::Undo);
        assert!(app.doc.annotations.is_empty());
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, original);
        begin(&mut app, None);
        type_text(&mut app, "Figure 2");
        assert!(!app.caption_format.style.formula);
        let _ = app.update(Message::InlineText(Action::Finish(false)));
        begin(&mut app, None);
        type_text(&mut app, "H2O");
        app.apply_text_style(StyleChange::Formula(false));
        type_text(&mut app, "2");
        assert!(!app.caption_format.style.formula);
        let _ = app.update(Message::InlineText(Action::Finish(false)));
        assert_eq!(app.doc, original);
    }

    #[test]
    fn text_tool_typing_and_formatting_commit_as_one_undo_step() {
        let mut app = app();
        app.inspector_open = false;
        let before = app.doc.clone();
        let _ = app.update(Message::Tool(Tool::Text));
        let _ = app.update(Message::Canvas(CanvasEdit::Click(Point::new(60., 70.))));
        assert!(app.inline_text.is_some());
        assert!(!app.inspector_open);
        type_text(&mut app, "加熱 H2O");
        let _ = app.update(Message::TextStyle(StyleChange::Formula(true)));
        let _ = app.update(Message::TextStyle(StyleChange::Color([30, 90, 70])));
        assert_eq!(app.doc, before);
        assert!(app.dirty());
        let _ = app.update(Message::InlineText(Action::Finish(true)));
        let finished = app.doc.clone();
        assert_eq!(finished.annotations[0].text, "加熱 H2O");
        assert_eq!(finished.annotations[0].position, Point::new(60., 70.));
        assert_eq!(finished.annotations[0].format.style.color, [30, 90, 70]);
        assert!(finished.annotations[0].format.style.formula);
        assert_eq!(app.selected, vec![finished.annotations[0].id]);
        assert!(!app.inspector_open);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, finished);
    }

    #[test]
    fn draft_undo_and_cancel_preserve_independent_canvas_changes() {
        let mut app = app();
        let id = label(&mut app, "酸触媒");
        begin(&mut app, Some(id));
        type_text(&mut app, "、加熱");
        let _ = app.update(Message::TextStyle(StyleChange::Bold(true)));
        let _ = app.update(Message::Undo);
        assert!(!app.caption_format.style.bold);
        assert_eq!(app.caption, "酸触媒、加熱");
        let _ = app.update(Message::Redo);
        assert!(app.caption_format.style.bold);
        let before = app.doc.clone();
        app.doc.add_atom("O", Point::new(200., 20.));
        app.changed(before.clone());
        let concurrent = app.doc.clone();
        app.sync_typography();
        assert_eq!(app.caption, "酸触媒、加熱");
        let _ = app.update(Message::Escape);
        assert_eq!(app.doc, concurrent);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }

    #[test]
    fn text_commit_preserves_other_edits_and_rejects_conflicting_targets() {
        let mut app = app();
        let id = label(&mut app, "A");
        begin(&mut app, Some(id));
        type_text(&mut app, "B");
        app.doc.add_atom("C", Point::default());
        let concurrent = app.doc.clone();
        assert!(app.finish_inline(true));
        assert_eq!(app.doc.annotations[0].text, "AB");
        assert_eq!(app.doc.atoms, concurrent.atoms);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, concurrent);
        begin(&mut app, Some(id));
        type_text(&mut app, " draft");
        app.doc.annotations[0].text = "changed externally".into();
        let external = app.doc.clone();
        assert!(!app.finish_inline(true));
        assert_eq!(app.doc, external);
        assert!(app.inline_text.is_some());
        assert!(app.finish_inline(false));
        assert_eq!(app.doc, external);
        begin(&mut app, Some(id));
        type_text(&mut app, " epoch");
        app.file_epoch += 1;
        assert!(!app.finish_inline(true));
        assert_eq!(app.doc, external);
    }

    #[test]
    fn untouched_labels_keep_order_and_do_not_add_history() {
        let mut app = app();
        let id = label(&mut app, "First");
        label(&mut app, "Second");
        let before = app.doc.clone();
        begin(&mut app, Some(id));
        assert!(app.finish_inline(true));
        assert_eq!(app.doc, before);
        assert!(!app.history.undo(&mut app.doc));
    }

    #[test]
    fn deleting_a_label_prunes_groups_and_undo_restores_them() {
        let mut app = app();
        let id = label(&mut app, "Caption");
        let atom = app.doc.add_atom("C", Point::default());
        app.selected = vec![id, atom];
        let _ = app.update(Message::Group);
        let before = app.doc.clone();
        assert!(!before.groups.is_empty());
        begin(&mut app, Some(id));
        let _ = app.update(Message::CaptionAction(Input::SelectAll));
        type_text(&mut app, "");
        assert!(app.finish_inline(true));
        assert!(app.doc.annotations.is_empty());
        assert_eq!(app.doc.atoms, before.atoms);
        app.doc.validate().unwrap();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, before);
    }

    #[test]
    fn unicode_ranges_keep_formatting_during_edits() {
        let mut app = app();
        let id = label(&mut app, "酸触媒 H2O");
        let range = "酸触媒 ".len().."酸触媒 H2O".len();
        app.doc.annotations[0]
            .format
            .apply("酸触媒 H2O", Some(range), &StyleChange::Bold(true));
        begin(&mut app, Some(id));
        let _ = app.update(Message::CaptionAction(Input::Move(Motion::DocumentStart)));
        type_text(&mut app, "濃 ");
        assert!(app.finish_inline(true));
        let label = &app.doc.annotations[0];
        assert_eq!(label.text, "濃 酸触媒 H2O");
        assert!(label.format.at("濃 酸触媒 ".len()).bold);
        assert!(!label.format.at(0).bold);
        label.format.validate(&label.text).unwrap();
    }

    #[test]
    fn recovery_includes_drafts_and_cancel_removes_them() {
        let mut app = app();
        let dir = tempfile::tempdir().unwrap();
        app.recovery = Some(reshiki::recovery::Recovery::in_directory(dir.path()).unwrap());
        begin(&mut app, None);
        type_text(&mut app, "Unsaved label");
        let _ = app.update(Message::Tick);
        let path = app.recovery.as_ref().unwrap().session.clone();
        let snapshot: reshiki::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(snapshot.document.annotations[0].text, "Unsaved label");
        assert!(app.doc.annotations.is_empty());
        type_text(&mut app, " updated");
        let _ = app.update(Message::Tick);
        let snapshot: reshiki::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            snapshot.document.annotations[0].text,
            "Unsaved label updated"
        );
        let _ = app.update(Message::Escape);
        let _ = app.update(Message::Tick);
        assert!(!path.exists());
    }

    #[test]
    fn file_and_tool_commands_finish_drafts_before_continuing() {
        let mut app = app();
        begin(&mut app, None);
        type_text(&mut app, "Keep me");
        let _ = app.update(Message::New);
        assert!(app.inline_text.is_none());
        assert_eq!(app.doc.annotations[0].text, "Keep me");
        assert!(app.pending.is_some());
        let _ = app.update(Message::Cancel);
        begin(&mut app, Some(1));
        type_text(&mut app, " too");
        let _ = app.update(Message::Tool(Tool::Bond(1)));
        assert!(app.inline_text.is_none());
        assert_eq!(app.tool, Tool::Bond(1));
        assert_eq!(app.doc.annotations[0].text, "Keep me too");
    }

    #[test]
    fn highlighter_uses_utf8_offsets_across_lines() {
        use iced::advanced::text::Highlighter;
        let text = "触媒\nH2O";
        let mut format = TextFormat::default();
        format.apply(
            text,
            Some("触媒\n".len()..text.len()),
            &StyleChange::Bold(true),
        );
        let mut highlighter = CaptionHighlighter::new(&(text.into(), format));
        let first: Vec<_> = highlighter.highlight_line("触媒").collect();
        assert_eq!(first[0].0, 0..3);
        assert_eq!(first[1].0, 3..6);
        assert!(first.iter().all(|(_, style)| !style.bold));
        let second: Vec<_> = highlighter.highlight_line("H2O").collect();
        assert!(second.iter().all(|(_, style)| style.bold));
        highlighter.change_line(0);
        assert!(
            highlighter
                .highlight_line("触媒")
                .all(|(_, style)| !style.bold)
        );
    }

    #[test]
    fn finished_labels_are_revealed_without_changing_zoom() {
        let mut app = app();
        app.camera.zoom = 1.;
        app.camera.center = Point::default();
        app.viewport = iced::Size::new(420., 360.);
        let _ = app.inline_action(Action::Begin(None, Point::new(190., 160.)));
        type_text(&mut app, "Conditions\nTime");
        assert!(app.finish_inline(true));
        let label = &app.doc.annotations[0];
        let (width, height) = label.size();
        let position = app
            .camera
            .screen(label.position, iced::Rectangle::with_size(app.viewport));
        assert!(position.x >= 20. && position.x + width <= app.viewport.width - 19.);
        assert!(position.y >= 20. && position.y + height <= app.viewport.height - 19.);
        assert_eq!(app.camera.zoom, 1.);
    }

    #[test]
    fn full_range_styles_are_reflected_in_the_toolbar_after_commit() {
        let mut app = app();
        let id = label(&mut app, "酸触媒");
        begin(&mut app, Some(id));
        let _ = app.update(Message::CaptionAction(Input::SelectAll));
        app.apply_text_style(StyleChange::Bold(true));
        app.apply_selection_color([20, 70, 130]);
        assert!(app.finish_inline(true));
        assert!(app.current_text_style().bold);
        assert_eq!(app.current_selection_color(), Some([20, 70, 130]));
    }

    #[test]
    fn popup_stays_in_view_at_document_edges_with_long_labels() {
        let viewport = iced::Size::new(420., 360.);
        for anchor in [
            iced::Point::new(-100., -100.),
            iced::Point::new(410., 350.),
            iced::Point::new(900., 800.),
        ] {
            for extra in [40., 120.] {
                let bounds = editor_bounds(viewport, anchor, 4000., 1800., extra);
                assert!(bounds.x >= 0. && bounds.y >= 0.);
                assert!(bounds.x + bounds.width <= viewport.width);
                assert!(bounds.y + bounds.height <= viewport.height);
                assert!(bounds.height > extra);
            }
        }
    }
}
