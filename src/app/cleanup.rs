use super::{App, Job, Message, Request};
use iced::Task;
use reshiki::cleanup::{Options, Scope};

#[derive(Debug, Clone)]
pub struct CleanupJob {
    pub options: Options,
    pub selection: Vec<u64>,
    pub serial: u64,
    pub epoch: u64,
}
impl App {
    pub(super) fn begin_cleanup(
        &mut self,
        scope: Option<Scope>,
        orientation: Option<bool>,
    ) -> Task<Message> {
        if self.tab.busy {
            return Task::none();
        }
        let mut job = if let Some(preview) = &self.tab.cleanup {
            preview.job.clone()
        } else {
            let selection: Vec<_> = self
                .tab
                .doc
                .expand_abbreviation_selection(&self.tab.selected)
                .into_iter()
                .filter(|id| self.tab.doc.atom(*id).is_some())
                .collect();
            if selection.is_empty() {
                self.status = "Select a molecule or atoms to clean up".into();
                self.error = true;
                return Task::none();
            }
            CleanupJob {
                options: Options {
                    scope: Scope::SelectedAtoms,
                    ..Options::default()
                },
                selection,
                serial: self.tab.cleanup_serial,
                epoch: self.tab.file_epoch,
            }
        };
        if let Some(scope) = scope {
            job.options.scope = scope;
        }
        if let Some(keep_orientation) = orientation {
            job.options.keep_orientation = keep_orientation;
        }
        if job.options.scope != Scope::Drawing && job.selection.is_empty() {
            self.status = "Select atoms before using selected cleanup".into();
            return Task::none();
        }
        self.tab.cleanup_serial = self.tab.cleanup_serial.wrapping_add(1);
        job.serial = self.tab.cleanup_serial;
        let mut request = Request::molecule("clean", self.tab.doc.clone());
        request.selected_ids = Some(job.selection.clone());
        request.cleanup = Some(job.options);
        self.run(request, Job::Clean(job))
    }
    pub(super) fn set_cleanup_original(&mut self, original: bool) {
        if let Some(preview) = &mut self.tab.cleanup {
            preview.original = original;
        }
    }
    pub(super) fn cancel_cleanup(&mut self) {
        self.tab.cleanup_serial = self.tab.cleanup_serial.wrapping_add(1);
        self.tab.cleanup = None;
        self.status = "Cleanup cancelled · Drawing unchanged".into();
        self.error = false;
    }
    pub(super) fn apply_cleanup(&mut self) {
        if self.tab.busy {
            return;
        }
        if let Some(preview) = self.tab.cleanup.take() {
            if preview.revision != self.tab.revision || preview.epoch != self.tab.file_epoch {
                self.status = "Drawing changed · Run cleanup again".into();
                return;
            }
            let before = self.tab.doc.clone();
            self.tab.doc = preview.document;
            self.changed(before);
            if !self.error {
                self.tab.analysis = preview.analysis;
                self.status = "Cleanup applied · Undo restores the original layout".into();
            }
        }
    }
}
