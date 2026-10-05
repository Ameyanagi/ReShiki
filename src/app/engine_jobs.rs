//! Chemistry engine jobs: launching requests and applying their results.
use super::{App, CleanupPreview, Job, Message, cleanup::CleanupJob, export_file, input_request};
use crate::canvas::Tool;
use iced::Task;
use reshiki::{
    document::{Document, Point},
    editing,
    engine::{ChemistryEngine, Request, Response},
};

impl App {
    pub(super) fn run(&mut self, request: Request, kind: Job) -> Task<Message> {
        if self.tab.busy {
            return Task::none();
        }
        self.tab.busy = true;
        self.error = false;
        self.status = "Working…".into();
        let engine = self.engine.clone();
        let revision = self.tab.revision;
        let aromatic_selection = matches!(kind, Job::AromaticDisplay);
        Task::perform(
            async move {
                if aromatic_selection {
                    super::shortcuts::aromatic_selection(engine, request).await
                } else {
                    engine.execute(request).await
                }
            },
            move |result| Message::EngineDone {
                revision,
                kind: kind.clone(),
                result: Box::new(result),
            },
        )
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
        self.run(input_request(&self.imports.input.text()), Job::Insert)
    }
    pub(super) fn import_input(&mut self) -> Task<Message> {
        self.run(input_request(&self.imports.input.text()), Job::Import)
    }
    pub(super) fn insert_example(&mut self, smiles: &'static str) -> Task<Message> {
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
        if matches!(&kind, Job::Clean(job) if job.serial != self.tab.cleanup_serial || job.epoch != self.tab.file_epoch)
        {
            return Task::none();
        }
        let response = match result {
            Err(e) => {
                self.error = true;
                self.status = e;
                return Task::none();
            }
            Ok(response) => response,
        };
        if let Job::Export(format) = kind {
            return export_file(response.output.unwrap_or_default(), format);
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
            kind @ (Job::Import | Job::ImportFile | Job::Export(_)) => {
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
    fn structure_inserted(&mut self, response: Response) {
        if let Some(document) = response.document {
            let before = self.tab.doc.clone();
            let center = editing::center(&document, &document.all_ids());
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
            self.tab.selected = editing::append(&mut self.tab.doc, &document, offset);
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
            reshiki::atom_labels::refresh_computed(&mut self.tab.doc, &document);
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
        // Checking is a read-only chemistry operation. Refresh
        // computed H labels without rewriting the user's bond
        // orders/stereo or inserting a step into Undo/Redo.
        if let Some(document) = response.document {
            reshiki::atom_labels::refresh_computed(&mut self.tab.doc, &document);
        }
        self.tab.analysis = response.analysis;
        self.tab.chemistry_notice = None;
        self.status = "No chemistry errors found".into();
        self.error = false;
    }
    fn structure_replaced(&mut self, kind: Job, response: Response) {
        if let Some(document) = response.document {
            let before = self.tab.doc.clone();
            self.tab.doc = document.clone();
            self.changed(before);
            reshiki::atom_labels::refresh_computed(&mut self.tab.doc, &document);
            self.tab.labels_dirty = false;
            self.tab.chemistry_notice = None;
            self.tab.selected.clear();
            if matches!(kind, Job::Import | Job::ImportFile) {
                self.fit();
            }
            if matches!(kind, Job::ImportFile) {
                self.tab.path = None;
                self.tab.untitled_name = None;
                self.tab.saved = Document::default();
                self.tab.file_epoch = self.next_epoch();
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
