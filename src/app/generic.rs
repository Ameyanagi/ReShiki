//! Finite expansion creates a new drawing; the source and its history stay intact.
use super::{App, Message, Tool, document_tab::TabId};
use iced::widget::{column, row, text};
use iced::{Element, Task};
use reshiki::{
    document::Point,
    generic::{Limits, PreparedExpansion, RepeatPattern},
};
#[derive(Debug, Clone)]
pub enum Action {
    Expand,
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
impl std::fmt::Display for PatternChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            RepeatPattern::HeadToTail => "Head to tail",
            RepeatPattern::HeadToHead => "Head to head",
            RepeatPattern::EitherUnknown => "Mixed / unknown",
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PatternChoice(RepeatPattern);
impl App {
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
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
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
            Action::Expand => {
                if self.generic.pending.is_some() {
                    return Task::none();
                }
                // Borrow first: no input payload is copied or scheduled until
                // its shared presentation/selection allowance has been paid.
                let input = match reshiki::generic::prepare_expansion_input(
                    &self.tab.doc,
                    &self.tab.selected,
                    Limits::default(),
                ) {
                    Ok(input) => input,
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                        return Task::none();
                    }
                };
                let key = (self.tab.id, self.tab.file_epoch);
                self.generic.pending = Some(key);
                self.status = "Expanding generic structures…".into();
                self.error = false;
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
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
                match *result {
                    Ok(prepared) => {
                        // Cheap action clones share one capability. Consume before
                        // opening a tab; replay/lock errors must not create an output.
                        let insertion = match reshiki::transaction::take_expansion(prepared) {
                            Ok(insertion) => insertion,
                            Err(error) => {
                                self.status = error.message().into();
                                self.error = true;
                                return Task::none();
                            }
                        };
                        let count = insertion.count();
                        let bounds = insertion.bounds();
                        self.add_tab();
                        let before = self.tab.doc.clone();
                        let reconciled = match reshiki::transaction::install_expansion(
                            &mut self.tab.doc,
                            before,
                            insertion,
                        ) {
                            Ok(reconciled) => reconciled,
                            Err(error) => {
                                self.status = error.message().into();
                                self.error = true;
                                return Task::none();
                            }
                        };
                        self.tab.cleanup = None;
                        self.tab.erase_stroke = false;
                        self.tab.untitled_name = Some("Generic expansion");
                        self.commit_changed(reconciled, false);
                        self.sync_drawing_defaults();
                        self.tool = Tool::Select;
                        self.fit_prepared_expansion(bounds);
                        self.status =
                            format!("Expanded {count} concrete structures in a new drawing");
                        self.error = false;
                    }
                    Err(error) => {
                        self.status = error;
                        self.error = true;
                    }
                }
            }
        }
        Task::none()
    }
    fn fit_prepared_expansion(&mut self, (lo, hi): (Point, Point)) {
        self.tab.pages.fit = None;
        self.tab.camera.center = Point::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
        let viewport = self.guides.paper(iced::Rectangle::with_size(self.viewport));
        self.tab.camera.zoom = ((viewport.width - 80.).max(100.) / (hi.x - lo.x).max(240.))
            .min((viewport.height - 80.).max(100.) / (hi.y - lo.y).max(200.))
            .clamp(0.005, 2.5);
        self.tab.fit_to_view = true;
    }
    pub(super) fn generic_expand_button(&self) -> Element<'_, Message> {
        reshiki::accessibility::button(
            "expand-generic-structures",
            "Expand Generic Structures",
            text("Expand Generic Structures").size(12),
        )
        .on_press_maybe(
            (self.generic.pending.is_none() && reshiki::generic::present(&self.tab.doc))
                .then_some(Message::Generic(Action::Expand)),
        )
        .padding([7, 9])
        .style(super::workspace::control(false))
        .into()
    }
    pub(super) fn generic_bracket_controls(&self, id: u64) -> Element<'_, Message> {
        let defined = self
            .tab
            .doc
            .generic
            .repeats
            .iter()
            .any(|u| u.bracket == Some(id) || u.bracket_objects.contains(&id));
        column![text("REPEATING UNIT").size(11).style(super::workspace::muted_text),
            row![text("Repeats").size(12),reshiki::accessibility::text_input("polymer-repeat-count","Polymer repeat count or label","1-3 or n",&self.generic.label).on_input(|s|Message::Generic(Action::RepeatLabel(s))).on_submit(Message::Generic(Action::DefineRepeat(id))).size(12).padding(6)].spacing(8),
            crate::appearance::pick_list([PatternChoice(RepeatPattern::HeadToTail),PatternChoice(RepeatPattern::HeadToHead),PatternChoice(RepeatPattern::EitherUnknown)],Some(PatternChoice(self.generic.pattern)),|v|Message::Generic(Action::Pattern(v.0))).text_size(12).padding(6),
            reshiki::accessibility::button("define-polymer-repeat","Use enclosed atoms",text("Use enclosed atoms").size(12)).on_press(Message::Generic(Action::DefineRepeat(id))).padding([7,9]).style(super::workspace::control(false)),
            text("The bracket records its enclosed atoms and the existing crossing bonds. Use n for an indefinite polymer.").size(11).style(super::workspace::muted_text),
            reshiki::accessibility::button("clear-polymer-repeat","Clear repeating unit",text("Clear repeating unit").size(12)).on_press_maybe(defined.then_some(Message::Generic(Action::ClearRepeat(id)))).padding([7,9]).style(super::workspace::control(false)),
            self.generic_expand_button()
        ].spacing(8).into()
    }
}

#[cfg(test)]
mod tests;
