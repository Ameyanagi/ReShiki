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
        let before = self.tab.doc.clone();
        match action {
            Action::Preset(value) => self.abbreviations.preset = value,
            Action::Label(value) => self.abbreviations.label = value,
            Action::ReverseLabel(value) => self.abbreviations.reverse_label = value,
            Action::Replace | Action::Find => {
                let replace = matches!(action, Action::Replace);
                if replace && reshiki::ligands::LABELS.contains(&self.abbreviations.preset.as_str())
                {
                    let anchor = self
                        .tab
                        .doc
                        .abbreviations
                        .iter()
                        .find(|g| {
                            !self.tab.selected.is_empty()
                                && self.tab.selected.iter().all(|id| g.members.contains(id))
                        })
                        .map(|g| g.anchor)
                        .or_else(|| {
                            (self.tab.selected.len() == 1)
                                .then(|| self.tab.selected.first().copied())
                                .flatten()
                        });
                    match anchor
                        .ok_or_else(|| "Select one endpoint or an existing group".to_string())
                        .and_then(|id| {
                            reshiki::ligands::replace(&self.tab.doc, id, &self.abbreviations.preset)
                        }) {
                        Ok(doc) => {
                            self.tab.doc = doc;
                            self.changed(before);
                            self.tab.selected = anchor.into_iter().collect();
                            self.status = "Ligand abbreviated · Real atoms and five-center attachment retained".into();
                            self.error = false;
                            self.tool = Tool::Select;
                        }
                        Err(error) => {
                            self.status = error;
                            self.error = true;
                        }
                    }
                    return Task::none();
                }
                let mut request = Request::molecule("abbreviate", self.tab.doc.clone());
                request.selected_ids = Some(self.tab.selected.clone());
                request.format = Some(if replace { "replace" } else { "find" }.into());
                request.text = replace.then(|| self.abbreviations.preset.clone());
                return self.run(request, Job::Abbreviate);
            }
            Action::Contract => {
                if let Err(error) = self.tab.doc.contract(
                    &self.tab.selected,
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
                    self.tab.doc.all_ids()
                } else {
                    self.tab.selected.clone()
                };
                let count = self.tab.doc.expand_abbreviations(&ids);
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
