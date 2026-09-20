use super::{App, Job, Message, Request};
use iced::Task;
use moruno::cleanup::{Options, Scope};

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
        if self.busy {
            return Task::none();
        }
        let mut job = if let Some(preview) = &self.cleanup {
            preview.job.clone()
        } else {
            let selection: Vec<_> = self
                .doc
                .expand_abbreviation_selection(&self.selected)
                .into_iter()
                .filter(|id| self.doc.atom(*id).is_some())
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
                serial: self.cleanup_serial,
                epoch: self.file_epoch,
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
        self.cleanup_serial = self.cleanup_serial.wrapping_add(1);
        job.serial = self.cleanup_serial;
        let mut request = Request::molecule("clean", self.doc.clone());
        request.selected_ids = Some(job.selection.clone());
        request.cleanup = Some(job.options);
        self.run(request, Job::Clean(job))
    }
}
