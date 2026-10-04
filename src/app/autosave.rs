//! Ordered recovery operations. Only one worker may touch a tab's draft file.
use super::{App, Message, document_tab::DocumentTab, document_tab::TabId};
use iced::Task;
use reshiki::{document::Document, recovery::Recovery};
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
        self.each_tab(Self::clear_recovery);
        self.exit.closing = Some(id);
        Task::batch([self.retry_retired(), self.start_autosave()])
    }

    pub(super) fn restart_after_recovery(&mut self) -> Task<Message> {
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
        self.tabs.deferred_results.clear();
    }

    /// Removes a closed tab's draft; a write still running for it finishes
    /// first, and its result, which finds no tab, starts the removal.
    pub(super) fn retire(&mut self, tab: DocumentTab) -> Task<Message> {
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
pub(super) mod tests {
    use super::*;
    use reshiki::document::Point;

    fn fixture() -> (App, tempfile::TempDir) {
        let (mut app, _) = App::new();
        let directory = tempfile::tempdir().unwrap();
        app.tab.recovery = Some(Recovery::in_directory(directory.path()).unwrap());
        app.tab.doc.add_atom("O", Point::default());
        app.tab.revision = 1;
        (app, directory)
    }

    #[test]
    fn failed_retirement_cancels_exit_and_is_retried_on_the_next_close() {
        let (mut app, directory) = fixture();
        let id = app.tab.id;
        let path = app.tab.recovery.as_ref().unwrap().session.clone();
        let _ = app.close_active_tab();
        assert!(app.tabs.retiring[0].pending);
        let key = Key {
            serial: 0,
            generation: 0,
            epoch: 0,
            revision: 0,
            clear: true,
        };
        app.tab.recovery = Some(Recovery::in_directory(directory.path()).unwrap());
        let _ = app.close_after_recovery(iced::window::Id::unique());
        assert!(app.exit.closing());
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::Autosaved(key, Err("Permission denied".into()))),
        ));
        assert!(!app.exit.closing());
        assert!(app.error && app.status.contains("Permission denied"));
        assert_eq!(app.tabs.retiring.len(), 1);
        assert_eq!(app.tabs.retiring[0].path, path);
        assert!(!app.tabs.retiring[0].pending);
        assert!(!app.drafts_settled());
        let _ = app.start_autosave();
        assert!(!app.tabs.retiring[0].pending, "No automatic retry loop");
        let _ = app.close_after_recovery(iced::window::Id::unique());
        assert!(app.tabs.retiring[0].pending);
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::Autosaved(key, Ok(None))),
        ));
        assert!(app.tabs.retiring.is_empty());
    }

    #[test]
    fn aborted_close_delivers_held_jobs_and_leaves_both_front_and_background_tabs_usable() {
        for background in [false, true] {
            let (mut app, directory) = fixture();
            app.tab.doc = Document::default();
            app.tab.saved = app.tab.doc.clone();
            app.tab.busy = true;
            let (id, revision) = (app.tab.id, app.tab.revision);
            if background {
                app.tab.recovery = None;
                app.add_tab();
                app.tab.recovery = Some(Recovery::in_directory(directory.path()).unwrap());
            }
            let front = app.tab.id;
            let _ = app.update(Message::Close(iced::window::Id::unique()));
            let key = app.tab.autosave.pending.unwrap();
            assert!(app.exit.closing());
            let mut document = Document::default();
            document.add_atom("O", Point::default());
            let _ = app.update(Message::Tab(
                id,
                Box::new(Message::EngineDone {
                    revision,
                    kind: super::super::Job::Insert,
                    result: Box::new(Ok(reshiki::engine::Response {
                        document: Some(document),
                        analysis: None,
                        output: None,
                        engine_version: "test".into(),
                        warnings: vec![],
                    })),
                }),
            ));
            assert!(app.strip().all(|tab| tab.doc.all_ids().is_empty()));
            assert_eq!(app.tabs.deferred_results.len(), 1);
            let _ = app.update(Message::Autosaved(key, Err("Permission denied".into())));
            assert!(!app.exit.frozen());
            assert!(app.tabs.deferred_results.is_empty());
            assert_eq!(app.tab.id, front);
            app.in_tab(id, |app| {
                assert!(!app.tab.busy);
                assert_eq!(app.tab.doc.atoms[0].element, "O");
                assert!(app.tab.history.can_undo());
                assert!(
                    app.update(Message::Analyze).units() > 0,
                    "Another job can start"
                );
            })
            .unwrap();
            if background {
                assert!(app.tab.doc.all_ids().is_empty() && !app.tab.busy);
            }
        }
    }

    #[test]
    fn successful_close_discards_held_results_and_stays_frozen_until_exit() {
        let (mut app, _) = fixture();
        app.tab.doc = Document::default();
        app.tab.saved = app.tab.doc.clone();
        let id = app.tab.id;
        let _ = app.update(Message::Close(iced::window::Id::unique()));
        let key = app.tab.autosave.pending.unwrap();
        let message = || Message::Tab(id, Box::new(Message::Pasted(Some("O".into()))));
        let _ = app.update(message());
        assert_eq!(app.tabs.deferred_results.len(), 1);
        let _ = app.update(Message::Autosaved(key, Ok(None)));
        assert!(app.exit.committed() && app.exit.frozen());
        assert!(app.tabs.deferred_results.is_empty());
        let _ = app.update(message());
        assert!(app.tabs.deferred_results.is_empty());
        assert!(!app.tab.busy && app.tab.doc.all_ids().is_empty());
    }

    fn run(work: (Key, PathBuf, Work)) -> (Key, Result<Option<PathBuf>, String>) {
        let (key, path, work) = work;
        (
            key,
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(execute(path, work)),
        )
    }

    pub(in crate::app) fn finish_pending(app: &mut App) {
        let key = app.tab.autosave.pending.unwrap();
        let work = if key.clear {
            Work::Clear
        } else {
            Work::Save {
                document: Box::new(app.recovery_document()),
                source: app.tab.path.clone(),
                retire_candidate: None,
            }
        };
        let path = app.tab.recovery.as_ref().unwrap().session.clone();
        let (key, result) = run((key, path, work));
        assert!(result.is_ok(), "{result:?}");
        let _ = app.update(Message::Autosaved(key, result));
    }

    /// Runs the draft work a tab started and delivers its tagged result.
    fn finish_tab(app: &mut App, id: TabId) {
        let work = app
            .in_tab(id, |app| {
                let key = app.tab.autosave.pending.unwrap();
                let work = if key.clear {
                    Work::Clear
                } else {
                    Work::Save {
                        document: Box::new(app.recovery_document()),
                        source: app.tab.path.clone(),
                        retire_candidate: app.tab.autosave.retire_candidate.clone(),
                    }
                };
                (
                    key,
                    app.tab.recovery.as_ref().unwrap().session.clone(),
                    work,
                )
            })
            .unwrap();
        let (key, result) = run(work);
        assert!(result.is_ok(), "{result:?}");
        let _ = app.update(Message::Tab(id, Box::new(Message::Autosaved(key, result))));
    }

    fn draft(app: &App, id: TabId) -> PathBuf {
        let tab = app.strip().find(|tab| tab.id == id).unwrap();
        tab.recovery.as_ref().unwrap().session.clone()
    }

    #[test]
    fn every_tab_keeps_its_own_draft_and_closing_removes_them() {
        let (mut app, directory) = fixture();
        app.tabs.recovery_root = Some(directory.path().into());
        let first = app.tab.id;
        let _ = app.update(Message::New);
        let second = app.tab.id;
        let before = app.tab.doc.clone();
        app.tab.doc.add_atom("N", Point::default());
        app.changed(before);
        let _ = app.update(Message::New);
        let closed = app.tab.id;
        let before = app.tab.doc.clone();
        app.tab.doc.add_atom("S", Point::default());
        app.changed(before);
        assert_ne!(draft(&app, first), draft(&app, second));
        let _ = app.update(Message::Tick);
        for id in [first, second, closed] {
            finish_tab(&mut app, id);
        }
        for (id, element) in [(first, "O"), (second, "N")] {
            let saved: reshiki::recovery::Snapshot =
                serde_json::from_slice(&std::fs::read(draft(&app, id)).unwrap()).unwrap();
            assert_eq!(saved.document.atoms[0].element, element);
        }
        // Discarding a tab removes its draft.
        let closed_draft = draft(&app, closed);
        let _ = app.update(Message::Tabs(super::super::tabs::Action::Close(None)));
        let _ = app.update(Message::Discard);
        assert!(!app.tabs.retiring.is_empty());
        let key = Key {
            serial: 0,
            generation: 0,
            epoch: 0,
            revision: 0,
            clear: true,
        };
        let result = run((key, closed_draft.clone(), Work::Clear)).1;
        let _ = app.update(Message::Tab(
            closed,
            Box::new(Message::Autosaved(key, result)),
        ));
        assert!(!closed_draft.exists() && app.tabs.retiring.is_empty());
        // The window closes once both remaining drafts are removed.
        let window = iced::window::Id::unique();
        let _ = app.update(Message::Close(window));
        for _ in 0..2 {
            let _ = app.update(Message::Discard);
        }
        assert!(app.exit.closing());
        finish_tab(&mut app, first);
        assert!(app.exit.closing(), "Waits for the other tab's draft");
        let path = draft(&app, second);
        let key = app
            .in_tab(second, |app| app.tab.autosave.pending.unwrap())
            .unwrap();
        let (key, result) = run((key, path.clone(), Work::Clear));
        let close = app.update(Message::Tab(
            second,
            Box::new(Message::Autosaved(key, result)),
        ));
        assert!(close.units() > 0 && !app.exit.closing());
        assert!(!path.exists() && !draft(&app, first).exists());
    }

    #[test]
    fn a_tab_closed_during_its_draft_write_removes_the_draft_afterwards() {
        let (mut app, directory) = fixture();
        app.tabs.recovery_root = Some(directory.path().into());
        let closed = app.tab.id;
        app.request_autosave();
        let write = app.prepare_autosave().unwrap();
        let _ = app.update(Message::Tabs(super::super::tabs::Action::Close(None)));
        assert!(
            app.update(Message::Discard).units() == 0,
            "The removal waits for the write"
        );
        assert_ne!(app.tab.id, closed);
        let path = write.1.clone();
        let (key, result) = run(write);
        assert!(path.exists());
        let removal = app.update(Message::Tab(
            closed,
            Box::new(Message::Autosaved(key, result)),
        ));
        assert!(removal.units() > 0 && !app.tabs.retiring.is_empty());
        let key = Key { clear: true, ..key };
        let result = run((key, path.clone(), Work::Clear)).1;
        let _ = app.update(Message::Tab(
            closed,
            Box::new(Message::Autosaved(key, result)),
        ));
        assert!(!path.exists() && app.tabs.retiring.is_empty());
    }

    #[test]
    fn restore_opens_every_draft_as_a_tab_with_the_latest_in_front() {
        use reshiki::recovery::{Candidate, Snapshot};
        let (mut app, directory) = fixture();
        app.tab.doc = Default::default();
        app.tab.revision = 0;
        app.tabs.recovery_root = Some(directory.path().into());
        let candidate = |element: &str, saved_at| {
            let mut document = reshiki::document::Document::default();
            document.add_atom(element, Point::default());
            let path = directory.path().join(format!("4294967294-{element}.json"));
            std::fs::write(&path, "draft").unwrap();
            Candidate {
                path,
                snapshot: Snapshot {
                    document,
                    source: None,
                    saved_at,
                },
            }
        };
        // Candidates arrive newest first.
        app.recovered = vec![candidate("N", 2), candidate("O", 1)];
        let _ = app.update(Message::Restore);
        assert!(app.recovered.is_empty());
        let elements: Vec<_> = app
            .strip()
            .map(|tab| tab.doc.atoms[0].element.clone())
            .collect();
        assert_eq!(elements, ["O", "N"], "The empty tab took the first draft");
        assert_eq!(app.tabs.active, 1);
        assert!(app.status.starts_with("Recovered 2 drawings"));
        let ids: Vec<_> = app.strip().map(|tab| tab.id).collect();
        for id in ids {
            assert!(app.edited(app.strip().find(|tab| tab.id == id).unwrap()));
            finish_tab(&mut app, id);
        }
        let old: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("4294967294")
            })
            .collect();
        assert!(old.is_empty(), "Each restored draft replaced its original");
    }

    #[test]
    fn discard_waits_for_old_write_then_clears_before_new_save() {
        let (mut app, _directory) = fixture();
        app.request_autosave();
        let old = app.prepare_autosave().unwrap();
        let path = old.1.clone();
        app.clear_recovery();
        app.tab.file_epoch += 1;
        app.tab.doc.atoms[0].element = "N".into();
        app.request_autosave();
        assert!(
            app.prepare_autosave().is_none(),
            "One writer, even after a discard"
        );
        let (key, result) = run(old);
        let _ = app.autosaved(key, result);
        assert_eq!(app.tab.autosaved_revision, None);
        assert!(app.tab.autosave_status.is_empty());
        let clear = app.prepare_autosave().unwrap();
        assert!(clear.0.clear);
        let (key, result) = run(clear);
        let _ = app.autosaved(key, result);
        assert!(!path.exists(), "The old write cannot run after its clear");
        let latest = app.prepare_autosave().unwrap();
        let (key, result) = run(latest);
        let _ = app.autosaved(key, result);
        let saved: reshiki::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved.document.atoms[0].element, "N");
        assert_eq!(app.tab.autosaved_revision, Some(app.tab.revision));
    }

    #[test]
    fn edits_coalesce_and_stale_failures_do_not_replace_status() {
        let (mut app, _directory) = fixture();
        app.request_autosave();
        let first = app.prepare_autosave().unwrap();
        for element in ["N", "S", "C"] {
            app.tab.doc.atoms[0].element = element.into();
            app.tab.revision += 1;
            app.request_autosave();
            assert!(app.prepare_autosave().is_none());
        }
        let _ = app.autosaved(first.0, Err("obsolete error".into()));
        assert!(app.tab.autosave_status.is_empty());
        let latest = app.prepare_autosave().unwrap();
        let (key, result) = run(latest);
        let _ = app.autosaved(key, result);
        assert_eq!(app.tab.autosaved_revision, Some(4));
        assert!(app.prepare_autosave().is_none());
        let path = app.tab.recovery.as_ref().unwrap().session.clone();
        let saved: reshiki::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved.document.atoms[0].element, "C");
    }

    #[test]
    fn undo_to_saved_state_clears_an_in_flight_first_save() {
        let (mut app, directory) = fixture();
        app.tab.history.commit(app.tab.saved.clone(), &app.tab.doc);
        let other = Recovery::in_directory(directory.path()).unwrap();
        other.save(&app.tab.doc, None).unwrap();
        let other_contents = std::fs::read(&other.session).unwrap();
        app.request_autosave();
        let old = app.prepare_autosave().unwrap();
        let path = old.1.clone();
        let _ = app.update(Message::Undo);
        assert!(!app.dirty());
        assert_ne!(app.tab.revision, old.0.revision);
        assert_eq!(app.tab.autosaved_revision, None);
        assert!(app.prepare_autosave().is_none(), "Wait for the old writer");

        let (key, result) = run(old);
        assert!(result.is_ok());
        assert!(path.exists(), "The old worker persisted the undone drawing");
        let _ = app.update(Message::Autosaved(key, result));
        assert!(
            app.tab.autosave.pending.unwrap().clear,
            "No further Tick is needed"
        );
        finish_pending(&mut app);
        assert!(!path.exists());
        assert_eq!(std::fs::read(other.session).unwrap(), other_contents);
        assert_eq!(app.tab.autosaved_revision, None);
        assert!(!app.tab.autosave.pending());
    }

    #[test]
    fn cancelled_or_undone_inline_drafts_clear_stale_first_save() {
        use iced::widget::text_editor::{Action, Edit};
        for cancel in [false, true] {
            let (mut app, _directory) = fixture();
            app.tab.saved = app.tab.doc.clone();
            let _ = app.update(Message::InlineText(
                super::super::inline_text::Action::Begin(None, Point::default()),
            ));
            let _ = app.update(Message::CaptionAction(Action::Edit(Edit::Paste(
                "Unsaved caption".to_owned().into(),
            ))));
            app.request_autosave();
            let old = app.prepare_autosave().unwrap();
            let path = old.1.clone();
            let _ = app.update(if cancel {
                Message::Escape
            } else {
                Message::Undo
            });
            assert!(!app.dirty());
            assert_eq!(app.tab.revision, old.0.revision);
            assert_ne!(app.tab.autosave.generation, old.0.generation);
            let (key, result) = run(old);
            assert!(result.is_ok());
            let snapshot: reshiki::recovery::Snapshot =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(snapshot.document.annotations[0].text, "Unsaved caption");
            let _ = app.update(Message::Autosaved(key, result));
            assert!(app.tab.autosave.pending.unwrap().clear);
            finish_pending(&mut app);
            assert!(!path.exists());
            assert_eq!(app.tab.doc, app.tab.saved);
            assert_eq!(app.tab.inline_text.is_some(), !cancel);
        }
    }

    #[test]
    fn stale_failed_save_after_undo_still_queues_recovery_cleanup() {
        let (mut app, _directory) = fixture();
        app.tab.history.commit(app.tab.saved.clone(), &app.tab.doc);
        app.tab
            .recovery
            .as_ref()
            .unwrap()
            .save(&app.tab.doc, None)
            .unwrap();
        app.request_autosave();
        let old = app.prepare_autosave().unwrap();
        let _ = app.update(Message::Undo);
        let status = app.status.clone();
        let _ = app.update(Message::Autosaved(old.0, Err("obsolete error".into())));
        assert!(app.tab.autosave.pending.unwrap().clear);
        assert_eq!(app.status, status);
        assert!(app.tab.autosave_status.is_empty());
        finish_pending(&mut app);
        assert!(!old.1.exists());
    }

    #[test]
    fn redo_during_stale_save_cleanup_preserves_latest_recovery() {
        let (mut app, _directory) = fixture();
        app.tab.history.commit(app.tab.saved.clone(), &app.tab.doc);
        app.request_autosave();
        let old = app.prepare_autosave().unwrap();
        let path = old.1.clone();
        let _ = app.update(Message::Undo);
        let (key, result) = run(old);
        assert!(result.is_ok());
        let _ = app.autosaved(key, result);
        let clear = app.prepare_autosave().unwrap();
        assert!(clear.0.clear);
        let _ = app.update(Message::Redo);
        assert!(app.dirty());
        app.request_autosave();
        assert!(
            app.prepare_autosave().is_none(),
            "Clear still owns the file"
        );
        let (key, result) = run(clear);
        assert!(result.is_ok());
        let _ = app.autosaved(key, result);
        assert!(!path.exists());
        let (key, result) = run(app.prepare_autosave().unwrap());
        assert!(!key.clear);
        assert!(result.is_ok());
        let _ = app.autosaved(key, result);
        let snapshot: reshiki::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(snapshot.document.atoms[0].element, "O");
        assert_eq!(app.tab.autosaved_revision, Some(app.tab.revision));
    }

    #[test]
    fn inline_draft_changes_invalidate_same_revision_completion() {
        let (mut app, _directory) = fixture();
        app.request_autosave();
        let first = app.prepare_autosave().unwrap();
        app.tab.autosave.edited_draft();
        let (key, result) = run(first);
        let _ = app.autosaved(key, result);
        assert_eq!(app.tab.autosaved_revision, None);
    }

    #[test]
    fn close_waits_for_clear_and_failure_keeps_window_open() {
        let (mut app, _directory) = fixture();
        app.request_autosave();
        let old = app.prepare_autosave().unwrap();
        let id = iced::window::Id::unique();
        assert_eq!(app.close_after_recovery(id).units(), 0);
        let (key, result) = run(old);
        assert_eq!(app.autosaved(key, result).units(), 0);
        let clear = app.prepare_autosave().unwrap();
        let (key, result) = run(clear);
        assert!(app.autosaved(key, result).units() > 0);
        assert!(!app.exit.closing());

        let _ = app.close_after_recovery(id);
        let key = app.tab.autosave.pending.unwrap();
        assert_eq!(
            app.autosaved(key, Err("permission denied".into())).units(),
            0
        );
        assert!(!app.exit.closing());
        assert!(app.tab.autosave_status.contains("permission denied"));
    }

    #[test]
    fn restoring_keeps_original_until_a_durable_copy_succeeds() {
        let (mut app, directory) = fixture();
        let original = directory.path().join("original.json");
        std::fs::write(&original, "original recovery").unwrap();
        let missing = directory.path().join("missing/draft.json");
        let work = Work::Save {
            document: Box::new(app.tab.doc.clone()),
            source: None,
            retire_candidate: Some(original.clone()),
        };
        assert!(
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(execute(missing, work))
                .is_err()
        );
        assert!(original.exists());
        app.recover_candidate(original.clone());
        let work = app.prepare_autosave().unwrap();
        let (key, result) = run(work);
        assert_eq!(result.as_ref().unwrap(), &Some(original.clone()));
        let _ = app.autosaved(key, result);
        assert!(!original.exists());
        assert!(app.tab.recovery.as_ref().unwrap().session.exists());
    }

    #[test]
    fn failed_restore_write_retries_without_losing_original_identity() {
        let (mut app, directory) = fixture();
        let original = directory.path().join("original.json");
        std::fs::write(&original, "original").unwrap();
        app.recover_candidate(original.clone());
        let (key, path, work) = app.prepare_autosave().unwrap();
        let failure = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(execute(directory.path().join("missing/draft.json"), work));
        assert!(failure.is_err());
        let _ = app.autosaved(key, failure);
        assert!(original.exists());
        app.request_autosave();
        let (key, result) = run(app.prepare_autosave().unwrap());
        assert!(result.is_ok());
        let _ = app.autosaved(key, result);
        assert!(!original.exists());
        assert!(path.exists());
        assert!(app.tab.autosave.retire_candidate.is_none());
    }

    #[test]
    fn updater_handoff_waits_for_recovery_clear() {
        let (mut app, _directory) = fixture();
        app.request_autosave();
        let old = app.prepare_autosave().unwrap();
        assert_eq!(app.restart_after_recovery().units(), 0);
        assert!(app.exit.closing());
        let (key, result) = run(old);
        assert_eq!(app.autosaved(key, result).units(), 0);
        let (key, result) = run(app.prepare_autosave().unwrap());
        assert!(app.autosaved(key, result).units() > 0);
    }

    #[test]
    fn failed_update_handoff_replays_clipboard_results_on_either_side_of_draft_cleanup() {
        for background in [false, true] {
            for before_cleanup in [false, true] {
                let (mut app, directory) = fixture();
                app.tab.busy = false;
                app.tab.clipboard_busy = true;
                let (id, epoch, revision) = (app.tab.id, app.tab.file_epoch, app.tab.revision);
                if background {
                    app.tab.recovery = None;
                    app.add_tab();
                    app.tab.recovery = Some(Recovery::in_directory(directory.path()).unwrap());
                }
                app.updates.restarting = true;
                let _ = app.restart_after_recovery();
                let key = app.tab.autosave.pending.unwrap();
                let completion = || {
                    Message::Tab(
                        id,
                        Box::new(Message::ClipboardRead {
                            epoch,
                            revision,
                            result: Box::new(Err("Clipboard unavailable".into())),
                        }),
                    )
                };
                if before_cleanup {
                    let _ = app.update(completion());
                }
                let _ = app.update(Message::Autosaved(key, Ok(None)));
                assert!(app.exit.frozen() && !app.exit.committed());
                if !before_cleanup {
                    let _ = app.update(completion());
                }
                assert_eq!(app.tabs.deferred_results.len(), 1);
                let _ = app.update(Message::Updates(super::super::updates::Action::Restarted(
                    Err("Installer failed".into()),
                )));
                assert!(!app.exit.frozen() && !app.updates.restarting);
                assert!(app.tabs.deferred_results.is_empty());
                assert!(app.updates.open, "The failed handoff remains visible");
                app.in_tab(id, |app| {
                    assert!(!app.tab.clipboard_busy);
                    assert!(app.status.contains("Clipboard unavailable"));
                    // Replay finishes while the failure dialog owns input;
                    // new clipboard commands resume after dismissing it.
                    let _ = app.update(Message::Escape);
                    assert!(!app.updates.open);
                    assert!(
                        app.update(Message::Paste).units() > 0,
                        "Paste is usable after dismissing a failed handoff"
                    );
                    if reshiki::clipboard::available() {
                        assert!(app.tab.clipboard_busy, "The new paste actually starts");
                    }
                })
                .unwrap();
            }
        }
    }

    #[test]
    fn close_waits_for_explicit_file_save_and_blocks_new_edits() {
        let (mut app, _directory) = fixture();
        app.file_io.saving = true;
        let id = iced::window::Id::unique();
        assert_eq!(app.close_after_recovery(id).units(), 0);
        assert!(!app.tab.autosave.pending());
        let before = app.tab.doc.clone();
        let _ = app.update(Message::ContextKey("n".into()));
        assert_eq!(app.tab.doc, before);
        app.file_io.saving = false;
        assert!(app.start_autosave().units() > 0);
        assert!(app.tab.autosave.pending.unwrap().clear);
    }

    #[test]
    fn failed_file_save_cancels_close_and_keeps_a_recovery_draft() {
        let (mut app, _directory) = fixture();
        app.file_io.saving = true;
        let _ = app.close_after_recovery(iced::window::Id::unique());
        let _ = app.update(Message::Saved(
            app.tab.file_epoch,
            Box::new(app.tab.doc.clone()),
            Err("disk full".into()),
        ));
        assert!(!app.exit.closing());
        assert!(app.error);
        assert_eq!(app.status, "disk full");
        assert!(
            !app.tab.autosave.pending.unwrap().clear,
            "Closing failure must save, not discard recovery"
        );
        finish_pending(&mut app);
        assert!(app.tab.recovery.as_ref().unwrap().session.exists());
    }

    #[test]
    fn failed_recovery_clear_does_not_start_update_handoff() {
        let (mut app, _directory) = fixture();
        app.updates.restarting = true;
        let _ = app.restart_after_recovery();
        let key = app.tab.autosave.pending.unwrap();
        assert_eq!(app.autosaved(key, Err("access denied".into())).units(), 0);
        assert!(!app.updates.restarting);
        assert!(!app.exit.closing());
        assert!(app.tab.autosave_status.contains("access denied"));
    }

    #[test]
    #[ignore = "release-mode recovery benchmark"]
    fn recovery_workloads() {
        use std::{hint::black_box, time::Instant};
        fn measure(name: &str, mut work: impl FnMut()) {
            for _ in 0..3 {
                work();
            }
            let mut samples = Vec::new();
            for _ in 0..30 {
                let start = Instant::now();
                work();
                samples.push(start.elapsed().as_secs_f64() * 1000.);
            }
            samples.sort_by(f64::total_cmp);
            println!("{name},{:.4},{:.4}", samples[15], samples[28]);
        }
        println!("workload,median_ms,p95_ms");
        let gallery: Document =
            serde_json::from_str(include_str!("../../assets/examples/shortcut-examples.rsk"))
                .unwrap();
        for copies in [1, 4] {
            let (mut app, _directory) = fixture();
            app.tab.doc = Document::default();
            for copy in 0..copies {
                reshiki::editing::append(
                    &mut app.tab.doc,
                    &gallery,
                    Point::new(copy as f32 * 1800., 0.),
                );
            }
            measure(&format!("gallery_{copies}x_recovery_synchronous"), || {
                app.tab
                    .recovery
                    .as_ref()
                    .unwrap()
                    .save(&app.recovery_document(), app.tab.path.clone())
                    .unwrap();
            });
            measure(&format!("gallery_{copies}x_recovery_dispatch"), || {
                app.tab.autosave.pending = None;
                app.tab.autosaved_revision = None;
                let _ = black_box(app.update(Message::Tick));
            });
            let runtime = tokio::runtime::Runtime::new().unwrap();
            measure(&format!("gallery_{copies}x_recovery_roundtrip"), || {
                app.tab.autosave.pending = None;
                app.tab.autosaved_revision = None;
                app.request_autosave();
                let (key, path, work) = app.prepare_autosave().unwrap();
                let result = runtime.block_on(execute(path, work));
                let _ = black_box(app.autosaved(key, result));
            });
        }
    }
}
