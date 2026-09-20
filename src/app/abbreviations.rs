use super::{App, InspectorTab, Job, Message, Request, Tool};
use iced::Task;

#[derive(Debug, Clone)]
pub enum Action {
    Preset(String),
    Label(String),
    ReverseLabel(String),
    Replace,
    Find,
    Contract,
    Expand,
    ExpandAll,
}
pub struct State {
    pub preset: String,
    pub label: String,
    pub reverse_label: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            preset: "OMe".into(),
            label: String::new(),
            reverse_label: String::new(),
        }
    }
}
impl App {
    pub(super) fn abbreviation_action(&mut self, action: Action) -> Task<Message> {
        let before = self.doc.clone();
        match action {
            Action::Preset(value) => self.abbreviations.preset = value,
            Action::Label(value) => self.abbreviations.label = value,
            Action::ReverseLabel(value) => self.abbreviations.reverse_label = value,
            Action::Replace | Action::Find => {
                let replace = matches!(action, Action::Replace);
                let mut request = Request::molecule("abbreviate", self.doc.clone());
                request.selected_ids = Some(self.selected.clone());
                request.format = Some(if replace { "replace" } else { "find" }.into());
                request.text = replace.then(|| self.abbreviations.preset.clone());
                return self.run(request, Job::Abbreviate);
            }
            Action::Contract => {
                if let Err(error) = self.doc.contract(
                    &self.selected,
                    &self.abbreviations.label,
                    &self.abbreviations.reverse_label,
                ) {
                    self.status = error;
                    self.error = true;
                    return Task::none();
                }
                self.changed(before);
                self.tool = Tool::Select;
                self.status = "Fragment abbreviated · All atoms and bonds retained · Expand restores the drawing".into();
            }
            Action::Expand | Action::ExpandAll => {
                let ids = if matches!(action, Action::ExpandAll) {
                    self.doc.all_ids()
                } else {
                    self.selected.clone()
                };
                let count = self.doc.expand_abbreviations(&ids);
                self.changed(before);
                self.status = format!(
                    "Expanded {count} abbreviation{}",
                    if count == 1 { "" } else { "s" }
                );
                self.tool = Tool::Select;
            }
        }
        self.inspector_tab = InspectorTab::Abbreviations;
        Task::none()
    }
}
