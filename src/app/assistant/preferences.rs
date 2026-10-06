//! Composer menus and the model, reasoning, service-tier and auto-apply preferences.

use super::{Action, Menu};
use crate::app::{App, Message};
use iced::Task;

impl App {
    pub(super) fn assistant_auto_apply(&mut self, value: bool) -> Task<Message> {
        if !value {
            self.assistant.waiting_for_canvas_edit = false;
        }
        self.assistant.preferences.auto_apply = value;
        self.assistant.preferences_dirty = true;
        self.assistant.menu = None;
        if value
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
        if let Err(error) = result {
            self.assistant.status = format!("Could not save assistant preferences: {error}");
            self.assistant.error = true;
        }
    }
}
