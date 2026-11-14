//! Composer menus and the model, reasoning, service-tier and auto-apply preferences.

use super::{Action, Menu};
use crate::app::{App, Message};
use iced::Task;

/// What Settings shows of the assistant preferences.
pub(in crate::app) struct Choices<'a> {
    /// The account's models; empty until it connects.
    pub models: &'a [reshiki::assistant::codex::Model],
    /// The chosen model id; None uses the default.
    pub model: Option<&'a str>,
    /// The model in use, whose reasoning levels Settings lists.
    pub resolved: Option<&'a reshiki::assistant::codex::Model>,
    pub effort: Option<&'a str>,
    pub auto_apply: bool,
    /// Why the last save failed.
    pub save_error: Option<&'a str>,
}

impl App {
    pub(in crate::app) fn assistant_choices(&self) -> Choices<'_> {
        let state = &self.assistant;
        let resolved = state.model();
        Choices {
            models: state.account.as_ref().map_or(&[], |a| &a.models),
            model: state.preferences.model.as_deref(),
            resolved,
            effort: resolved.map(|model| state.preferences.effort(model)),
            auto_apply: state.preferences.auto_apply,
            save_error: state.preferences_error.as_deref(),
        }
    }

    pub(super) fn assistant_auto_apply(&mut self, value: bool) -> Task<Message> {
        if !value {
            self.assistant.waiting_for_canvas_edit = false;
        }
        self.assistant.preferences.auto_apply = value;
        self.assistant.preferences_dirty = true;
        self.assistant.menu = None;
        if value
            && !self.assistant.requires_apply
            && self
                .assistant
                .draft
                .as_ref()
                .is_some_and(|d| d.review.can_auto_apply())
            && !self.assistant.busy
        {
            return self.assistant_action(Action::Apply);
        }
        Task::none()
    }
    pub(super) fn assistant_set_model(&mut self, value: Option<String>) {
        self.assistant.preferences.model = value;
        self.assistant.preferences_dirty = true;
        self.assistant.menu = None;
    }
    pub(super) fn assistant_toggle_menu(&mut self, value: Option<Menu>) {
        self.assistant.menu = if self.assistant.menu == value {
            None
        } else {
            value
        };
        self.assistant.search.clear();
    }
    pub(super) fn assistant_set_effort(&mut self, value: String) {
        if let Some(id) = self.assistant.model().map(|m| m.id.clone()) {
            self.assistant.preferences.efforts.insert(id, value);
            self.assistant.preferences_dirty = true;
        }
        self.assistant.menu = None;
    }
    pub(super) fn assistant_set_tier(&mut self, value: String) {
        if let Some(id) = self.assistant.model().map(|m| m.id.clone()) {
            self.assistant.preferences.tiers.insert(id, value);
            self.assistant.preferences_dirty = true;
        }
        self.assistant.menu = None;
    }
    pub(super) fn assistant_preferences_saved(&mut self, result: Result<(), String>) {
        self.assistant.preferences_saving = false;
        self.assistant.preferences_error = result
            .err()
            .map(|error| format!("Could not save assistant preferences: {error}"));
        if let Some(error) = &self.assistant.preferences_error {
            self.assistant.status = error.clone();
            self.assistant.error = true;
        }
    }
}
