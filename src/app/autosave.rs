//! Ordered recovery operations. Only one worker may touch a tab's draft file.
use super::{App, Message, document_tab::DocumentTab, document_tab::TabId};
use iced::Task;
use reshiki::{
    document::Document,
    recovery::{Candidate, Recovery},
};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    serial: u64,
    generation: u64,
    epoch: u64,
    revision: u64,
    clear: bool,
}

/// A document's recovery draft.
#[derive(Default)]
pub(super) struct State {
    serial: u64,
    generation: u64,
    pending: Option<Key>,
    clear: bool,
    requested: bool,
    retire_candidate: Option<PathBuf>,
}

/// A closed tab's draft remains tracked until its removal succeeds.
pub(super) struct Retired {
    id: TabId,
    path: PathBuf,
    pending: bool,
}

impl State {
    pub(super) fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn edited_draft(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// A write or removal waits to start.
    fn ready(&self) -> bool {
        self.pending.is_none() && (self.clear || self.requested)
    }

    /// No write is running or waiting to remove the draft.
    fn settled(&self) -> bool {
        self.pending.is_none() && !self.clear
    }
}

/// A window close or an update restart that waits for the recovery draft.
#[derive(Default)]
pub(super) struct Exit {
    closing: Option<iced::window::Id>,
    updating: bool,
    handoff: bool,
    committed: bool,
}

impl Exit {
    pub(super) fn closing(&self) -> bool {
        self.closing.is_some() || self.updating
    }

    pub(super) fn frozen(&self) -> bool {
        self.closing() || self.handoff || self.committed
    }

    pub(super) fn committed(&self) -> bool {
        self.committed
    }
}

pub(super) enum Work {
    Save {
        document: Box<Document>,
        source: Option<PathBuf>,
        retire_candidate: Option<PathBuf>,
    },
    Clear,
}

pub(super) async fn execute(path: PathBuf, work: Work) -> Result<Option<PathBuf>, String> {
    tokio::task::spawn_blocking(move || {
        let recovery = Recovery { session: path };
        match work {
            Work::Save {
                document,
                source,
                retire_candidate,
            } => {
                recovery.save(&document, source)?;
                // Keep the original recovery candidate until its replacement
                // is durably written. A failed save must never lose it.
                if let Some(path) = &retire_candidate {
                    reshiki::recovery::remove(path)?;
                }
                Ok(retire_candidate)
            }
            Work::Clear => recovery.clear().map(|()| None),
        }
    })
    .await
    .map_err(|e| format!("Recovery worker failed: {e}"))?
}

impl App {
    pub(super) fn cancel_close(&mut self) {
        if self.exit.frozen() {
            self.exit = Exit::default();
            self.each_tab(|app| {
                app.tab.autosave.clear = false;
                app.tab.autosave.requested = true;
            });
        }
    }

    pub(super) fn clear_recovery(&mut self) {
        self.tab.autosave.edited_draft();
        self.tab.autosave.clear = self.tab.recovery.is_some();
        self.tab.autosave.requested = false;
        self.tab.autosave.retire_candidate = None;
        self.tab.autosaved_revision = None;
        self.tab.autosave_status.clear();
    }

    pub(super) fn request_autosave(&mut self) {
        if self.dirty() {
            if self.tab.autosaved_revision != Some(self.tab.revision) {
                self.tab.autosave.requested = true;
            }
        } else {
            self.clear_recovery();
        }
    }

    /// The autosave timer's tick: a draft write or removal for the tab in
    /// front and every other tab whose draft is behind its drawing.
    pub(super) fn request_drafts(&mut self) {
        self.request_autosave();
        let behind: Vec<_> = self
            .tabs
            .background
            .iter()
            .filter(|tab| self.needs_draft(tab))
            .map(|tab| tab.id)
            .collect();
        for id in behind {
            self.in_tab(id, Self::request_autosave);
        }
    }

    /// The autosave timer runs while some tab's draft is behind its drawing.
    pub(super) fn needs_draft(&self, tab: &DocumentTab) -> bool {
        let dirty = self.edited(tab);
        tab.recovery.is_some()
            && !tab.autosave.pending()
            && ((dirty && tab.autosaved_revision != Some(tab.revision))
                || (!dirty && tab.autosaved_revision.is_some()))
    }

    pub(super) fn recover_candidate(&mut self, path: PathBuf) {
        self.tab.autosave.retire_candidate = Some(path);
        self.tab.autosave.requested = true;
        self.tab.autosaved_revision = None;
    }

    pub(super) fn close_after_recovery(&mut self, id: iced::window::Id) -> Task<Message> {
        self.each_tab(Self::pause_optimization);
        self.each_tab(Self::clear_recovery);
        self.exit.closing = Some(id);
        Task::batch([self.retry_retired(), self.start_autosave()])
    }

    pub(super) fn restart_after_recovery(&mut self) -> Task<Message> {
        self.each_tab(Self::pause_optimization);
        self.each_tab(Self::clear_recovery);
        self.exit.updating = true;
        Task::batch([self.retry_retired(), self.start_autosave()])
    }

    /// Starts one draft write or removal for every tab that waits for one.
    pub(super) fn start_autosave(&mut self) -> Task<Message> {
        // Explicit saves and library writes must finish before the process
        // exits. Their completion messages are handled before this gate.
        if self.exit.closing() && (self.file_io.saving || self.templates.pending()) {
            return Task::none();
        }
        let ready: Vec<_> = self
            .strip()
            .filter(|tab| tab.recovery.is_some() && tab.autosave.ready())
            .map(|tab| tab.id)
            .collect();
        let mut tasks = vec![];
        for id in ready {
            if let Some(Some((key, path, work))) = self.in_tab(id, Self::prepare_autosave) {
                tasks.push(Task::perform(execute(path, work), move |result| {
                    Message::Tab(id, Box::new(Message::Autosaved(key, result)))
                }));
            }
        }
        if tasks.is_empty() && self.exit.closing() && self.drafts_settled() {
            return self.finish_exit();
        }
        Task::batch(tasks)
    }

    /// Every draft write and removal has finished, closed tabs' too.
    fn drafts_settled(&self) -> bool {
        self.tabs.retiring.is_empty() && self.strip().all(|tab| tab.autosave.settled())
    }

    fn finish_exit(&mut self) -> Task<Message> {
        if let Some(task) = self.template_close_warning() {
            return task;
        }
        if self.exit.updating {
            self.exit.updating = false;
            // The installer can still fail. Keep results until it confirms
            // the handoff, so cancellation can restore every operation.
            self.exit.handoff = true;
            return Task::done(Message::Updates(super::updates::Action::RecoveryCleared));
        }
        self.commit_exit();
        self.exit
            .closing
            .take()
            .map_or_else(Task::none, |id| self.accessibility_close(id))
    }

    pub(super) fn commit_exit(&mut self) {
        self.exit.handoff = false;
        self.exit.committed = true;
        self.each_tab(Self::detach_office);
        self.office.closing.clear();
        self.tabs.deferred_results.clear();
    }

    /// Removes a closed tab's draft; a write still running for it finishes
    /// first, and its result, which finds no tab, starts the removal.
    pub(super) fn retire(&mut self, mut tab: DocumentTab) -> Task<Message> {
        if let Some(binding) = tab.office.take() {
            binding.finish();
        }
        self.retire_assistant(tab.id);
        let Some(recovery) = tab.recovery else {
            return Task::none();
        };
        self.tabs.retiring.push(Retired {
            id: tab.id,
            path: recovery.session.clone(),
            pending: true,
        });
        if tab.autosave.pending() {
            return Task::none();
        }
        remove_draft(tab.id, recovery.session)
    }

    /// A draft result for a closed tab: its last write finished, so remove
    /// the draft, or the draft is gone.
    pub(super) fn retired(
        &mut self,
        id: TabId,
        key: Key,
        result: Result<Option<PathBuf>, String>,
    ) -> Task<Message> {
        let Some(retired) = self.tabs.retiring.iter_mut().find(|tab| tab.id == id) else {
            return Task::none();
        };
        if key.clear {
            if let Err(error) = result {
                retired.pending = false;
                let updating = self.exit.updating;
                self.cancel_close();
                let error = format!("Could not remove a closed drawing's recovery draft: {error}");
                if updating {
                    self.update_restart_failed(error.clone());
                }
                self.status = error;
                self.error = true;
            } else {
                self.tabs.retiring.retain(|tab| tab.id != id);
            }
            return Task::none();
        }
        // Even a failed final write can have left an older draft on disk.
        remove_draft(id, retired.path.clone())
    }

    /// Retry failures on the next explicit close/restart, never in a hot loop.
    fn retry_retired(&mut self) -> Task<Message> {
        Task::batch(self.tabs.retiring.iter_mut().filter_map(|retired| {
            if retired.pending {
                return None;
            }
            retired.pending = true;
            Some(remove_draft(retired.id, retired.path.clone()))
        }))
    }

    fn prepare_autosave(&mut self) -> Option<(Key, PathBuf, Work)> {
        if self.tab.autosave.pending.is_some() {
            return None;
        }
        let Some(recovery) = &self.tab.recovery else {
            return None;
        };
        let path = recovery.session.clone();
        let clear = self.tab.autosave.clear;
        let work = if clear {
            self.tab.autosave.clear = false;
            Work::Clear
        } else if self.tab.autosave.requested {
            self.tab.autosave.requested = false;
            if !self.dirty() || self.tab.autosaved_revision == Some(self.tab.revision) {
                return None;
            }
            Work::Save {
                document: Box::new(self.recovery_document()),
                source: self.tab.path.clone(),
                retire_candidate: self.tab.autosave.retire_candidate.clone(),
            }
        } else {
            return None;
        };
        self.tab.autosave.serial = self.tab.autosave.serial.wrapping_add(1);
        let key = Key {
            serial: self.tab.autosave.serial,
            generation: self.tab.autosave.generation,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            clear,
        };
        self.tab.autosave.pending = Some(key);
        Some((key, path, work))
    }

    pub(super) fn autosaved(
        &mut self,
        key: Key,
        result: Result<Option<PathBuf>, String>,
    ) -> Task<Message> {
        if self.tab.autosave.pending != Some(key) {
            return Task::none();
        }
        self.tab.autosave.pending = None;
        if let Ok(Some(path)) = &result {
            self.recovered.retain(|candidate| candidate.path != *path);
            if self.tab.autosave.retire_candidate.as_ref() == Some(path) {
                self.tab.autosave.retire_candidate = None;
            }
        }
        if !key.clear && !self.dirty() {
            // Undo or cancelling a text draft can make the drawing clean while
            // its first save is running. A clean drawing with no acknowledged
            // save has no autosave timer, so queue the ordered clear now, even
            // when the completed save's revision or draft generation is stale.
            self.clear_recovery();
            return Task::none();
        }
        if key.generation != self.tab.autosave.generation || key.epoch != self.tab.file_epoch {
            return Task::none();
        }
        if key.clear {
            if result.is_ok()
                && self.exit.closing()
                && (self.file_io.saving || self.templates.pending())
            {
                self.tab.autosave.clear = true;
                return Task::none();
            }
            match result {
                Ok(_) => {
                    if self.exit.closing() && self.drafts_settled() {
                        return self.finish_exit();
                    }
                }
                Err(error) => {
                    let updating = self.exit.updating;
                    self.cancel_close();
                    if updating {
                        self.update_restart_failed(format!(
                            "Could not remove recovery draft: {error}"
                        ));
                    }
                    self.tab.autosave_status = format!("Could not remove recovery draft: {error}");
                }
            }
        } else if key.revision == self.tab.revision {
            match result {
                Ok(_) => {
                    self.tab.autosaved_revision = Some(self.tab.revision);
                    self.tab.autosave_status = "Recovery draft saved".into();
                }
                Err(error) => self.tab.autosave_status = format!("Recovery save failed: {error}"),
            }
        }
        Task::none()
    }

    pub(super) fn restore_recovered(&mut self) {
        // Oldest first, so the latest draft ends up in front.
        let candidates = std::mem::take(&mut self.recovered);
        let count = candidates.len();
        for candidate in candidates.into_iter().rev() {
            self.restore(candidate);
        }
        self.status = match count {
            0 => return,
            1 => "Recovered drawing · Save to keep a new copy".into(),
            n => format!("Recovered {n} drawings as tabs · Save them to keep new copies"),
        };
    }

    pub(super) fn dismiss_recovery(&mut self) {
        self.recovered.clear();
        if self.status.is_empty() {
            self.status = super::READY.into();
        }
    }

    /// Opens a recovery draft in a new tab, or in the tab in front if it is an
    /// unchanged empty Untitled drawing.
    fn restore(&mut self, candidate: Candidate) {
        self.target_tab();
        let before = self.tab.doc.clone();
        self.tab.doc = candidate.snapshot.document;
        self.tab.doc.version = self.tab.doc.version.max(15);
        self.sync_drawing_defaults();
        self.tab.styles.editor = None;
        self.theme_library.editor = None;
        self.tab.path = None;
        self.tab.untitled_name = None;
        self.tab.saved = Document::default();
        self.tab.file_epoch = self.next_epoch();
        self.changed(before);
        self.tab.revision = self.tab.revision.wrapping_add(1);
        self.fit();
        self.tab.selected.clear();
        self.recover_candidate(candidate.path);
    }
}

fn remove_draft(id: TabId, path: PathBuf) -> Task<Message> {
    let key = Key {
        serial: 0,
        generation: 0,
        epoch: 0,
        revision: 0,
        clear: true,
    };
    Task::perform(execute(path, Work::Clear), move |result| {
        Message::Tab(id, Box::new(Message::Autosaved(key, result)))
    })
}

#[cfg(test)]
pub(super) mod tests;
