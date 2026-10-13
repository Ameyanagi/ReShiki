use super::{
    App, Message,
    inline_text::{CaptionHighlighter, editor_range, perform_editor_action, script_key},
    workspace::{control, hover_keys, muted_text},
};
use crate::canvas::layered::canvas;
use iced::widget::{
    Space, column, container, mouse_area, opaque, row, stack, text, text_editor, tooltip,
};
use iced::{Element, Length, Task};
use reshiki::abbreviations::LabelAlignment;
use reshiki::atom_text::{self, Mode};
use reshiki::typography::{Script, ScriptRun, StyleChange, TextFormat, TextStyle};

#[derive(Debug, Clone)]
pub enum Action {
    Begin(Option<u64>),
    Input(String),
    Editor(text_editor::Action),
    Script(Script),
    Mode(Mode),
    ContractSelection,
    ReverseInput(String),
    Apply,
    Cancel,
}
pub struct State {
    id: u64,
    epoch: u64,
    input: String,
    editor: text_editor::Content,
    format: TextFormat,
    mode: Mode,
    label_edited: bool,
    scripts_edited: bool,
    manual_scripts: bool,
    alignment: LabelAlignment,
    members: Option<Vec<u64>>,
    reverse: String,
    reverse_edited: bool,
    error: Option<String>,
}
impl State {
    fn scripts_enabled(&self) -> bool {
        self.members.is_none() && self.mode != Mode::Group
    }

    fn scripts(&self) -> Vec<ScriptRun> {
        if !self.manual_scripts || !self.scripts_enabled() || self.input.is_empty() {
            return vec![];
        }
        let mut runs: Vec<ScriptRun> = vec![];
        for (start, character) in self.input.char_indices() {
            let script = self.format.at(start).script;
            let end = start + character.len_utf8();
            if let Some(last) = runs.last_mut()
                && last.end == start
                && last.script == script
            {
                last.end = end;
            } else {
                runs.push(ScriptRun { start, end, script });
            }
        }
        // The draft owns valid ranges; all-Normal coverage is the explicit
        // Plain marker, while gaps in other manual runs are Normal.
        reshiki::typography::normalize_label_scripts(&self.input, &runs).unwrap_or(runs)
    }

    fn selected_script(&self, script: Script) -> bool {
        let range = editor_range(&self.editor, &self.input).unwrap_or(0..self.input.len());
        !range.is_empty()
            && self
                .input
                .char_indices()
                .filter(|(offset, _)| range.contains(offset))
                .all(|(offset, _)| self.format.at(offset).script == script)
    }

    fn editor_action(&mut self, action: text_editor::Action) {
        let action = match action {
            text_editor::Action::Edit(text_editor::Edit::Paste(value)) => {
                text_editor::Action::Edit(text_editor::Edit::Paste(single_line(&value).into()))
            }
            // Return is a popup command, never a line break in the label.
            text_editor::Action::Edit(
                text_editor::Edit::Enter | text_editor::Edit::Insert('\r' | '\n'),
            ) => return,
            action => action,
        };
        if let Some((text, replaced)) = perform_editor_action(&mut self.editor, &self.input, action)
        {
            self.label_edited |= self.input != text;
            if self.manual_scripts {
                self.format.edited(&self.input, &text, replaced);
            } else {
                self.format = label_format(
                    &text,
                    self.format.style.clone(),
                    reshiki::typography::label_runs(&text),
                );
            }
            self.input = text;
            self.error = None;
        }
    }

    fn replace_input(&mut self, value: String) {
        let previous = self.input.clone();
        let previous_format = self.manual_scripts.then(|| self.format.clone());
        self.editor.perform(text_editor::Action::SelectAll);
        self.editor_action(text_editor::Action::Edit(text_editor::Edit::Paste(
            value.into(),
        )));
        if let Some(mut format) = previous_format {
            // Native accessibility supplies the complete updated value rather
            // than an editor selection. Preserve its unchanged text via the
            // format's diff fallback; an appended value inherits the last run.
            format.edited(&previous, &self.input, previous.len()..previous.len());
            self.format = format;
        }
    }

    fn apply_script(&mut self, script: Script) {
        if !self.scripts_enabled() {
            return;
        }
        // Like ChemDraw, the first ⌘−/⌘+ sets the script even where the
        // automatic look already shows it; only a manual script toggles off.
        let script =
            if script != Script::Normal && self.manual_scripts && self.selected_script(script) {
                Script::Normal
            } else {
                script
            };
        self.format.apply(
            &self.input,
            editor_range(&self.editor, &self.input),
            &StyleChange::Script(script),
        );
        self.manual_scripts = true;
        self.scripts_edited = true;
        self.error = None;
    }
}

/// Smallest text size in the Appearance preview of a formatted label.
const PREVIEW_SIZE_PT: f32 = 16.;

fn single_line(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n'))
        .collect()
}

fn label_format(
    text: &str,
    mut style: TextStyle,
    runs: Vec<(std::ops::Range<usize>, Script)>,
) -> TextFormat {
    style.script = Script::Normal;
    style.formula = false;
    let mut format = TextFormat {
        style,
        ..Default::default()
    };
    for (range, script) in runs {
        if script != Script::Normal {
            format.apply(text, Some(range), &StyleChange::Script(script));
        }
    }
    format
}

fn key_binding(key: text_editor::KeyPress) -> Option<text_editor::Binding<Message>> {
    if !matches!(key.status, text_editor::Status::Focused { .. }) {
        return text_editor::Binding::from_key_press(key);
    }
    use iced::keyboard::{Key, key::Named};
    let action = match &key.key {
        Key::Named(Named::Escape) => Some(Action::Cancel),
        Key::Named(Named::Enter) => Some(Action::Apply),
        Key::Character(c) if key.modifiers.command() && c.eq_ignore_ascii_case("t") => {
            Some(Action::Script(Script::Normal))
        }
        _ => script_key(&key.key, key.modifiers).map(Action::Script),
    };
    // Group drafts consume the shortcut too; apply_script leaves them intact.
    action
        .map(|action| text_editor::Binding::Custom(Message::AtomText(action)))
        .or_else(|| text_editor::Binding::from_key_press(key))
}
impl App {
    pub(super) fn atom_text_target(&self) -> Option<u64> {
        if let [id] = self.tab.selected.as_slice()
            && self.tab.doc.atom(*id).is_some()
        {
            return Some(*id);
        }
        self.tab
            .doc
            .abbreviations
            .iter()
            .find(|g| {
                g.members.len() == self.tab.selected.len()
                    && g.members.iter().all(|id| self.tab.selected.contains(id))
            })
            .map(|g| g.anchor)
    }
    pub(super) fn atom_text_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Begin(id) => {
                if self.tab.cleanup.is_some() || self.tab.busy || !self.finish_inline(true) {
                    return Task::none();
                }
                let Some(id) = id.or_else(|| self.atom_text_target()) else {
                    self.status = "Select one atom, then press Enter to edit its label".into();
                    return Task::none();
                };
                let Some(atom) = self.tab.doc.atom(id) else {
                    return Task::none();
                };
                if !atom.centroid.is_empty() && self.tab.doc.abbreviation(id).is_none() {
                    self.status =
                        "This is a tracked attachment point. Edit the atom bonded to it instead."
                            .into();
                    return Task::none();
                }
                let group = self.tab.doc.abbreviation(id);
                let input = group
                    .map(|g| g.label.clone())
                    .unwrap_or_else(|| atom_text::entry(atom));
                let mut editor = text_editor::Content::with_text(&input);
                editor.perform(text_editor::Action::SelectAll);
                let runs = if group.is_none() && atom.display.variable.is_some() {
                    reshiki::typography::effective_label_runs(&atom.display)
                } else {
                    reshiki::typography::label_runs(&input)
                };
                let format = label_format(
                    &input,
                    atom.text_style
                        .clone()
                        .unwrap_or_else(|| self.tab.doc.drawing_style.text_style()),
                    runs,
                );
                self.tab.atom_text = Some(State {
                    id,
                    epoch: self.tab.file_epoch,
                    input,
                    editor,
                    format,
                    mode: if group.is_some() {
                        Mode::Group
                    } else if atom.display.variable.is_some() {
                        Mode::Text
                    } else {
                        Mode::Auto
                    },
                    error: None,
                    label_edited: false,
                    scripts_edited: false,
                    manual_scripts: group.is_none() && !atom.display.label_scripts.is_empty(),
                    alignment: group.map(|g| g.alignment).unwrap_or_default(),
                    members: None,
                    reverse: group.map(|g| g.reverse_label.clone()).unwrap_or_default(),
                    reverse_edited: false,
                });
                self.context_menu = None;
                return iced::widget::operation::focus("atom-text");
            }
            Action::Input(input) => {
                if let Some(state) = &mut self.tab.atom_text {
                    state.replace_input(input);
                }
            }
            Action::Editor(action) => {
                if let Some(state) = &mut self.tab.atom_text {
                    state.editor_action(action);
                }
            }
            Action::Script(script) => {
                if let Some(state) = &mut self.tab.atom_text {
                    state.apply_script(script);
                }
                return iced::widget::operation::focus("atom-text");
            }
            Action::ContractSelection => {
                if self.tab.cleanup.is_some() || self.tab.busy || !self.finish_inline(true) {
                    return Task::none();
                }
                let mut candidate = self.tab.doc.clone();
                match candidate.contract(&self.tab.selected, "Group", "") {
                    Ok(()) => {
                        if let Some(group) = candidate.abbreviations.last() {
                            self.tab.atom_text = Some(State {
                                id: group.anchor,
                                epoch: self.tab.file_epoch,
                                input: String::new(),
                                editor: text_editor::Content::new(),
                                format: TextFormat::default(),
                                mode: Mode::Group,
                                label_edited: true,
                                scripts_edited: false,
                                manual_scripts: false,
                                alignment: LabelAlignment::Auto,
                                members: Some(group.members.clone()),
                                reverse: String::new(),
                                reverse_edited: false,
                                error: None,
                            });
                            self.context_menu = None;
                            return iced::widget::operation::focus("atom-text");
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
            Action::ReverseInput(value) => {
                if let Some(state) = &mut self.tab.atom_text {
                    state.reverse = value;
                    state.reverse_edited = true;
                    state.error = None;
                }
            }
            Action::Mode(mode) => {
                if let Some(state) = &mut self.tab.atom_text {
                    state.label_edited |= state.mode != mode;
                    state.mode = mode;
                    state.error = None;
                }
            }
            Action::Cancel => self.tab.atom_text = None,
            Action::Apply => {
                let Some(state) = &self.tab.atom_text else {
                    return Task::none();
                };
                let result = if state.epoch != self.tab.file_epoch {
                    Err("The drawing changed. Cancel and select the atom again.".into())
                } else if !state.label_edited && !state.scripts_edited {
                    Ok(self.tab.doc.clone())
                } else if let Some(members) = &state.members {
                    let mut doc = self.tab.doc.clone();
                    doc.contract(members, &state.input, &state.reverse)
                        .map(|()| doc)
                } else {
                    atom_text::apply_with_scripts(
                        &self.tab.doc,
                        state.id,
                        &state.input,
                        &state.scripts(),
                        state.mode,
                    )
                };
                match result {
                    Ok(mut doc) => {
                        if let Some(group) =
                            doc.abbreviations.iter_mut().find(|g| g.anchor == state.id)
                        {
                            group.alignment = state.alignment;
                            if state.members.is_some()
                                || state.reverse_edited
                                || self
                                    .tab
                                    .doc
                                    .abbreviation(state.id)
                                    .is_some_and(|g| g.label == group.label)
                            {
                                group.reverse_label = state.reverse.trim().into();
                            }
                        }
                        if let Err(error) = doc.validate() {
                            if let Some(state) = &mut self.tab.atom_text {
                                state.error = Some(error);
                            }
                            return Task::none();
                        }
                        let before = std::mem::replace(&mut self.tab.doc, doc);
                        self.tab.atom_text = None;
                        self.changed(before);
                    }
                    Err(error) => {
                        if let Some(state) = &mut self.tab.atom_text {
                            state.error = Some(error);
                        }
                    }
                }
            }
        }
        Task::none()
    }
    pub(super) fn with_atom_text<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(state) = &self.tab.atom_text else {
            return base;
        };
        let base = reshiki::accessibility::inert(base);
        let editor = text_editor(&state.editor)
            .id("atom-text")
            .font(iced::Font::with_name(reshiki::style::font_name(
                &state.format.style.family,
            )))
            .size(18)
            .padding(10)
            .wrapping(iced::widget::text::Wrapping::None)
            .placeholder("N, NH3, C2H5, Boc, Cp*, M…")
            .on_action(|action| Message::AtomText(Action::Editor(action)))
            .key_binding(key_binding)
            // The label's fonts, in the field's own text color for both themes.
            .highlight_with::<CaptionHighlighter>(
                (state.input.clone(), state.format.clone()),
                |style, _| iced::advanced::text::highlighter::Format {
                    color: None,
                    font: Some(iced::Font {
                        family: iced::font::Family::Name(reshiki::style::font_name(&style.family)),
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
                },
            )
            .style(crate::appearance::editor_style);
        let editor = reshiki::accessibility::editor(
            "atom-text",
            "Atom or group label",
            state.input.clone(),
            editor,
            |value| Message::AtomText(Action::Input(value)),
        );
        let script_control = |id: &'static str,
                              name: &'static str,
                              label: &'static str,
                              key: &'static str,
                              script| {
            hover_keys(
                reshiki::accessibility::button(id, name, text(label).size(13))
                    .on_press_maybe(
                        state
                            .scripts_enabled()
                            .then_some(Message::AtomText(Action::Script(script))),
                    )
                    .checked(state.scripts_enabled() && state.selected_script(script))
                    .style(control(
                        state.scripts_enabled() && state.selected_script(script),
                    ))
                    .padding([5, 9]),
                name,
                Some(super::shortcuts::keys(
                    iced::keyboard::Modifiers::COMMAND,
                    key,
                )),
                tooltip::Position::Top,
            )
        };
        let mut content = column![
            text(if state.members.is_some() {
                "Create group label"
            } else {
                "Edit atom label"
            })
            .size(20),
            editor,
            row![
                script_control(
                    "atom-text-subscript",
                    "Subscript",
                    "X₂",
                    "-",
                    Script::Subscript
                ),
                script_control(
                    "atom-text-superscript",
                    "Superscript",
                    "X²",
                    "+",
                    Script::Superscript
                ),
                script_control(
                    "atom-text-plain",
                    "Plain text",
                    "Plain",
                    "T",
                    Script::Normal
                ),
            ]
            .spacing(6),
        ]
        .spacing(12);
        if state.manual_scripts && state.scripts_enabled() {
            let mut preview = reshiki::document::Document {
                drawing_style: self.tab.doc.drawing_style.clone(),
                canvas_theme: self.tab.doc.canvas_theme,
                color_theme: self.tab.doc.color_theme,
                ..Default::default()
            };
            let id = preview.add_atom("*", reshiki::document::Point::default());
            if let Some(atom) = preview.atom_mut(id) {
                atom.display.variable = Some(state.input.clone());
                atom.display.label_scripts = state.scripts();
                // Enlarged so small raised and lowered characters stay legible.
                let mut style = state.format.style.clone();
                style.size_pt = style.size_pt.max(PREVIEW_SIZE_PT);
                atom.text_style = Some(style);
            }
            content = content.push(
                column![
                    text("Appearance").size(10).style(muted_text),
                    canvas(crate::canvas::OwnedDrawingPreview(preview))
                        .width(Length::Fill)
                        .height(52),
                ]
                .spacing(3),
            );
        }
        if state.members.is_none() {
            content = content.push(
                crate::appearance::pick_list(
                    [Mode::Auto, Mode::Text, Mode::Group],
                    Some(state.mode),
                    |mode| Message::AtomText(Action::Mode(mode)),
                )
                .width(Length::Fill),
            );
        }
        content = content.push(text(if state.members.is_some() {
                "The selected atoms define this group. Its formula and bonds are retained; expand it to edit the structure."
            } else { atom_text::description(&state.input, state.mode) })
                .size(12)
                .style(muted_text));
        if state.members.is_some() || self.tab.doc.abbreviation(state.id).is_some() {
            content = content.push(
                reshiki::accessibility::text_input(
                    "atom-text-reverse",
                    "Label when facing left (optional)",
                    "Label when facing left (optional)",
                    &state.reverse,
                )
                .style(crate::appearance::input_style)
                .on_input(|s| Message::AtomText(Action::ReverseInput(s)))
                .on_submit(Message::AtomText(Action::Apply))
                .padding(8),
            );
        }
        if let Some(error) = &state.error {
            content = content.push(text(error).size(12).style(crate::appearance::text_color(
                iced::Color::from_rgb8(175, 54, 54),
            )));
        }
        let popup = container(
            content.push(
                row![
                    Space::new().width(Length::Fill),
                    reshiki::accessibility::button(
                        "atom-text-cancel",
                        "Cancel atom label edit",
                        "Cancel · Esc"
                    )
                    .on_press(Message::AtomText(Action::Cancel))
                    .style(control(false)),
                    reshiki::accessibility::button(
                        "atom-text-apply",
                        "Apply atom label",
                        "Apply · Enter"
                    )
                    .on_press(Message::AtomText(Action::Apply))
                    .style(crate::appearance::primary),
                ]
                .spacing(8),
            ),
        )
        .padding(22)
        .width(430)
        .style(|theme| {
            crate::appearance::container(
                theme,
                container::Style {
                    background: Some(iced::Color::WHITE.into()),
                    border: iced::Border {
                        radius: 12.into(),
                        width: 1.,
                        color: iced::Color::from_rgb8(213, 224, 220),
                    },
                    ..Default::default()
                },
            )
        });
        stack![
            base,
            opaque(
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|theme| crate::appearance::container(
                            theme,
                            container::Style {
                                background: Some(iced::Color::from_rgba8(20, 30, 30, 0.3).into()),
                                ..Default::default()
                            }
                        ))
                )
                .on_press(Message::AtomText(Action::Cancel))
            ),
            container(opaque(popup)).center(Length::Fill)
        ]
        .into()
    }
}

// A modal draft must not let unhandled keyboard shortcuts edit the canvas or
// replace the document. Background work and close/save completion still run.
pub(super) fn background(message: &Message) -> bool {
    matches!(
        message,
        Message::Tick
            | Message::Viewport(_)
            | Message::Assistant(_)
            | Message::Updates(_)
            | Message::Close(_)
            | Message::Cancel
            | Message::Discard
    ) || super::results::admitted(super::results::Modal::AtomText, message)
}

#[cfg(test)]
mod tests;
