//! Chemistry engine jobs: launching requests and applying their results.
use super::{App, CleanupPreview, Job, Message, cleanup::CleanupJob, export_file, input_request};
use crate::canvas::Tool;
use iced::{Subscription, Task};
use reshiki::{
    document::{Document, Point},
    editing,
    engine::{ChemistryEngine, Request, Response},
};

const INSERT_STRUCTURE_FAILED: &str = "Could not insert structure";
const PROGRESS_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

/// A receipt belongs to one request, drawing and tab, even after cancellation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    tab: super::document_tab::TabId,
    epoch: u64,
    revision: u64,
    serial: u64,
}

/// Dropping the task handle aborts its future, including an active native helper.
/// A blocking calculation already running may finish, but cannot publish a result.
pub(super) struct Pending {
    key: Key,
    kind: Job,
    started: iced::time::Instant,
    _handle: iced::task::Handle,
}

impl App {
    pub(super) fn run(&mut self, request: Request, kind: Job) -> Task<Message> {
        if self.tab.busy {
            return Task::none();
        }
        let engine = self.engine.clone();
        let aromatic_selection = matches!(kind, Job::AromaticDisplay);
        self.start_engine_job(kind, async move {
            if aromatic_selection {
                super::shortcuts::aromatic_selection(engine, request).await
            } else {
                engine.execute(request).await
            }
        })
    }

    fn start_engine_job(
        &mut self,
        kind: Job,
        future: impl Future<Output = Result<Response, String>> + Send + 'static,
    ) -> Task<Message> {
        if self.tab.busy {
            return Task::none();
        }
        self.tab.busy = true;
        self.error = false;
        self.status = "Working…".into();
        self.tab.engine_serial = self.tab.engine_serial.wrapping_add(1);
        let key = Key {
            tab: self.tab.id,
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            serial: self.tab.engine_serial,
        };
        let (task, handle) = Task::perform(future, move |result| Message::EngineFinished {
            key,
            result: Box::new(result),
        })
        .abortable();
        self.tab.engine_job = Some(Pending {
            key,
            kind,
            started: iced::time::Instant::now(),
            _handle: handle.abort_on_drop(),
        });
        task
    }

    pub(super) fn engine_subscription(&self) -> Subscription<Message> {
        if self.strip().any(|tab| tab.engine_job.is_some()) {
            iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }

    pub(super) fn engine_progress(&self) -> Option<(Key, String)> {
        let job = self.tab.engine_job.as_ref()?;
        let elapsed = job.started.elapsed();
        (elapsed >= PROGRESS_DELAY).then(|| (job.key, format!("Working · {} s", elapsed.as_secs())))
    }

    /// An activation queued on a previous job must never cancel a newer one.
    pub(super) fn cancel_engine_request(&mut self, key: Key) {
        if key.tab == self.tab.id
            && key.epoch == self.tab.file_epoch
            && self
                .tab
                .engine_job
                .as_ref()
                .is_some_and(|job| job.key == key)
        {
            self.cancel_engine_job();
        }
    }

    /// Cancel affects only transient work. Drawing, history, selection and camera stay put.
    pub(super) fn cancel_engine_job(&mut self) {
        let Some(job) = self.tab.engine_job.take() else {
            return;
        };
        self.tab.busy = false;
        #[cfg(target_arch = "wasm32")]
        if matches!(job.kind, Job::ImportFile(_)) {
            self.tab.file_handle = None;
        }
        #[cfg(any(windows, target_os = "linux"))]
        if matches!(job.kind, Job::ImportFile(_)) {
            self.desktop_import_finished();
        }
        drop(job);
        self.status = "Operation cancelled".into();
        self.error = false;
    }

    pub(super) fn engine_finished(
        &mut self,
        key: Key,
        result: Result<Response, String>,
    ) -> Task<Message> {
        if key.tab != self.tab.id || key.epoch != self.tab.file_epoch {
            return Task::none();
        }
        let Some(job) = self.tab.engine_job.take_if(|job| job.key == key) else {
            return Task::none();
        };
        self.engine_done(key.revision, job.kind, result)
    }
    pub(super) fn request_aromatic_display(&mut self) -> Task<Message> {
        if self.tab.selected.is_empty() {
            self.status = "Select an aromatic ring first".into();
            return Task::none();
        }
        let mut request = Request::molecule("aromatic", self.tab.doc.clone());
        request.selected_ids = Some(self.tab.selected.clone());
        self.run(request, Job::AromaticDisplay)
    }
    pub(super) fn insert_input(&mut self) -> Task<Message> {
        if self.imports.kind == super::import::InputKind::Name {
            return self.naming_action(super::naming::Action::InsertName);
        }
        self.run(input_request(&self.imports.input.text()), Job::Insert)
    }
    pub(super) fn import_input(&mut self) -> Task<Message> {
        self.run(input_request(&self.imports.input.text()), Job::Import)
    }
    pub(super) fn insert_example(&mut self, smiles: &'static str) -> Task<Message> {
        self.imports.kind = super::import::InputKind::Structure;
        self.imports.set_text(smiles);
        self.run(Request::import_smiles(smiles), Job::Insert)
    }
    pub(super) fn analyze_drawing(&mut self) -> Task<Message> {
        self.run(
            Request::molecule("analyze", self.tab.doc.clone()),
            Job::Analyze,
        )
    }
    /// A chemistry job's result for its originating drawing.
    pub(super) fn engine_done(
        &mut self,
        revision: u64,
        kind: Job,
        result: Result<Response, String>,
    ) -> Task<Message> {
        self.tab.busy = false;
        #[cfg(any(windows, target_os = "linux"))]
        if matches!(kind, Job::ImportFile(_)) {
            self.desktop_import_finished();
        }
        if matches!(&kind, Job::Clean(job) if job.serial != self.tab.cleanup_serial || job.epoch != self.tab.file_epoch)
        {
            return Task::none();
        }
        let response = match result {
            Err(e) => {
                #[cfg(target_arch = "wasm32")]
                if matches!(kind, Job::ImportFile(_)) {
                    self.tab.file_handle = None;
                }
                self.error = true;
                self.status = e;
                return Task::none();
            }
            Ok(response) => response,
        };
        if let Job::Export(format) = kind {
            return export_file(
                response.output.unwrap_or_default(),
                format,
                self.export_stem(),
                response.warnings,
            );
        }
        if self.tab.revision != revision {
            self.status =
                "Operation finished; newer edits were preserved. Run it again to update.".into();
            return Task::none();
        }
        match kind {
            Job::Clean(job) => self.cleanup_preview_ready(job, revision, response),
            Job::Insert => self.structure_inserted(response),
            Job::AromaticDisplay => self.aromatic_display_ready(response),
            Job::Abbreviate => self.abbreviations_ready(response),
            Job::Analyze => self.analysis_ready(response),
            kind @ (Job::Import | Job::ImportFile(_) | Job::Export(_)) => {
                self.structure_replaced(kind, response)
            }
        }
        Task::none()
    }
    fn cleanup_preview_ready(&mut self, job: CleanupJob, revision: u64, response: Response) {
        if let Some(document) = response.document {
            if let Err(error) = document.validate() {
                self.status = error;
                self.error = true;
            } else {
                self.tab.cleanup = Some(CleanupPreview {
                    job,
                    warnings: response.warnings,
                    document,
                    analysis: response.analysis,
                    revision,
                    epoch: self.tab.file_epoch,
                    original: false,
                });
                self.status =
                    "Cleanup preview · Compare with the original, then Apply or Cancel".into();
                self.error = false;
            }
        }
    }
    /// Place a document beside the drawing, centered on the camera's row.
    /// An empty drawing centers it on the camera instead. Returns the inserted
    /// IDs, or `None` when append inserts nothing, leaving the drawing unchanged
    /// if IDs are exhausted or a document is invalid.
    pub(super) fn insert_beside_drawing(&mut self, document: &Document) -> Option<Vec<u64>> {
        let center = editing::center(document, &document.all_ids());
        let offset = if self.tab.doc.all_ids().is_empty() {
            Point::new(
                self.tab.camera.center.x - center.x,
                self.tab.camera.center.y - center.y,
            )
        } else {
            let (_, existing_max) = self.tab.doc.bounds();
            let (insert_min, _) = document.bounds();
            Point::new(
                existing_max.x + self.tab.doc.drawing_style.bond_length_world - insert_min.x,
                self.tab.camera.center.y - center.y,
            )
        };
        let ids = editing::append(&mut self.tab.doc, document, offset);
        (!ids.is_empty()).then_some(ids)
    }

    fn structure_inserted(&mut self, response: Response) {
        if let Some(document) = response.document {
            let before = self.tab.doc.clone();
            let Some(selected) = self.insert_beside_drawing(&document) else {
                self.tab.doc = before;
                self.error = true;
                self.status = INSERT_STRUCTURE_FAILED.into();
                return;
            };
            self.tab.selected = selected;
            self.changed(before);
            self.fit();
            self.tool = Tool::Select;
            self.status = "Inserted structure · Drag to position · Delete or Undo to remove".into();
            if !response.warnings.is_empty() {
                self.status.push_str(" · ");
                self.status.push_str(&response.warnings.join(" · "));
            }
        }
    }
    fn aromatic_display_ready(&mut self, response: Response) {
        if let Some(document) = response.document {
            let before = self.tab.doc.clone();
            self.tab.doc = document.clone();
            self.changed(before);
            if self.error {
                return;
            }
            // The layout repair below moves what the labels' ink pushes.
            reshiki::atom_labels::copy_computed(&mut self.tab.doc, &document);
            self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
            self.settle_label_layout();
            self.tab.labels_dirty = false;
            self.tab.analysis = response.analysis;
            self.tool = Tool::Select;
            self.status = "Aromatic display changed · Molecular identity retained".into();
        }
    }
    fn abbreviations_ready(&mut self, response: Response) {
        if let Some(document) = response.document {
            let before = self.tab.doc.clone();
            let count = document.abbreviations.len();
            self.tab.doc = document;
            self.tab.selected = self
                .tab
                .doc
                .expand_abbreviation_selection(&self.tab.selected);
            self.changed(before);
            self.tab.analysis = response.analysis;
            self.tool = Tool::Select;
            self.status = if count == 0 {
                "No matching common groups in this selection".into()
            } else {
                format!(
                    "{count} abbreviation{} · Full chemistry retained · Expand to edit internal atoms",
                    if count == 1 { "" } else { "s" }
                )
            };
        }
    }
    fn analysis_ready(&mut self, response: Response) {
        if response.analysis.is_none() {
            // A valid drawing can have no molecular interpretation, notably
            // when a crystal's remaining edges still carry distance provenance.
            // A skipped check must not refresh labels or move the drawing.
            self.tab.analysis = None;
            self.tab.chemistry_notice = None;
            self.status = if response.warnings.is_empty() {
                "Chemical analysis unavailable".into()
            } else {
                response.warnings.join(" · ")
            };
            self.error = false;
            return;
        }
        // Checking is a read-only chemistry operation. Refresh
        // computed H labels without rewriting the user's bond
        // orders/stereo or inserting a step into Undo/Redo.
        if let Some(document) = response.document {
            // The layout repair below moves what the labels' ink pushes.
            reshiki::atom_labels::copy_computed(&mut self.tab.doc, &document);
            self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
            self.settle_label_layout();
        }
        self.tab.analysis = response.analysis;
        self.tab.chemistry_notice = None;
        self.status = if response.warnings.is_empty() {
            "No chemistry errors found".into()
        } else {
            format!("Structure checked · {}", response.warnings.join(" · "))
        };
        self.error = false;
    }
    fn structure_replaced(&mut self, kind: Job, response: Response) {
        if let Some(document) = response.document {
            self.cancel_reference_stretch();
            let before = self.tab.doc.clone();
            self.tab.doc = document.clone();
            self.changed(before);
            // The layout repair below moves what the labels' ink pushes.
            reshiki::atom_labels::copy_computed(&mut self.tab.doc, &document);
            self.tab.label_generation = self.tab.label_generation.wrapping_add(1);
            self.settle_label_layout();
            self.tab.labels_dirty = false;
            self.tab.chemistry_notice = None;
            self.tab.selected.clear();
            if matches!(kind, Job::Import | Job::ImportFile(_)) {
                self.fit();
            }
            if let Job::ImportFile(name) = &kind {
                self.tab.path = None;
                self.tab.untitled_name = Some(name.clone());
                self.tab.mark_saved(Document::default());
                self.begin_file_epoch();
            }
        }
        self.tab.analysis = response.analysis;
        self.status = match kind {
            Job::Clean(_) => "Structure cleaned",
            Job::Analyze => "No chemistry errors found",
            _ => "Structure imported · Undo restores the previous drawing",
        }
        .into();
        if !response.warnings.is_empty() {
            self.status.push_str(" · ");
            self.status.push_str(&response.warnings.join(" · "));
        }
        self.error = false;
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cancel_tests;

#[cfg(test)]
mod crystal_tests;

#[cfg(test)]
mod import_file_tests;
