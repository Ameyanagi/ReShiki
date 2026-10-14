//! An Office save target belongs to its document tab, not to the whole window.
use super::{App, Message, document_tab::TabId, files};
use crate::session::{Binding, Phase};
use iced::Task;

#[derive(Default)]
pub(super) struct State {
    // Keep the tab while its write settles. A failed/cancelled save cancels this
    // close, so its Office binding and drawing remain available for retry.
    pub(super) closing: Vec<TabId>,
}

impl App {
    pub(super) fn detach_office(&mut self) {
        self.tab.detach_office();
    }

    pub(super) fn office_path_conflict(&self, binding: &Binding) -> bool {
        self.strip().any(|tab| {
            tab.path
                .as_ref()
                .is_some_and(|path| binding.same_path(path))
                && (tab.office.is_some()
                    || self.edited(tab)
                    || self.file_io.saving && self.file_io.saving_tab == Some(tab.id))
        })
    }

    pub(super) fn finish_office_closes(&mut self) -> Task<Message> {
        if self.exit.frozen() || self.pending.is_some() || self.file_io.saving {
            return Task::none();
        }
        let Some(id) = self.office.closing.first().copied() else {
            return Task::none();
        };
        self.office.closing.remove(0);
        self.tab_action(super::tabs::Action::Close(Some(id)))
    }

    pub(super) fn office_prepared(
        &mut self,
        binding: Binding,
        opened: files::Opened,
    ) -> Task<Message> {
        let Some((path, result)) = opened else {
            binding.reject();
            return Task::none();
        };
        let result = match result {
            Ok(files::Prepared::Native(doc)) => Ok(doc),
            Ok(_) => Err("Office editing requires a native ReShiki drawing".into()),
            Err(error) => Err(error),
        };
        let conflict = self.office_path_conflict(&binding);
        if self.exit.frozen() || self.updates.restarting || conflict || !binding.preparable() {
            binding.reject();
            self.status =
                "This drawing already belongs to another edit session, or the editor is closing"
                    .into();
            self.error = true;
            return Task::none();
        }
        match result {
            Ok(doc) => {
                if path != binding.path {
                    binding.reject();
                    self.status = "Office edit source does not match the opened file".into();
                    self.error = true;
                    return Task::none();
                }
                if let Some(lease) = &binding.lease
                    && !lease.transition(Phase::Prepared, Phase::Open)
                {
                    return Task::none();
                }
                let task = self.file_prepared(Some((path, Ok(files::Prepared::Native(doc)))));
                self.tab.path = Some(binding.path.clone());
                self.tab.office = Some(binding);
                task
            }
            Err(error) => {
                binding.reject();
                self.status = error;
                self.error = true;
                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests;
