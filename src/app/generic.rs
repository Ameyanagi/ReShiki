//! Finite expansion creates a new drawing; the source and its history stay intact.
use super::inspector::Section;
use super::workspace::{accessible_command, hover_hint, muted_text};
use super::{App, Message, Tool, document_tab::TabId};
use iced::widget::{Row, column, container, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::generic::{Limits, PreparedExpansion, RepeatPattern};

pub(super) const EXPANSION_DISCARDED: &str =
    "Generic expansion was discarded because another edit was open; expand again";

pub(super) const EXPAND: &str = "Expand generic structures";
const EXPAND_HINT: &str = "Opens all concrete structures together in one new tab";
const NOTHING_TO_EXPAND: &str = "This drawing has no generic definitions";
const EXPANSION_RUNNING: &str = "An expansion is already running";
const AFTER_EXPANSION: &str = "Labels and properties are calculated after expansion.";
const DEFINE_HINT: &str = "Records the enclosed atoms and the existing crossing bonds";
const CLEAR_HINT: &str = "Removes the definition; the bracket stays";
const NOTHING_TO_CLEAR: &str = "This bracket has no repeating unit";
const REPEAT_HINT: &str = "Use n for an indefinite polymer.";
/// The Repeats field and the Pattern list share one column.
const FIELD_WIDTH: f32 = 140.;

#[derive(Debug, Clone)]
pub enum Action {
    Expand,
    ExpandComponent(Vec<u64>),
    Expanded {
        source: TabId,
        epoch: u64,
        result: Box<Result<PreparedExpansion, String>>,
    },
    RepeatLabel(String),
    Pattern(RepeatPattern),
    DefineRepeat(u64),
    ClearRepeat(u64),
}
pub struct State {
    pub label: String,
    pub pattern: RepeatPattern,
    pending: Option<(TabId, u64)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            label: "n".into(),
            pattern: RepeatPattern::HeadToTail,
            pending: None,
        }
    }
}
#[cfg(test)]
impl State {
    pub(super) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PatternChoice(RepeatPattern);
impl std::fmt::Display for PatternChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            RepeatPattern::HeadToTail => "Head to tail",
            RepeatPattern::HeadToHead => "Head to head",
            RepeatPattern::EitherUnknown => "Mixed / unknown",
        })
    }
}
impl App {
    /// These are exactly the states whose later gates drop the expansion result.
    pub(super) fn generic_expansion_blocked(&self) -> bool {
        self.tab.atom_text.is_some()
            || self.tab.cleanup.is_some()
            || self.tab.optimization.is_some()
            || self.updates.open
            || self.updates.restarting
            || self.exit.frozen()
    }

    pub(super) fn discard_generic_expansion(&mut self, source: TabId, epoch: u64) {
        if self.generic.pending == Some((source, epoch)) {
            self.generic.pending = None;
            // A replaced source keeps its newer status.
            self.in_tab(source, |app| {
                if app.tab.file_epoch == epoch {
                    app.status = EXPANSION_DISCARDED.into();
                    app.error = false;
                }
            });
            self.status = EXPANSION_DISCARDED.into();
            self.error = false;
        }
    }

    pub(super) fn generic_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::RepeatLabel(label) => self.generic.label = label,
            Action::Pattern(pattern) => self.generic.pattern = pattern,
            Action::DefineRepeat(bracket) => {
                match reshiki::generic::define_bracket(
                    &self.tab.doc,
                    bracket,
                    &self.generic.label,
                    self.generic.pattern,
                ) {
                    Ok(doc) => {
                        let before = self.tab.doc.clone();
                        self.tab.doc = doc;
                        self.changed(before);
                        self.error = false;
                        self.status =
                            "Repeating unit defined from enclosed atoms and crossing bonds".into();
                    }
                    Err(error) => return self.generic_failed(error),
                }
            }
            Action::ClearRepeat(bracket) => {
                let before = self.tab.doc.clone();
                self.tab.doc.generic.repeats.retain(|u| {
                    u.bracket != Some(bracket) && !u.bracket_objects.contains(&bracket)
                });
                self.changed(before);
                self.status = "Repeating-unit definition cleared".into();
                self.error = false;
            }
            Action::Expand | Action::ExpandComponent(_) => {
                if self.generic.pending.is_some() {
                    return Task::none();
                }
                // Borrow first: no input payload is copied or scheduled until
                // its shared presentation/selection allowance has been paid.
                let prepared_input = match action {
                    Action::ExpandComponent(hit) => {
                        reshiki::generic::prepare_component_expansion_input(
                            &self.tab.doc,
                            &hit,
                            Limits::default(),
                        )
                    }
                    _ => reshiki::generic::prepare_expansion_input(
                        &self.tab.doc,
                        &self.tab.selected,
                        Limits::default(),
                    ),
                };
                let input = match prepared_input {
                    Ok(input) => input,
                    Err(error) => return self.generic_failed(error),
                };
                let key = (self.tab.id, self.tab.file_epoch);
                self.generic.pending = Some(key);
                self.status = "Expanding generic structures…".into();
                self.error = false;
                return Task::perform(
                    async move {
                        reshiki::task::spawn_blocking(move || {
                            reshiki::generic::expand_prepared(input)
                        })
                        .await
                        .map_err(|e| format!("Generic expansion failed: {e}"))
                        .and_then(|r| r)
                    },
                    move |result| {
                        Message::Generic(Action::Expanded {
                            source: key.0,
                            epoch: key.1,
                            result: Box::new(result),
                        })
                    },
                );
            }
            Action::Expanded {
                source,
                epoch,
                result,
            } => {
                if self.generic.pending != Some((source, epoch)) {
                    return Task::none();
                }
                self.generic.pending = None;
                if !self
                    .strip()
                    .any(|tab| tab.id == source && tab.file_epoch == epoch)
                {
                    self.status = "Expansion source was closed or replaced".into();
                    return Task::none();
                }
                // The source tab keeps its own status, which still says the
                // expansion is running, so settle it even when it is behind.
                // A failure is reported in front, as before, and on the source.
                let failed = |app: &mut Self, message: String| {
                    app.in_tab(source, |app| app.generic_failed(message.clone()));
                    app.generic_failed(message)
                };
                match *result {
                    Ok(prepared) => {
                        // Cheap action clones share one capability. Consume before
                        // opening a tab; replay/lock errors must not create an output.
                        let insertion = match reshiki::transaction::take_expansion(prepared) {
                            Ok(insertion) => insertion,
                            Err(error) => return failed(self, error.message().into()),
                        };
                        let count = insertion.count();
                        let bounds = insertion.bounds();
                        self.in_tab(source, |app| {
                            app.status =
                                format!("Expanded {count} concrete structures in a new tab");
                            app.error = false;
                        });
                        self.add_tab();
                        let before = self.tab.doc.clone();
                        let reconciled = match reshiki::transaction::install_expansion(
                            &mut self.tab.doc,
                            before,
                            insertion,
                        ) {
                            Ok(reconciled) => reconciled,
                            Err(error) => return failed(self, error.message().into()),
                        };
                        self.tab.cleanup = None;
                        self.tab.erase_stroke = false;
                        self.tab.untitled_name = Some("Generic expansion".into());
                        self.commit_changed(reconciled, false);
                        self.sync_drawing_defaults();
                        self.tool = Tool::Select;
                        self.fit_bounds(bounds);
                        self.status =
                            format!("Expanded {count} concrete structures in a new drawing");
                        self.error = false;
                    }
                    Err(error) => return failed(self, error),
                }
            }
        }
        Task::none()
    }
    fn generic_failed(&mut self, message: impl Into<String>) -> Task<Message> {
        self.status = message.into();
        self.error = true;
        Task::none()
    }
    /// The drawing has generic parts and no expansion is running. Shared by
    /// the button and the right-click menu entry.
    pub(super) fn generic_expand_enabled(&self) -> bool {
        self.generic.pending.is_none() && reshiki::generic::present(&self.tab.doc)
    }
    fn generic_expand_button(&self) -> Element<'_, Message> {
        let enabled = self.generic_expand_enabled();
        let hint = if enabled {
            EXPAND_HINT
        } else if self.generic.pending.is_some() {
            EXPANSION_RUNNING
        } else {
            NOTHING_TO_EXPAND
        };
        hover_hint(
            accessible_command("expand-generic-structures", EXPAND, EXPAND)
                .on_press_maybe(enabled.then_some(Message::Generic(Action::Expand))),
            hint,
            tooltip::Position::Top,
        )
        .into()
    }
    /// The Properties card of a drawing with generic parts: its labels and
    /// properties wait for the expansion, which the card offers.
    pub(super) fn generic_structure_section(&self) -> Option<Element<'_, Message>> {
        if !reshiki::generic::present(&self.tab.doc) {
            return None;
        }
        Some(
            self.inspector_section(
                Section::GenericStructure,
                "Generic structure",
                "",
                true,
                column![
                    self.generic_expand_button(),
                    text(AFTER_EXPANSION).size(11).style(muted_text),
                ]
                .spacing(6),
            ),
        )
    }
    /// The repeating unit of the one selected bracket, as its own section
    /// after the Graphic style card.
    pub(super) fn repeating_unit_section(&self) -> Option<Element<'_, Message>> {
        let mut selected = self
            .tab
            .doc
            .graphics
            .iter()
            .filter(|g| self.tab.selected.contains(&g.id) && g.picture.is_none());
        let (Some(bracket), None) = (selected.next(), selected.next()) else {
            return None;
        };
        if !bracket.kind.brackets() {
            return None;
        }
        let id = bracket.id;
        let defined = self
            .tab
            .doc
            .generic
            .repeats
            .iter()
            .any(|u| u.bracket == Some(id) || u.bracket_objects.contains(&id));
        Some(
            self.inspector_section(
                Section::RepeatingUnit,
                "Repeating unit",
                "",
                true,
                column![
                    self.repeat_count_row(id),
                    self.pattern_row(),
                    self.define_repeat_button(id),
                    self.clear_repeat_button(defined, id),
                    text(REPEAT_HINT).size(11).style(muted_text),
                ]
                .spacing(6),
            ),
        )
    }
    fn repeat_count_row(&self, id: u64) -> Row<'_, Message> {
        let field = reshiki::accessibility::text_input(
            "polymer-repeat-count",
            "Polymer repeat count or label",
            "1-3 or n",
            &self.generic.label,
        )
        .style(crate::appearance::input_style)
        .on_input(|s| Message::Generic(Action::RepeatLabel(s)))
        .on_submit(Message::Generic(Action::DefineRepeat(id)))
        .size(12)
        .padding(6);
        labeled("Repeats", field)
    }
    fn pattern_row(&self) -> Row<'_, Message> {
        let patterns = crate::appearance::pick_list(
            [
                PatternChoice(RepeatPattern::HeadToTail),
                PatternChoice(RepeatPattern::HeadToHead),
                PatternChoice(RepeatPattern::EitherUnknown),
            ],
            Some(PatternChoice(self.generic.pattern)),
            |v| Message::Generic(Action::Pattern(v.0)),
        )
        .text_size(12)
        .padding(6)
        .width(Length::Fill);
        labeled("Pattern", patterns)
    }
    fn define_repeat_button(&self, id: u64) -> Element<'_, Message> {
        let label = "Define repeating unit";
        hover_hint(
            accessible_command("define-polymer-repeat", label, label)
                .on_press(Message::Generic(Action::DefineRepeat(id))),
            DEFINE_HINT,
            tooltip::Position::Top,
        )
        .into()
    }
    fn clear_repeat_button(&self, defined: bool, id: u64) -> Element<'_, Message> {
        let label = "Clear repeating unit";
        hover_hint(
            accessible_command("clear-polymer-repeat", label, label)
                .on_press_maybe(defined.then_some(Message::Generic(Action::ClearRepeat(id)))),
            if defined {
                CLEAR_HINT
            } else {
                NOTHING_TO_CLEAR
            },
            tooltip::Position::Top,
        )
        .into()
    }
}

/// A label and its field on one centered row; fields share one column.
fn labeled<'a>(label: &'a str, field: impl Into<Element<'a, Message>>) -> Row<'a, Message> {
    row![
        text(label).size(12).width(Length::Fill),
        container(field).width(FIELD_WIDTH),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
}

#[cfg(test)]
mod tests;
