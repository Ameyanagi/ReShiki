//! In-place caption drafts. Applying is one document edit; Escape is nonmutating.
use super::{App, Message};
use crate::canvas::layered::canvas;
use iced::widget::{Space, column, container, mouse_area, opaque, row, stack, text, text_editor};
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
    ReplaceText(String),
}
#[derive(Clone)]
struct Revision {
    text: String,
    format: TextFormat,
    auto_formula: bool,
}
pub struct State {
    pub(super) session: iced::widget::Id,
    original: Option<Annotation>,
    position: Point,
    epoch: u64,
    past: Vec<Revision>,
    future: Vec<Revision>,
    auto_formula: bool,
}
impl super::DocumentTab {
    /// A caption draft that differs from the text it started with.
    pub(super) fn inline_changed(&self) -> bool {
        self.inline_text
            .as_ref()
            .is_some_and(|s| match &s.original {
                Some(a) => a.text != self.caption || a.format != self.caption_format,
                None => !self.caption.trim().is_empty(),
            })
    }
}
impl App {
    pub(super) fn auto_format_caption(&mut self) {
        if self
            .tab
            .inline_text
            .as_ref()
            .is_some_and(|s| s.auto_formula)
        {
            let formula = reshiki::typography::is_formula(&self.tab.caption);
            self.tab.caption_format.style.formula = formula;
            for span in &mut self.tab.caption_format.spans {
                span.style.formula = formula;
            }
        }
    }
    pub(super) fn manual_caption_format(&mut self) {
        if let Some(state) = &mut self.tab.inline_text {
            state.auto_formula = false;
        }
    }
    pub(super) fn text_history_available(&self, redo: bool) -> Option<bool> {
        self.tab.inline_text.as_ref().map(|s| {
            if redo {
                !s.future.is_empty()
            } else {
                !s.past.is_empty()
            }
        })
    }
    pub(super) fn inline_label_id(&self) -> Option<u64> {
        self.tab
            .inline_text
            .as_ref()?
            .original
            .as_ref()
            .map(|a| a.id)
    }
    pub(super) fn inline_checkpoint(&mut self) {
        if let Some(state) = &mut self.tab.inline_text {
            state.past.push(Revision {
                text: self.tab.caption.clone(),
                format: self.tab.caption_format.clone(),
                auto_formula: state.auto_formula,
            });
            state.future.clear();
            if state.past.len() > 100 {
                state.past.remove(0);
            }
            self.tab.autosaved_revision = None;
            self.tab.autosave.edited_draft();
        }
    }
    pub(super) fn inline_candidate(&self) -> Result<Document, String> {
        self.inline_snapshot(true)
    }
    fn inline_snapshot(&self, validate: bool) -> Result<Document, String> {
        let Some(state) = &self.tab.inline_text else {
            return Ok(self.tab.doc.clone());
        };
        if state.epoch != self.tab.file_epoch {
            return Err("The drawing changed. Cancel this text draft before continuing.".into());
        }
        if let Some(original) = &state.original
            && self
                .tab
                .doc
                .annotations
                .iter()
                .find(|a| a.id == original.id)
                != Some(original)
        {
            return Err("This label changed elsewhere. Copy your draft or cancel before editing the updated label.".into());
        }
        self.tab.caption_format.validate(&self.tab.caption)?;
        let mut doc = self.tab.doc.clone();
        match (&state.original, self.tab.caption.trim().is_empty()) {
            (Some(original), true) => doc.delete(&[original.id]),
            (Some(original), false) => {
                if let Some(label) = doc.annotations.iter_mut().find(|a| a.id == original.id) {
                    label.text = self.tab.caption.clone();
                    label.format = self.tab.caption_format.clone();
                }
            }
            (None, false) => doc.annotations.push(Annotation {
                id: doc.next_id(),
                position: state.position,
                text: self.tab.caption.clone(),
                format: self.tab.caption_format.clone(),
            }),
            (None, true) => {}
        }
        if validate {
            doc.validate()?;
        }
        Ok(doc)
    }
    pub(super) fn recovery_document(&self) -> Document {
        // Full validation runs on the recovery worker; building a caption draft
        // only needs its local format/identity checks on the event-loop thread.
        self.inline_snapshot(false)
            .unwrap_or_else(|_| self.tab.doc.clone())
    }
    pub(super) fn finish_inline(&mut self, apply: bool) -> bool {
        if self.tab.inline_text.is_none() {
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
            let id = self.tab.inline_text.as_ref().map(|s| {
                s.original
                    .as_ref()
                    .map(|a| a.id)
                    .unwrap_or_else(|| self.tab.doc.next_id())
            });
            let before = self.tab.doc.clone();
            self.tab.doc = document;
            self.tab.selected = id
                .into_iter()
                .filter(|id| self.tab.doc.annotations.iter().any(|a| a.id == *id))
                .collect();
            self.tab.caption_target = self.tab.selected.first().copied();
            self.tab.inline_text = None;
            self.changed(before);
            if let Some(label) = self
                .tab
                .doc
                .annotations
                .iter()
                .find(|a| Some(a.id) == self.tab.caption_target)
            {
                let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
                self.tab.camera.center = reveal_label(self.tab.camera, paper.size(), label);
            }
        } else {
            self.tab.inline_text = None;
            self.error = false;
            self.status = "Text edit cancelled".into();
        }
        self.tab.autosaved_revision = None;
        self.tab.autosave.edited_draft();
        self.tool = crate::canvas::Tool::Select;
        self.tab
            .selected
            .retain(|id| self.tab.doc.all_ids().contains(id));
        let inspector = (self.inspector_open, self.inspector_tab);
        self.sync_typography();
        (self.inspector_open, self.inspector_tab) = inspector;
        true
    }
    pub(super) fn inline_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Begin(id, position) => {
                if self.tab.cleanup.is_some() || !self.finish_inline(true) {
                    return Task::none();
                }
                let original = id
                    .and_then(|id| self.tab.doc.annotations.iter().find(|a| a.id == id))
                    .cloned();
                if id.is_some() && original.is_none() {
                    return Task::none();
                }
                self.tab.caption = original
                    .as_ref()
                    .map(|a| a.text.clone())
                    .unwrap_or_default();
                self.tab.caption_format = original
                    .as_ref()
                    .map(|a| a.format.clone())
                    .unwrap_or_else(|| TextFormat {
                        style: self.tab.caption_format.style.clone(),
                        ..Default::default()
                    });
                self.tab.caption_target = id;
                self.tab.selected = id.into_iter().collect();
                self.tab.caption_editor = text_editor::Content::with_text(&self.tab.caption);
                self.tab
                    .caption_editor
                    .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
                self.tab.inline_text = Some(State {
                    session: iced::widget::Id::unique(),
                    auto_formula: original.is_none()
                        && self.tab.caption_format.style.script
                            == reshiki::typography::Script::Normal,
                    position: original.as_ref().map(|a| a.position).unwrap_or(position),
                    original,
                    epoch: self.tab.file_epoch,
                    past: vec![],
                    future: vec![],
                });
                self.palette = None;
                self.tab.hover = None;
                self.tab.fit_to_view = false;
                self.error = false;
                self.status = format!(
                    "Editing text · {} applies · Escape cancels",
                    super::shortcuts::keys(iced::keyboard::Modifiers::COMMAND, "Enter")
                );
                self.sync_style_inputs();
                return iced::widget::operation::focus("inline-caption");
            }
            Action::ReplaceText(value) => {
                if self.tab.inline_text.is_some() {
                    self.caption_action(text_editor::Action::SelectAll);
                    self.caption_action(text_editor::Action::Edit(text_editor::Edit::Paste(
                        std::sync::Arc::new(value),
                    )));
                }
            }
            Action::Finish(apply) => {
                self.finish_inline(apply);
            }
            Action::Undo(redo) => {
                if let Some(state) = &mut self.tab.inline_text {
                    let current = Revision {
                        text: self.tab.caption.clone(),
                        format: self.tab.caption_format.clone(),
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
                        self.tab.caption = revision.text;
                        self.tab.caption_format = revision.format;
                        state.auto_formula = revision.auto_formula;
                        self.tab.caption_editor =
                            text_editor::Content::with_text(&self.tab.caption);
                        self.tab
                            .caption_editor
                            .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
                        self.tab.autosaved_revision = None;
                        self.tab.autosave.edited_draft();
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
        let Some(state) = &self.tab.inline_text else {
            return base;
        };
        let paper = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        let position = self.tab.camera.screen(state.position, paper);
        let size = (self.tab.caption_format.style.size() * self.tab.camera.zoom).clamp(12., 56.);
        let natural = reshiki::typography::layout(&self.tab.caption, &self.tab.caption_format);
        let old = state.original.as_ref().map(|a| a.size().0).unwrap_or(0.);
        let complex = complex_format(&self.tab.caption_format) && self.viewport.height >= 240.;
        let extra = if complex { 120. } else { 40. };
        let bounds = editor_bounds(
            self.viewport,
            iced::Point::new(paper.x + position.x - 8., paper.y + position.y - 8.),
            natural.width.max(old) * self.tab.camera.zoom + 36.,
            (natural.height * self.tab.camera.zoom + 20.).max(size * 1.5 + 16.),
            extra,
        );
        let x = bounds.x;
        let y = bounds.y;
        let width = bounds.width;
        let editor_height = (bounds.height - extra).max(24.);
        let editor = text_editor(&self.tab.caption_editor)
            .id("inline-caption")
            .font(iced::Font::with_name(reshiki::style::font_name(
                &self.tab.caption_format.style.family,
            )))
            .size(size)
            .line_height(self.tab.caption_format.line_spacing)
            .padding(7)
            .height(editor_height)
            .placeholder("Type a label…")
            .on_action(Message::CaptionAction)
            .highlight_with::<CaptionHighlighter>(
                {
                    // The editor draws display colors directly.
                    let palette = reshiki::palette::Palette::of(&self.tab.doc);
                    let mut format = self.tab.caption_format.clone();
                    format.style.color =
                        reshiki::palette::Color::Custom(palette.rgb(format.style.color));
                    for span in &mut format.spans {
                        span.style.color =
                            reshiki::palette::Color::Custom(palette.rgb(span.style.color));
                    }
                    (self.tab.caption.clone(), format)
                },
                |style, _| {
                    let [r, g, b] = style.color.rgb();
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
                background: crate::appearance::color(
                    self.tab.doc.canvas_theme.is_dark(),
                    Color::WHITE,
                )
                .into(),
                border: Border::default(),
                placeholder: crate::appearance::color(
                    self.tab.doc.canvas_theme.is_dark(),
                    super::workspace::muted(),
                ),
                value: crate::appearance::color(self.tab.doc.canvas_theme.is_dark(), Color::BLACK),
                selection: crate::appearance::color(
                    self.tab.doc.canvas_theme.is_dark(),
                    Color::from_rgb8(193, 224, 216),
                ),
            });
        let editor = reshiki::accessibility::editor(
            "inline-caption",
            "Caption text",
            self.tab.caption.clone(),
            editor,
            |value| Message::InlineText(Action::ReplaceText(value)),
        );
        let mut body = column![editor].spacing(5);
        if complex {
            let preview = Document {
                canvas_theme: self.tab.doc.canvas_theme,
                color_theme: self.tab.doc.color_theme,
                annotations: vec![Annotation {
                    id: 1,
                    position: Point::default(),
                    text: self.tab.caption.clone(),
                    format: self.tab.caption_format.clone(),
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
                    reshiki::accessibility::button(
                        "inline-caption-done",
                        "Apply caption text",
                        text("Done ↵").size(11)
                    )
                    .on_press(Message::InlineText(Action::Finish(true)))
                    .style(super::workspace::control(true)),
                    super::workspace::keyed_text(
                        "Apply",
                        Some(super::shortcuts::keys(
                            iced::keyboard::Modifiers::COMMAND,
                            "Enter"
                        )),
                        ""
                    ),
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
                    shadow: crate::appearance::surface_shadow(iced::Shadow {
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
            | Message::Imports(super::import::Action::Files(_))
            | Message::Pictures(
                super::pictures::Action::Replace
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
            | Message::OpenShortcutExamples
            | Message::Tabs(
                super::tabs::Action::Select(_)
                    | super::tabs::Action::Cycle(_)
                    | super::tabs::Action::Number(_)
                    | super::tabs::Action::Close(_)
            )
            | Message::Save
            | Message::SaveAs
            | Message::Close(_)
            | Message::Export(_)
            | Message::Copy(_)
            | Message::CopyImage
            | Message::CopyAs(_)
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
            | Message::ObjectToolbar(super::object_toolbar::Action::Layer(_))
            | Message::Transform(_)
            | Message::Arrange(_)
            | Message::Cleanup(super::cleanup::Action::Begin)
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
            | Message::ArrowAction(_)
            | Message::Assistant(super::assistant::Action::Send)
    ) || matches!(message, Message::Canvas(edit) if !matches!(edit, crate::canvas::Edit::Hover(_) | crate::canvas::Edit::Pan(..) | crate::canvas::Edit::Zoom(..) | crate::canvas::Edit::BeginTransform(_)))
}

#[cfg(test)]
mod tests;
